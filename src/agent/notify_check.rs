use crate::kv::kv_service;
use crate::llm::{Message, Role};
use crate::{llm, timer};
use anyhow::anyhow;
use serde::Deserialize;
use tracing::error;

const NOTIFY_CHECK_PROMPT: &str = r#"
You are a reminder delivery judge.

### Task:
Decide whether the current scheduled reminder result should be delivered to the user.

### You will receive:
- the user profile
- the timer task info, including the original task prompt and schedule metadata
- the previous delivered result for this timer task
- the current result that would be delivered now

### Decision rules:
- First infer the timer task intent from the timer task info:
  - recurring reminder task: the purpose is to remind the user to do something repeatedly on schedule
  - update/report task: the purpose is to notify the user only when there is meaningful new information or status change
- For update/report tasks, return `notify: false` if the current result is not materially different from the previous result.
- For recurring reminder tasks, similarity to the previous result alone is not enough to suppress delivery.
- Return `notify: false` only when the current result is clearly redundant, empty, invalid, or adds no practical value for this
timer task.
- Return `notify: true` when the current result provides a meaningful user-relevant reminder, a meaningful status change, or
important new information.
- Use the user profile only as a relevance hint. Do not invent missing context.
- Judge only from the provided inputs.
- Return exactly one valid JSON object.
"#;

const JSON_FORMAT: &str = r#"
{
    "notify": false
}
"#;

#[derive(Debug, Deserialize)]
struct NotifyCheckResp {
    notify: bool,
}

pub async fn make_notify_choice(message: String, task_id: String) -> anyhow::Result<bool> {
    // select previous result
    let timer = match timer::timer_service::get_by_id(task_id.clone()).await? {
        Some(timer) => timer,
        None => {
            error!("Task {} not found", task_id);
            return Ok(false);
        }
    };
    let pre_message = match timer.last_result {
        Some(r) => r,
        None => return Ok(true),
    };

    // set system prompt
    let system_prompt = build_system_prompt().await?;
    let system = Message::new(Role::System, system_prompt);
    let timer_info = Message::new(
        Role::User,
        format!(
            "Timer task info:\n- prompt: {}\n- cron_expr: {}\n- remaining_runs: {}\n- next_trigger_at: {}\n- last_completed_at: {}",
            timer.prompt,
            timer.cron_expr,
            timer.remaining_runs,
            timer.next_trigger_at.as_deref().unwrap_or("null"),
            timer.last_completed_at.as_deref().unwrap_or("null"),
        ),
    );
    let pre_message = Message::new(
        Role::User,
        format!("Previous delivered result: \n{pre_message}"),
    );
    let cur_message = Message::new(
        Role::User,
        format!("Current result to evaluate: \n{message}"),
    );

    let messages = vec![system, timer_info, pre_message, cur_message];

    // make notify choice
    let mut llm = llm::base_llm::Llm::new(messages, vec![]);
    let chat_resp = llm.call().await?;
    match chat_resp.choices.first() {
        Some(choice) => {
            let notify_check_resp: NotifyCheckResp = serde_json::from_str(&choice.message.content)?;
            Ok(notify_check_resp.notify)
        }
        None => Err(anyhow!("notify choice response is empty")),
    }
}

async fn build_system_prompt() -> anyhow::Result<String> {
    let user_profile = kv_service::get_user_profile().await?;

    Ok(format!(
        r#"{NOTIFY_CHECK_PROMPT}
    
        ## User profile
        {user_profile}

        ## Output Format Json
        {JSON_FORMAT}"#
    ))
}

#[cfg(test)]
#[path = "notify_check_tests.rs"]
mod tests;
