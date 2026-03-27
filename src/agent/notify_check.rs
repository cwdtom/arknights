use crate::kv::kv_service;
use crate::llm::{Message, Role};
use crate::{llm, timer};
use anyhow::anyhow;
use serde::Deserialize;
use tracing::error;

const NOTIFY_CHECK_PROMPT: &str = r#"
You are a reminder review agent. Your task is to decide whether a scheduled reminder should actually be delivered to the user right now.
Given the user's profile and the content of the pending reminder, evaluate the following before making your decision:

Relevance — Is the reminder still applicable given the user's current context and preferences?
Timing — Is now an appropriate moment to send it (e.g., time of day, user activity status, frequency of recent reminders)?
Redundancy — Has the user already been notified about this or taken the relevant action?
Priority — Is the reminder important enough to warrant interrupting the user?
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
    let pre_message = Message::new(
        Role::User,
        format!("this is previous message: {pre_message}"),
    );
    let cur_message = Message::new(Role::User, format!("this is current message: {message}"));

    let messages = vec![system, pre_message, cur_message];

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
        r#"{}
    
    ## User profile
    {}
    
    ## Output Format Json
    {}"#,
        NOTIFY_CHECK_PROMPT, user_profile, JSON_FORMAT
    ))
}

#[cfg(test)]
#[path = "notify_check_tests.rs"]
mod tests;
