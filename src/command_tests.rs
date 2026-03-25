use super::*;
use crate::kv_service::{clear_personal_value_for_test, get_personal_value};
use crate::memory::rag_embedder;

#[tokio::test]
async fn execute_rejects_invalid_command() {
    let err = execute("/invalid".to_string()).await.unwrap_err();
    assert!(err.to_string().contains("invalid command"));
}

#[tokio::test]
async fn execute_set_personal_persists_value() {
    let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
    clear_personal_value_for_test().await.unwrap();

    execute("/set_personal Amiya".to_string()).await.unwrap();

    let value = get_personal_value().await.unwrap();
    assert_eq!(value, "Amiya");

    clear_personal_value_for_test().await.unwrap();
}
