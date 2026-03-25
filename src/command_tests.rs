use super::*;
use crate::kv_service::get_personal_value;
use crate::test_support;

#[tokio::test]
async fn execute_rejects_invalid_command() {
    let err = execute("/invalid".to_string()).await.unwrap_err();
    assert!(err.to_string().contains("invalid command"));
}

#[tokio::test]
async fn execute_set_personal_persists_value() {
    let _guard = test_support::app_test_guard();
    test_support::clear_personal_value().await.unwrap();

    execute("/set_personal Amiya".to_string()).await.unwrap();

    let value = get_personal_value().await.unwrap();
    assert_eq!(value, "Amiya");

    test_support::clear_personal_value().await.unwrap();
}
