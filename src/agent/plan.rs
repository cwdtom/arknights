use crate::agent::notify_check::make_notify_choice;
use crate::agent::{ReAct, personal};
use crate::kv::kv_service;
use crate::llm::Message;
use crate::llm::Role;
use crate::llm::base_llm::{FunctionCall, Llm, ToolCall};
use crate::{im, memory, timer};
use anyhow::anyhow;
use rand::distr::{Alphanumeric, SampleString};
use serde::Deserialize;
use std::collections::HashSet;
use tracing::{error, warn};

const MAX_TURNS: u8 = 20;
const PLAN_PROMPT: &str = r#"
You are the PLAN or REPLAN node in a Plan-ReAct-Replan pipeline.

## Highest Priority
- Return ONLY one valid json object.

## Role
First, from `user profile` phrase expand user's colloquial question into a complete and unambiguous one.
Then:
- If the question is already fully answerable from the current context, return the final answer.
- Otherwise, select the minimal required tools and produce an ordered plan for downstream ReAct nodes.

## Tool Boundaries
- system: Use for current local time or system-level operations.
- internet: Use for external web search or fetching content from URLs.
- memory: Use only for chat history, semantic recall, and user profile retrieval. It is not the source of truth for persisted
schedule/calendar/event records.
- timer: Use for timer-task(not for user) CRUD that triggers the agent later. It is not for calendar or schedule-event records.
- schedule: Use for persisted schedule, calendar, meeting, itinerary, and event records. This is the source of truth for saved
schedule-event data.
- browser: Use for interactive webpage navigation, reading, and actions when page state or DOM interaction matters.

## Decision Rules
- If the question is NOT yet fully answered, set `is_done` to false, omit `content`, produce ordered subtasks, and put the minimal required tool groups into `tools`.
- If previous execution was insufficient, still set `is_done` to false and produce a better plan instead of repeating the failed route.
- If the question HAS been fully answered:
  - Set `is_done` to true.
  - Write the final answer in `content` with `text` and `files` (use an empty list if there are no files).
- Each subtask must be directly executable for its step, include all critical external inputs not already available from the user
  message or prior subtask outputs, and state the expected result clearly. Subtasks may depend on outputs from earlier subtasks in the same plan.
- Do not guess a concrete current date or time, use `system` tool get system datetime.
- For relative-time questions, include `system` when downstream execution must verify or compute current or relative time.
- For relative-date schedule queries such as "What is on my schedule today?" or "What is on my schedule tomorrow?", you MUST include
both `system` and `schedule`.
- If the message contains a URL, be sure to include that URL in every subtask.
- For schedule, calendar, meeting, itinerary, or event-record queries, you MUST include `schedule`.
- Do not route schedule/calendar/event-record queries to `memory` unless the user explicitly asks about past conversations or
  remembered preferences.

## Language Rule
`content.text`, `expand_goal` and every `plan` field must be written in the same language as the user's message.
"#;

const JSON_FORMAT: &str = r#"
{
    "expand_goal": "<expanded question>",
    "plans": [
        "<sub task 1>",
        "<sub task 2>"
    ],
    "tools": ["system", "schedule", "tool 3"],
    "is_done": false
}

{
    "expand_goal": "<expanded question>",
    "plans": [],
    "tools": [],
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
        let user = Message::new(Role::User, user_message.clone());
        messages.push(user);

        // make plan
        let mut llm = Llm::new(messages, vec![]);
        let chat_resp = llm.call().await?;
        match chat_resp.choices.first() {
            Some(choice) => {
                let plan_resp: PlanResp = match serde_json::from_str(&choice.message.content) {
                    Ok(resp) => resp,
                    Err(err) => {
                        error!("plan response parse error: {}", err);
                        // It is highly likely that the JSON format was not followed,
                        // and the response was given directly.
                        return Ok(Plan {
                            question: user_message,
                            plans: vec![],
                            tools: HashSet::new(),
                            llm,
                            answer: Some(Content {
                                text: choice.message.content.clone(),
                                files: Vec::new(),
                            }),
                        });
                    }
                };
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

            ## Output Format json
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
            warn!("Failed to personalize message: {}", err);
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
