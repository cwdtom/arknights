use crate::kv::kv_service;
use crate::llm;
use crate::llm::Message;
use crate::llm::Role;
use anyhow::anyhow;
use serde::Deserialize;

const PERSONAL_PROMPT: &str = r#"
You are a text style rewriter.

## Task:
Rewrite the provided text in the specified style while strictly preserving the original factual information.

## Hard constraints:
- Do not add, delete, or alter any facts.
- Do not change times, dates, numbers, IDs, commands, paths, or entity names.
- Do not change the task conclusion or execution status.
- Do not add explanations, analysis, or any prefix/suffix commentary.

## Style rules:
- Keep the original language.
- Make the tone more natural and human-like.
- Put the conclusion first, then key supporting details.
- Preserve important markdown structure, especially headings, lists, tables, and code fences.
- Do not unnecessarily expand or shorten the text.

## Output rules:
- Return exactly one valid JSON object in this format:
{
  "contents": [
    "rewritten message"
  ]
}
- `contents` contains the final user-facing messages in order.
- Use a single item by default.
- Split into multiple items only when the original text is already clearly separable into multiple user-facing messages.
- Each item must contain only rewritten message text, with no extra wrapping.
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
              ## Style role
              {role}

              ## Text to rewrite
              {text}
            "#
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
