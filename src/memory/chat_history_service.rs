use crate::dao::chat_history_dao::ChatHistoryDao;
use crate::dao::chat_history_vec_dao::ChatHistoryVecDao;
use crate::llm::{Message, Role};
use crate::memory::rag_embedder::{self, RagConfig, RagModel, RagRuntimeConfig};
use anyhow::anyhow;
use std::sync::LazyLock;
use tracing::{error, info};

#[cfg(test)]
use std::path::PathBuf;

#[cfg(not(test))]
static CHAT_HISTORY_DAO: LazyLock<anyhow::Result<ChatHistoryDao>> =
    LazyLock::new(ChatHistoryDao::new);
#[cfg(test)]
static CHAT_HISTORY_DAO: LazyLock<anyhow::Result<ChatHistoryDao>> =
    LazyLock::new(|| ChatHistoryDao::with_path(test_db_path()));

#[cfg(test)]
static TEST_DB_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    std::env::temp_dir().join(format!("arknights_chat_history_service_{nanos}.db"))
});

fn chat_history_dao() -> anyhow::Result<&'static ChatHistoryDao> {
    CHAT_HISTORY_DAO.as_ref().map_err(|err| anyhow!("{err:#}"))
}

#[cfg(not(test))]
fn chat_history_vec_dao(config: &RagRuntimeConfig) -> anyhow::Result<ChatHistoryVecDao> {
    ChatHistoryVecDao::new(config.model.dimension())
}

#[cfg(test)]
fn chat_history_vec_dao(config: &RagRuntimeConfig) -> anyhow::Result<ChatHistoryVecDao> {
    ChatHistoryVecDao::with_path(test_db_path(), config.model.dimension())
}

#[cfg(test)]
fn test_db_path() -> &'static PathBuf {
    &TEST_DB_PATH
}

pub async fn save_chat_history(user_content: &str, assistant_content: &str) -> anyhow::Result<i64> {
    let dao = chat_history_dao()?;
    let id = dao.insert(user_content, assistant_content).await?;
    spawn_index_chat_history(id, user_content.to_string(), assistant_content.to_string());
    Ok(id)
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

fn spawn_index_chat_history(chat_history_id: i64, user_content: String, assistant_content: String) {
    let config = match RagConfig::from_env() {
        Ok(RagConfig::Enabled(config)) => config,
        Ok(RagConfig::Disabled) => {
            info!(
                event = "rag_index_skipped",
                chat_history_id,
                reason = "model_not_configured"
            );
            return;
        }
        Err(err) => {
            error!(event = "rag_index_failed", chat_history_id, error = %err);
            return;
        }
    };

    info!(
        event = "rag_index_schedule",
        chat_history_id,
        model = config.model.model_name(),
        cache_dir = %config.cache_dir.display()
    );

    tokio::spawn(async move {
        match index_chat_history(chat_history_id, config, user_content, assistant_content).await {
            Ok(dimension) => {
                info!(event = "rag_index_success", chat_history_id, dimension);
            }
            Err(err) => {
                error!(event = "rag_index_failed", chat_history_id, error = %err);
            }
        }
    });
}

async fn index_chat_history(
    chat_history_id: i64,
    config: RagRuntimeConfig,
    user_content: String,
    assistant_content: String,
) -> anyhow::Result<usize> {
    let embedding = rag_embedder::embed_chat_history(config.clone(), &user_content, &assistant_content)
        .await?;
    let dimension = embedding.len();
    let dao = chat_history_vec_dao(&config)?;
    dao.upsert_embedding(chat_history_id, embedding).await?;
    Ok(dimension)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::rag_embedder;
    use std::sync::OnceLock;
    use serde_json::Value;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

    #[tokio::test]
    async fn save_chat_history_persists_pair_and_returns_positive_id() {
        let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
        disable_rag_for_test();
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
        let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
        disable_rag_for_test();
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
        let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
        disable_rag_for_test();
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

    #[tokio::test]
    async fn save_chat_history_skips_rag_when_disabled() {
        let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
        rag_embedder::clear_test_embedding_mode();
        unsafe {
            std::env::remove_var("ARKNIGHTS_RAG_MODEL");
        }

        let token = unique_token("disabled");
        let id = save_chat_history(&format!("user-{token}"), &format!("assistant-{token}"))
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert!(id > 0);
        assert!(!test_chat_history_vec_dao(RagModel::BgeSmallEnV15)
            .unwrap()
            .has_embedding(id)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn save_chat_history_indexes_embedding_in_background() {
        let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("ARKNIGHTS_RAG_MODEL", "BAAI/bge-small-en-v1.5");
        }
        rag_embedder::set_test_embedding_success(vec![0.25; 384]);

        let token = unique_token("rag");
        let id = save_chat_history(&format!("user-{token}"), &format!("assistant-{token}"))
            .await
            .unwrap();

        wait_for_embedding(id, RagModel::BgeSmallEnV15).await;

        assert!(test_chat_history_vec_dao(RagModel::BgeSmallEnV15)
            .unwrap()
            .has_embedding(id)
            .await
            .unwrap());
        rag_embedder::clear_test_embedding_mode();
    }

    #[tokio::test]
    async fn save_chat_history_returns_success_when_rag_model_is_invalid() {
        let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
        rag_embedder::clear_test_embedding_mode();
        unsafe {
            std::env::set_var("ARKNIGHTS_RAG_MODEL", "invalid-model");
        }

        let token = unique_token("invalid-model");
        let id = save_chat_history(&format!("user-{token}"), &format!("assistant-{token}"))
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert!(id > 0);
        assert!(!test_chat_history_vec_dao(RagModel::BgeSmallEnV15)
            .unwrap()
            .has_embedding(id)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn save_chat_history_emits_rag_log_events() {
        let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
        init_test_logging();

        unsafe {
            std::env::set_var("ARKNIGHTS_RAG_MODEL", "BAAI/bge-small-en-v1.5");
        }
        rag_embedder::set_test_embedding_success(vec![0.5; 384]);
        let success_id = save_chat_history("log-success-user", "log-success-assistant")
            .await
            .unwrap();
        wait_for_embedding(success_id, RagModel::BgeSmallEnV15).await;
        rag_embedder::clear_test_embedding_mode();

        unsafe {
            std::env::remove_var("ARKNIGHTS_RAG_MODEL");
        }
        let skipped_id = save_chat_history("log-skip-user", "log-skip-assistant")
            .await
            .unwrap();

        unsafe {
            std::env::set_var("ARKNIGHTS_RAG_MODEL", "invalid-model");
        }
        let failed_id = save_chat_history("log-fail-user", "log-fail-assistant")
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let logs = std::fs::read_to_string("logs/arknights.log").unwrap();
        assert!(logs.contains("rag_index_schedule"));
        assert!(logs.contains(&format!("chat_history_id={success_id}")));
        assert!(logs.contains("cache_dir="));
        assert!(logs.contains("rag_index_success"));
        assert!(logs.contains("rag_index_skipped"));
        assert!(logs.contains(&format!("chat_history_id={skipped_id}")));
        assert!(logs.contains("reason=\"model_not_configured\""));
        assert!(logs.contains("rag_index_failed"));
        assert!(logs.contains(&format!("chat_history_id={failed_id}")));
        assert!(logs.contains("unsupported ARKNIGHTS_RAG_MODEL"));
    }

    fn unique_token(label: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        format!("chat-history-service-{label}-{nanos}")
    }

    fn disable_rag_for_test() {
        rag_embedder::clear_test_embedding_mode();
        unsafe {
            std::env::remove_var("ARKNIGHTS_RAG_MODEL");
        }
    }

    fn init_test_logging() {
        TEST_LOG_GUARD.get_or_init(|| {
            std::fs::create_dir_all("logs").unwrap();
            let _ = std::fs::remove_file("logs/arknights.log");

            let appender = tracing_appender::rolling::never("logs", "arknights.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(appender);
            let _ = tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(non_blocking)
                .try_init();

            guard
        });
    }

    fn test_chat_history_vec_dao(model: RagModel) -> anyhow::Result<ChatHistoryVecDao> {
        ChatHistoryVecDao::with_path(test_db_path(), model.dimension())
    }

    async fn wait_for_embedding(chat_history_id: i64, model: RagModel) {
        for _ in 0..20 {
            if test_chat_history_vec_dao(model)
                .unwrap()
                .has_embedding(chat_history_id)
                .await
                .unwrap()
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        panic!("timed out waiting for background rag indexing");
    }
}
