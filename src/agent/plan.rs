use crate::agent::notify_check::make_notify_choice;
use crate::agent::{ReAct, personal};
use crate::kv::kv_service;
use crate::llm::Message;
use crate::llm::Role;
use crate::llm::base_llm::{FunctionCall, Llm, ToolCall};
use crate::{im, memory, timer};
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
- system: System-related(`date`, `bash`) info or operations
- internet: Internet-related(`web_search`, `curl`) operations
- memory: Memory is only for chat history, semantic memory recall, and user profile retrieval.
  Use it for previous conversations, long-term memory lookup, and user profile data.
  Do NOT use memory as a substitute for persisted schedule/calendar/event records.
- timer: Timer-related(CRUD timer task, used to invoke the agent) operations
- schedule: Persistent user schedule/calendar event operations.
  Use it for schedule, calendar, meeting, itinerary, and event queries.
  This group is the source of truth for saved schedule-event records.
- browser: Browser-related(`navigate`, `snapshot`, `click`, `fill`, `scroll`, `wait_text`, `get_text`, `get_html`, `screenshot`, `close`) operations

## Decision Rules
1. If the question has NOT been fully answered yet:
   - Decompose it into ordered subtasks.
   - Put the union of required tool groups into `tools`.
   - Set `is_done` to false and omit `content`.
2. If the question HAS been fully answered:
   - Set `is_done` to true.
   - Write the final answer in `content` with `text` and `files`(no file set empty list).
3. If the current plan failed or is insufficient:
   - Reformulate a new plan with alternative subtasks and tools.
   - Set `is_done` to false and omit `content`.
4. For questions involving relative time, be sure to check the current time first.
5. Each subtask MUST contain the necessary information and be able to be completed independently.
6. If the user asks about schedule/calendar/event records, you MUST include `schedule`.
7. For relative-date schedule queries such as "What is on my schedule today?" or "What is on my schedule tomorrow?", you MUST include both `system` and `schedule`.
8. Do not route schedule/calendar/event-record queries to `memory` unless the user explicitly asks about past conversations or remembered preferences.

## Language Rule
`content.text`, `expand_goal` and every `plan` field must be written in the same language as the user's message.
"#;

const JSON_FORMAT: &str = r#"
// unfinished
{
    "expand_goal": "Find all schedule events saved for today.",
    "plans": [
        "Check the current date.",
        "Query saved schedule events for today's full-day time range."
    ],
    "tools": ["system", "schedule"],
    "is_done": false
}

// finished
{
    "expand_goal": "<expanded question>",
    "plans": [
        "<subtask description>",
        "<subtask description>"
    ],
    "tools": ["system", "schedule"],
    "content": {
        "text": "<final answer text>",
        "files": [
          {
            "type": "<file type>",
            "name": "<file name>",
            "path": "<file path>"
          }
        ]
    },
    "is_done": true
}
"#;

/// agent plan module
pub struct Plan {
    question: String,
    plans: Vec<String>,
    tools: HashSet<String>,
    llm: Llm,
    // if first plan can answer
    answer: Option<Content>,
}

#[derive(Deserialize, Debug)]
pub struct PlanResp {
    plans: Vec<String>,
    tools: HashSet<String>,
    #[serde(default)]
    is_done: bool,
    content: Option<Content>,
    expand_goal: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Content {
    pub text: String,
    pub files: Vec<File>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct File {
    pub r#type: String,
    pub name: String,
    pub path: String,
}

impl Plan {
    pub async fn new(user_message: String) -> anyhow::Result<Self> {
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
                    return if plan_resp.content.is_some() {
                        Ok(Plan {
                            question: plan_resp.expand_goal,
                            plans: vec![],
                            tools: HashSet::new(),
                            llm,
                            answer: plan_resp.content,
                        })
                    } else {
                        Err(anyhow!("llm response is empty"))
                    };
                }

                // send expand goal
                send_process_message(plan_resp.expand_goal.clone());

                Ok(Plan {
                    question: plan_resp.expand_goal,
                    plans: plan_resp.plans,
                    tools: plan_resp.tools,
                    llm,
                    answer: None,
                })
            }
            None => Err(anyhow!("llm response is empty")),
        }
    }

    pub async fn execute(&mut self) -> anyhow::Result<String> {
        if let Some(answer) = self.answer.take() {
            return send_final_answer(self.question.clone(), answer).await;
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
                send_process_message(plan.clone() + " Done");

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
                        return match plan_resp.content {
                            Some(c) => send_final_answer(self.question.clone(), c).await,
                            None => Err(anyhow!("llm response is empty")),
                        };
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

async fn send_final_answer(question: String, content: Content) -> anyhow::Result<String> {
    // check notify values
    if let Some(timer_id) = timer::timer_service::get_thread_local_timer_id() {
        let is_notify = make_notify_choice(content.text.clone(), timer_id).await?;
        if !is_notify {
            return Ok(content.text);
        }
    }

    // save chat history
    match memory::chat_history_service::save_chat_history(&question, &content.text).await {
        Ok(_) => {}
        Err(err) => error!("Failed to save chat history: {}", err),
    }

    match personal::personal_message(content.text.clone()).await {
        Ok(cs) => {
            // send personal answers
            for c in &cs {
                im::base_im::async_send_text(c.clone());
            }

            // send files
            im::base_im::async_send_files(content.files.clone());
            Ok(cs.join("\n"))
        }
        Err(err) => {
            error!("Failed to personalize message: {}", err);
            im::base_im::async_send_text(content.text.clone());
            im::base_im::async_send_files(content.files.clone());
            Ok(content.text)
        }
    }
}

fn send_process_message(content: String) {
    if timer::timer_service::get_thread_local_timer_id().is_some() {
        return;
    }

    im::base_im::async_send_text(content);
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
