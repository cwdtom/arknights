use crate::agent::{ReAct, personal};
use crate::kv::kv_service;
use crate::llm::Message;
use crate::llm::Role;
use crate::llm::base_llm::{FunctionCall, Llm, ToolCall};
use crate::{im, memory};
use anyhow::anyhow;
use chrono::Local;
use rand::distr::{Alphanumeric, SampleString};
use serde::Deserialize;
use std::collections::HashSet;
use tracing::error;

const MAX_TURNS: u8 = 20;
const PLAN_PROMPT: &str = r#"
You are the PLAN or REPLAN node in a Plan-ReAct-Replan pipeline.

## Role
First, from `user profile` phrase expand user's colloquial question into a complete and unambiguous one.
Second, select appropriate tools and put them into `tools`.
Then, given the expanded question and any previous execution results, produce a structured plan that guides downstream ReAct nodes to find the answer.

## Available Tools
- system: System-related info or operations
- internet: Internet-related operations
- memory: Memory-related search

## Decision Rules
1. If the question has NOT been fully answered yet:
   - Decompose it into ordered subtasks.
   - Assign relevant tools to each subtask.
   - Set `is_done` to false and leave `content` empty.
2. If the question HAS been fully answered:
   - Set `is_done` to true.
   - Write the final answer in `content`.
3. If the current plan failed or is insufficient:
   - Reformulate a new plan with alternative subtasks and tools.
   - Set `is_done` to false and leave `content` empty.
4. For questions involving relative time, be sure to check the current time first.
5. Each subtask MUST contain the necessary information and be able to be completed independently.

## Language Rule
`content`, `expand_goal` and every `plan` field must be written in the same language as the user's message.
"#;

const JSON_FORMAT: &str = r#"{
  "expand_goal": "<expanded question>",
  "plans": [
    "<subtask description>",
    "<subtask description>"
  ],
  "tools": ["internet", "memory"],
  "content": "<final answer>",
  "is_done": false
}
"#;

/// agent plan module
pub struct Plan {
    question: String,
    plans: Vec<String>,
    tools: HashSet<String>,
    llm: Llm,
    // if first plan can answer
    answer: Option<String>,
    is_timer_flow: bool,
}

#[derive(Deserialize, Debug)]
pub struct PlanResp {
    plans: Vec<String>,
    tools: HashSet<String>,
    #[serde(default)]
    is_done: bool,
    content: String,
    expand_goal: String,
}

impl Plan {
    pub async fn new(user_message: String, is_timer_flow: bool) -> anyhow::Result<Self> {
        // set system prompt
        let system = Message::new(
            Role::System,
            build_system_prompt(&kv_service::get_user_profile().await?),
        );
        let mut messages = vec![system];

        // set history chat
        let history = memory::chat_history_service::build_chat_history_messages(20).await?;
        messages.extend(history);

        // make user message
        let now = Local::now().to_rfc3339();
        let user = Message::new(Role::User, format!("[{now}] {user_message}"));
        messages.push(user);

        // make plan
        let mut llm = Llm::new(messages, vec![]);
        let chat_resp = llm.call().await?;
        match chat_resp.choices.first() {
            Some(choice) => {
                let plan_resp: PlanResp = serde_json::from_str(&choice.message.content)?;
                llm.push_message(choice.message.clone());

                // plan already answer
                if plan_resp.is_done {
                    return Ok(Plan {
                        question: plan_resp.expand_goal,
                        plans: vec![],
                        tools: HashSet::new(),
                        llm,
                        answer: Some(plan_resp.content),
                        is_timer_flow,
                    });
                }

                // send expand goal
                send_process_message(plan_resp.expand_goal.clone(), is_timer_flow);

                Ok(Plan {
                    question: plan_resp.expand_goal,
                    plans: plan_resp.plans,
                    tools: plan_resp.tools,
                    llm,
                    answer: None,
                    is_timer_flow,
                })
            }
            None => Err(anyhow!("llm response is empty")),
        }
    }

    pub async fn execute(&mut self) -> anyhow::Result<String> {
        if let Some(answer) = self.answer.take() {
            return send_final_answer(self.question.clone(), answer, self.is_timer_flow).await;
        }

        let mut re_act_history: Vec<Message> = vec![];

        for _ in 1..=MAX_TURNS {
            for plan in &self.plans {
                // set sub task
                let sub_message = Message::new(Role::User, plan.clone());
                re_act_history.push(sub_message);

                // init reAct
                let mut re_act = ReAct::new(re_act_history.clone(), self.tools.clone())?;
                let re_act_resp = re_act.execute().await?;
                // set sub answer, fake tool call
                let (tool_call, tool_result) =
                    build_tool_call_message(plan.clone(), re_act_resp.content);
                self.llm.push_message(tool_call.clone());
                self.llm.push_message(tool_result.clone());
                re_act_history.push(tool_call.clone());
                re_act_history.push(tool_result.clone());

                // send reAct answer
                send_process_message(plan.clone() + " Done", self.is_timer_flow);

                if re_act_resp.needs_replan {
                    break;
                }
            }

            // REPLAN
            let chat_resp = self.llm.call().await?;
            match chat_resp.choices.first() {
                Some(choice) => {
                    let plan_resp: PlanResp = serde_json::from_str(&choice.message.content)?;
                    self.question = plan_resp.expand_goal.clone();

                    if plan_resp.is_done {
                        return send_final_answer(
                            self.question.clone(),
                            plan_resp.content,
                            self.is_timer_flow,
                        )
                        .await;
                    } else {
                        // update plans
                        self.llm.push_message(choice.message.clone());
                        self.plans = plan_resp.plans;
                        self.question = plan_resp.expand_goal;
                        self.tools = plan_resp.tools;

                        continue;
                    }
                }
                None => return Err(anyhow!("llm response is empty")),
            }
        }

        Err(anyhow!("plan exceeded max turns ({MAX_TURNS})"))
    }
}

fn build_system_prompt(user_profile: &str) -> String {
    format!(
        r#"{}

            ## User profile
            {}

            ## Output Format Json
            {}
            "#,
        PLAN_PROMPT, user_profile, JSON_FORMAT
    )
}

async fn send_final_answer(
    question: String,
    content: String,
    is_timer_flow: bool,
) -> anyhow::Result<String> {
    // check notify values
    if is_timer_flow {
        // TODO, now all send
        im::base_im::async_send(content.clone());
        return Ok(content);
    }

    // save chat history
    match memory::chat_history_service::save_chat_history(question.as_str(), content.as_str()).await
    {
        Ok(_) => {}
        Err(err) => error!("Failed to save chat history: {}", err),
    }

    match personal::personal_message(content.clone()).await {
        Ok(c) => Ok(c),
        Err(err) => {
            error!("Failed to personalize message: {}", err);
            im::base_im::async_send(content.clone());
            Ok(content)
        }
    }
}

fn send_process_message(content: String, is_timer_flow: bool) {
    if is_timer_flow {
        return;
    }

    im::base_im::async_send(content);
}

/// fake build tool call, return tool call and tool result
fn build_tool_call_message(question: String, result: String) -> (Message, Message) {
    let id = format!(
        "call_00_{}",
        Alphanumeric.sample_string(&mut rand::rng(), 24)
    );
    let function_call = FunctionCall {
        name: "reAct".to_string(),
        arguments: serde_json::json!({
            "question": question
        })
        .to_string(),
    };
    let tool_call = ToolCall {
        id: id.clone(),
        r#type: "function".to_string(),
        function: function_call,
    };
    let tool_call_message = Message {
        role: Role::Assistant,
        content: "".to_string(),
        tool_call_id: None,
        tool_calls: Some(vec![tool_call]),
    };
    let tool_result_message = Message {
        role: Role::Tool,
        content: result,
        tool_call_id: Some(id),
        tool_calls: None,
    };

    (tool_call_message, tool_result_message)
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
