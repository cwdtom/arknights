use super::*;
use crate::memory::rag_embedder::{
    RagEmbeddingBackend, RagModel, RagRuntimeConfig, SharedRagEmbeddingBackend,
};
use crate::test_support;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn chat_history_service_source_keeps_only_external_test_module_gate() {
    let source = include_str!("chat_history_service.rs");
    assert_eq!(source.matches("#[cfg(test)]").count(), 1);
}

#[tokio::test]
async fn save_chat_history_persists_pair_and_returns_positive_id() {
    let _guard = test_support::app_test_guard().await;
    disable_rag_for_test();
    let token = test_support::unique_test_token("chat-history-service", "save");
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
    let _guard = test_support::app_test_guard().await;
    disable_rag_for_test();
    let token = test_support::unique_test_token("chat-history-service", "build");
    let older_user = format!("older-user-{token}");
    let older_assistant = format!("older-assistant-{token}");
    let newer_user = format!("newer-user-{token}");
    let newer_assistant = format!("newer-assistant-{token}");

    save_chat_history(&older_user, &older_assistant)
        .await
        .unwrap();
    save_chat_history(&newer_user, &newer_assistant)
        .await
        .unwrap();

    let messages = build_chat_history_messages(100).await.unwrap();
    let matched_messages: Vec<_> = messages
        .into_iter()
        .filter(|message| message.content.contains(&token))
        .collect();

    assert_eq!(matched_messages.len(), 4);
    assert!(matches!(matched_messages[0].role, Role::User));
    assert_eq!(matched_messages[0].content, older_user);
    assert!(matches!(matched_messages[1].role, Role::Assistant));
    assert_eq!(matched_messages[1].content, older_assistant);
    assert!(matches!(matched_messages[2].role, Role::User));
    assert_eq!(matched_messages[2].content, newer_user);
    assert!(matches!(matched_messages[3].role, Role::Assistant));
    assert_eq!(matched_messages[3].content, newer_assistant);
}

#[tokio::test]
async fn build_chat_history_messages_skips_histories_older_than_24_hours() {
    let _guard = test_support::app_test_guard().await;
    disable_rag_for_test();
    let token = test_support::unique_test_token("chat-history-service", "ttl");
    let expired_user = format!("expired-user-{token}");
    let expired_assistant = format!("expired-assistant-{token}");
    let recent_user = format!("recent-user-{token}");
    let recent_assistant = format!("recent-assistant-{token}");

    insert_chat_history_with_created_at(
        &expired_user,
        &expired_assistant,
        &(chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339(),
    );
    insert_chat_history_with_created_at(
        &recent_user,
        &recent_assistant,
        &(chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339(),
    );

    let messages = build_chat_history_messages(100).await.unwrap();
    let matched_messages: Vec<_> = messages
        .into_iter()
        .filter(|message| message.content.contains(&token))
        .collect();

    assert_eq!(matched_messages.len(), 2);
    assert!(matches!(matched_messages[0].role, Role::User));
    assert_eq!(matched_messages[0].content, recent_user);
    assert!(matches!(matched_messages[1].role, Role::Assistant));
    assert_eq!(matched_messages[1].content, recent_assistant);
}

#[tokio::test]
async fn fuzz_query_keeps_matches_from_each_keyword_as_json_lines() {
    let _guard = test_support::app_test_guard().await;
    disable_rag_for_test();
    let token = test_support::unique_test_token("chat-history-service", "fuzz");
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
    let _guard = test_support::app_test_guard().await;
    test_support::disable_rag_for_test();

    let token = test_support::unique_test_token("chat-history-service", "disabled");
    let id = save_chat_history(&format!("user-{token}"), &format!("assistant-{token}"))
        .await
        .unwrap();

    assert!(id > 0);
    assert!(
        !test_chat_history_vec_dao(RagModel::BgeSmallEnV15)
            .unwrap()
            .has_embedding(id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn save_chat_history_indexes_embedding_in_background() {
    let _guard = test_support::app_test_guard().await;
    test_support::set_rag_model("BAAI/bge-small-en-v1.5");
    let backend = fake_embedder_success(vec![0.25; 384]);
    let token = test_support::unique_test_token("chat-history-service", "rag");
    let id = save_chat_history_with_backend(
        &format!("user-{token}"),
        &format!("assistant-{token}"),
        backend,
    )
    .await
    .unwrap();

    wait_for_embedding(id, RagModel::BgeSmallEnV15).await;

    assert!(
        test_chat_history_vec_dao(RagModel::BgeSmallEnV15)
            .unwrap()
            .has_embedding(id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn save_chat_history_returns_success_when_rag_model_is_invalid() {
    let _guard = test_support::app_test_guard().await;
    test_support::set_rag_model("invalid-model");

    let token = test_support::unique_test_token("chat-history-service", "invalid-model");
    let id = save_chat_history(&format!("user-{token}"), &format!("assistant-{token}"))
        .await
        .unwrap();

    assert!(id > 0);
    assert!(
        !test_chat_history_vec_dao(RagModel::BgeSmallEnV15)
            .unwrap()
            .has_embedding(id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn save_chat_history_emits_rag_log_events() {
    let _guard = test_support::app_test_guard().await;
    test_support::init_test_logging();
    test_support::set_rag_model("BAAI/bge-small-en-v1.5");
    let success_id = save_chat_history_with_backend(
        "log-success-user",
        "log-success-assistant",
        fake_embedder_success(vec![0.5; 384]),
    )
    .await
    .unwrap();
    wait_for_embedding(success_id, RagModel::BgeSmallEnV15).await;

    test_support::disable_rag_for_test();
    let skipped_id = save_chat_history("log-skip-user", "log-skip-assistant")
        .await
        .unwrap();

    test_support::set_rag_model("invalid-model");
    let failed_id = save_chat_history("log-fail-user", "log-fail-assistant")
        .await
        .unwrap();

    let logs = test_support::wait_for_test_logs_contains(&[
        "rag_index_schedule".to_string(),
        format!("chat_history_id={success_id}"),
        "cache_dir=".to_string(),
        "rag_index_success".to_string(),
        "rag_index_skipped".to_string(),
        format!("chat_history_id={skipped_id}"),
        "reason=\"model_not_configured\"".to_string(),
        "rag_index_failed".to_string(),
        format!("chat_history_id={failed_id}"),
        "unsupported ARKNIGHTS_RAG_MODEL".to_string(),
    ])
    .await
    .unwrap();
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

#[tokio::test]
async fn search_rag_returns_json_lines_for_top_matches() {
    let _guard = test_support::app_test_guard().await;
    disable_rag_for_test();

    let token = test_support::unique_test_token("chat-history-service", "search-rag");
    let ids = [
        save_chat_history(&format!("user-1-{token}"), &format!("assistant-1-{token}"))
            .await
            .unwrap(),
        save_chat_history(&format!("user-2-{token}"), &format!("assistant-2-{token}"))
            .await
            .unwrap(),
        save_chat_history(&format!("user-3-{token}"), &format!("assistant-3-{token}"))
            .await
            .unwrap(),
        save_chat_history(&format!("user-4-{token}"), &format!("assistant-4-{token}"))
            .await
            .unwrap(),
        save_chat_history(&format!("user-5-{token}"), &format!("assistant-5-{token}"))
            .await
            .unwrap(),
        save_chat_history(&format!("user-6-{token}"), &format!("assistant-6-{token}"))
            .await
            .unwrap(),
    ];

    test_support::set_rag_model("BAAI/bge-small-en-v1.5");

    let dao = test_chat_history_vec_dao(RagModel::BgeSmallEnV15).unwrap();
    dao.upsert_embedding(
        ids[0],
        embedding_with_offset(RagModel::BgeSmallEnV15, 40, 1.0, 0.0),
    )
    .await
    .unwrap();
    dao.upsert_embedding(
        ids[1],
        embedding_with_offset(RagModel::BgeSmallEnV15, 40, 0.99, 0.01),
    )
    .await
    .unwrap();
    dao.upsert_embedding(
        ids[2],
        embedding_with_offset(RagModel::BgeSmallEnV15, 40, 0.97, 0.03),
    )
    .await
    .unwrap();
    dao.upsert_embedding(
        ids[3],
        embedding_with_offset(RagModel::BgeSmallEnV15, 40, 0.95, 0.05),
    )
    .await
    .unwrap();
    dao.upsert_embedding(
        ids[4],
        embedding_with_offset(RagModel::BgeSmallEnV15, 40, 0.9, 0.1),
    )
    .await
    .unwrap();
    dao.upsert_embedding(
        ids[5],
        embedding_with_offset(RagModel::BgeSmallEnV15, 40, 0.0, 1.0),
    )
    .await
    .unwrap();

    let joined = search_rag_with_backend(
        vec![" deploy ".to_string(), token.clone(), "".to_string()],
        fake_embedder_success(embedding_with_offset(RagModel::BgeSmallEnV15, 40, 1.0, 0.0)),
    )
    .await
    .unwrap();
    let lines: Vec<_> = joined.lines().collect();

    assert_eq!(lines.len(), RAG_SEARCH_LIMIT);
    assert!(joined.contains('\n'));

    let expected_ids = &ids[..RAG_SEARCH_LIMIT];
    for (line, expected_id) in lines.iter().zip(expected_ids.iter()) {
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["id"].as_i64().unwrap(), *expected_id);
        assert!(value["user_content"].as_str().unwrap().contains(&token));
    }
}

#[tokio::test]
async fn search_rag_returns_err_when_keywords_are_empty_after_trim() {
    let _guard = test_support::app_test_guard().await;
    disable_rag_for_test();

    let err = search_rag(vec![" ".to_string(), "\t".to_string()])
        .await
        .unwrap_err();

    assert!(err.to_string().contains("keywords must not be empty"));
}

#[tokio::test]
async fn search_rag_returns_err_when_rag_is_disabled() {
    let _guard = test_support::app_test_guard().await;
    disable_rag_for_test();

    let err = search_rag(vec!["hello".to_string()]).await.unwrap_err();

    assert!(
        err.to_string()
            .contains("rag search requires ARKNIGHTS_RAG_MODEL")
    );
}

#[tokio::test]
async fn search_rag_returns_err_when_rag_model_is_invalid() {
    let _guard = test_support::app_test_guard().await;
    test_support::set_rag_model("invalid-model");

    let err = search_rag(vec!["hello".to_string()]).await.unwrap_err();

    assert!(err.to_string().contains("unsupported ARKNIGHTS_RAG_MODEL"));
}

#[tokio::test]
async fn search_rag_emits_log_events() {
    let _guard = test_support::app_test_guard().await;
    test_support::init_test_logging();
    disable_rag_for_test();

    let token = test_support::unique_test_token("chat-history-service", "search-log");
    let id = save_chat_history(&format!("user-{token}"), &format!("assistant-{token}"))
        .await
        .unwrap();
    test_support::set_rag_model("BAAI/bge-small-en-v1.5");
    let dao = test_chat_history_vec_dao(RagModel::BgeSmallEnV15).unwrap();
    dao.upsert_embedding(
        id,
        embedding_with_offset(RagModel::BgeSmallEnV15, 80, 1.0, 0.0),
    )
    .await
    .unwrap();
    let success_query = format!("success-{token}");
    let success_joined = search_rag_with_backend(
        vec![success_query.clone()],
        fake_embedder_success(embedding_with_offset(RagModel::BgeSmallEnV15, 80, 1.0, 0.0)),
    )
    .await
    .unwrap();
    let expected_result_count = if success_joined.is_empty() {
        0
    } else {
        success_joined.lines().count()
    };

    test_support::set_rag_model("invalid-model");
    let failed_query = format!("failed-{token}");
    let err = search_rag(vec![failed_query.clone()]).await.unwrap_err();
    assert!(err.to_string().contains("unsupported ARKNIGHTS_RAG_MODEL"));

    let logs = test_support::wait_for_test_logs_contains(&[
        "rag_search_start".to_string(),
        "rag_search_success".to_string(),
        "rag_search_failed".to_string(),
        format!("query={success_query}"),
        format!("query={failed_query}"),
        format!("result_count={expected_result_count}"),
        "dimension=384".to_string(),
        "unsupported ARKNIGHTS_RAG_MODEL".to_string(),
    ])
    .await
    .unwrap();
    assert!(logs.contains("rag_search_start"));
    assert!(logs.contains("rag_search_success"));
    assert!(logs.contains("rag_search_failed"));
    assert!(logs.contains(&format!("query={success_query}")));
    assert!(logs.contains(&format!("query={failed_query}")));
    assert!(logs.contains(&format!("result_count={expected_result_count}")));
    assert!(logs.contains("dimension=384"));
    assert!(logs.contains("unsupported ARKNIGHTS_RAG_MODEL"));
}

fn disable_rag_for_test() {
    test_support::disable_rag_for_test();
}

fn test_chat_history_vec_dao(model: RagModel) -> anyhow::Result<ChatHistoryVecDao> {
    ChatHistoryVecDao::with_path(test_support::test_db_path(), model.dimension())
}

fn insert_chat_history_with_created_at(
    user_content: &str,
    assistant_content: &str,
    created_at: &str,
) -> i64 {
    let _ = chat_history_dao().unwrap();
    let conn = rusqlite::Connection::open(test_support::test_db_path()).unwrap();
    conn.execute(
        "insert into chat_history (user_content, assistant_content, created_at)
         values (?1, ?2, ?3)",
        rusqlite::params![user_content, assistant_content, created_at],
    )
    .unwrap();

    conn.last_insert_rowid()
}

fn embedding_with_offset(model: RagModel, offset: usize, first: f32, second: f32) -> Vec<f32> {
    let mut embedding = vec![0.0; model.dimension()];
    embedding[offset] = first;
    embedding[offset + 1] = second;
    embedding
}

async fn wait_for_embedding(chat_history_id: i64, model: RagModel) {
    test_support::wait_until_async(
        "background rag indexing",
        20,
        Duration::from_millis(25),
        || async {
            let has_embedding = test_chat_history_vec_dao(model)?
                .has_embedding(chat_history_id)
                .await?;
            Ok(has_embedding)
        },
    )
    .await
    .unwrap();
}

fn fake_embedder_success(embedding: Vec<f32>) -> SharedRagEmbeddingBackend {
    Arc::new(FakeRagEmbedder::new(vec![Ok(embedding)]))
}

struct FakeRagEmbedder {
    results: Mutex<VecDeque<anyhow::Result<Vec<f32>>>>,
}

impl FakeRagEmbedder {
    fn new(results: Vec<anyhow::Result<Vec<f32>>>) -> Self {
        Self {
            results: Mutex::new(results.into()),
        }
    }
}

#[async_trait::async_trait]
impl RagEmbeddingBackend for FakeRagEmbedder {
    async fn embed_text(
        &self,
        _config: RagRuntimeConfig,
        _text: String,
    ) -> anyhow::Result<Vec<f32>> {
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .expect("fake rag embedder ran out of results")
    }
}
