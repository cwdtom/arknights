use super::*;
use crate::memory::rag_embedder;

#[tokio::test]
async fn set_then_get_personal_value_round_trips() {
    let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
    clear_personal_value_for_test().await.unwrap();

    set_personal_value("Kal'tsit").await.unwrap();

    let value = get_personal_value().await.unwrap();
    assert_eq!(value, "Kal'tsit");

    clear_personal_value_for_test().await.unwrap();
}

#[tokio::test]
async fn get_personal_value_returns_error_when_missing() {
    let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
    clear_personal_value_for_test().await.unwrap();

    let err = get_personal_value().await.unwrap_err();
    assert!(err.to_string().contains("key not found"));
}
