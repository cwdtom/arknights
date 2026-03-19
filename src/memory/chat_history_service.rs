use crate::dao::chat_history_dao::ChatHistoryDao;
use crate::llm::{Message, Role};
use anyhow::anyhow;
use std::sync::LazyLock;

#[cfg(not(test))]
static CHAT_HISTORY_DAO: LazyLock<anyhow::Result<ChatHistoryDao>> =
    LazyLock::new(ChatHistoryDao::new);
#[cfg(test)]
static CHAT_HISTORY_DAO: LazyLock<anyhow::Result<ChatHistoryDao>> =
    LazyLock::new(|| ChatHistoryDao::with_path(":memory:"));

fn chat_history_dao() -> anyhow::Result<&'static ChatHistoryDao> {
    CHAT_HISTORY_DAO.as_ref().map_err(|err| anyhow!("{err:#}"))
}

pub async fn save_chat_history(user_content: &str, assistant_content: &str) -> anyhow::Result<i64> {
    let dao = chat_history_dao()?;
    dao.insert(user_content, assistant_content).await
}

pub async fn build_chat_history_messages(limit: usize) -> anyhow::Result<Vec<Message>> {
    let dao = chat_history_dao()?;
    let mut histories = dao.list(limit, 0).await?;
    histories.reverse();

    let mut messages = Vec::with_capacity(histories.len() * 2);
    for history in histories {
        messages.push(Message::new(Role::User, history.user_content));
        messages.push(Message::new(Role::Assistant, history.assistant_content));
    }

    Ok(messages)
}

pub async fn fuzz_query(keywords: Vec<String>) -> anyhow::Result<String> {
    let dao = chat_history_dao()?;

    let mut histories = vec![];
    for k in keywords {
        let arr = dao.fuzzy_query(k.as_str(), 5, 0).await?;
        for a in arr {
            match serde_json::to_string(&a) {
                Ok(json) => histories.push(json),
                Err(_err) => continue,
            }
        }
    }

    Ok(histories.join("\n"))
}
