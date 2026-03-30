use crate::kv::kv_service;
use crate::llm;
use crate::llm::Message;
use crate::llm::Role;
use anyhow::anyhow;
use serde::Deserialize;

const PERSONAL_PROMPT: &str = r#"
You are a **text style rewriter**.
Task: rewrite the input text in a specified character’s style while strictly preserving the original factual information.

**Hard constraints (must be followed)**
1. Do not add, delete, or alter any facts.
2. Do not change times, dates, numbers, IDs, commands, or entity names.
3. Do not change the task’s conclusion or execution status.
4. Do not output explanations, analysis, or any prefix/suffix commentary—only output the final rewritten text.

**Style and expression requirements**
- Keep the original language; you may polish the tone and reorder expressions, but factual content must remain unchanged.
- Output length should be between 0.7 and 1.3 times the length of the original text.
- The tone should be more natural and human‑like: state the conclusion first, then add key details.
- Preserve the original markdown block structure whenever possible.

**Subtask handling**
- Output only the text body of the completed subtask, without adding any explanation, prefix/suffix, or extra wrapping.
- You may polish the tone, but the original subtask name and completion status must be fully preserved.

## Output Format Json
{
    "contents": [
        "first message",
        "second message"
    ]
}
"#;

#[derive(Debug, Deserialize)]
struct PersonalResp {
    contents: Vec<String>,
}

pub async fn personal_message(text: String) -> anyhow::Result<Vec<String>> {
    // set system prompt
    let system = Message::new(Role::System, PERSONAL_PROMPT.to_string());
    let mut messages = vec![system];

    // make user message
    let role = kv_service::get_personal_value().await?;
    let user = Message::new(
        Role::User,
        format!(
            r#"
                ## Role introduction that needs to be rewritten
                {}

                ## The following is the content that needs to be rewritten
                {}
            "#,
            role, text
        ),
    );
    messages.push(user);

    // make personal
    let mut llm = llm::base_llm::Llm::new(messages, vec![]);
    let chat_resp = llm.call().await?;
    match chat_resp.choices.first() {
        Some(choice) => {
            let personal_resp: PersonalResp = serde_json::from_str(&choice.message.content)?;

            Ok(personal_resp.contents)
        }
        None => Err(anyhow!("personal response is empty")),
    }
}

#[cfg(test)]
#[path = "personal_tests.rs"]
mod tests;
