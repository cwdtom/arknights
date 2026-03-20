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
    let histories = dao.list(limit, 0).await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn save_chat_history_persists_pair_and_returns_positive_id() {
        let token = unique_token("save");
        let user_content = format!("user-{token}");
        let assistant_content = format!("assistant-{token}");

        let id = save_chat_history(&user_content, &assistant_content)
            .await
            .unwrap();

        assert!(id > 0);

        let messages = build_chat_history_messages(100).await.unwrap();
        let matched_messages: Vec<_> = messages
            .into_iter()
            .filter(|message| message.content.contains(&token))
            .collect();

        assert_eq!(matched_messages.len(), 2);
        assert!(matches!(matched_messages[0].role, Role::User));
        assert_eq!(matched_messages[0].content, user_content);
        assert!(matches!(matched_messages[1].role, Role::Assistant));
        assert_eq!(matched_messages[1].content, assistant_content);
    }

    #[tokio::test]
    async fn build_chat_history_messages_returns_pairs_in_history_order() {
        let token = unique_token("build");
        let older_user = format!("older-user-{token}");
        let older_assistant = format!("older-assistant-{token}");
        let newer_user = format!("newer-user-{token}");
        let newer_assistant = format!("newer-assistant-{token}");

        save_chat_history(&older_user, &older_assistant).await.unwrap();
        save_chat_history(&newer_user, &newer_assistant).await.unwrap();

        let messages = build_chat_history_messages(100).await.unwrap();
        let matched_messages: Vec<_> = messages
            .into_iter()
            .filter(|message| message.content.contains(&token))
            .collect();

        assert_eq!(matched_messages.len(), 4);
        assert!(matches!(matched_messages[0].role, Role::User));
        assert_eq!(matched_messages[0].content, newer_user);
        assert!(matches!(matched_messages[1].role, Role::Assistant));
        assert_eq!(matched_messages[1].content, newer_assistant);
        assert!(matches!(matched_messages[2].role, Role::User));
        assert_eq!(matched_messages[2].content, older_user);
        assert!(matches!(matched_messages[3].role, Role::Assistant));
        assert_eq!(matched_messages[3].content, older_assistant);
    }

    #[tokio::test]
    async fn fuzz_query_keeps_matches_from_each_keyword_as_json_lines() {
        let token = unique_token("fuzz");
        let keyword_one = format!("keyword-one-{token}");
        let keyword_two = format!("keyword-two-{token}");
        let user_content = format!("user contains {keyword_one}");
        let assistant_content = format!("assistant contains {keyword_two}");

        save_chat_history(&user_content, &assistant_content)
            .await
            .unwrap();

        let joined = fuzz_query(vec![keyword_one, keyword_two]).await.unwrap();
        let lines: Vec<_> = joined.lines().collect();

        assert_eq!(lines.len(), 2);
        assert!(joined.contains('\n'));

        for line in lines {
            let value: Value = serde_json::from_str(line).unwrap();
            assert_eq!(value["user_content"], user_content);
            assert_eq!(value["assistant_content"], assistant_content);
            assert!(value["id"].as_i64().unwrap() > 0);
        }
    }

    fn unique_token(label: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        format!("chat-history-service-{label}-{nanos}")
    }
}
