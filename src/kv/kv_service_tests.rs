use super::*;
use crate::test_support;

#[test]
fn kv_service_source_keeps_only_external_test_module_gate() {
    let source = include_str!("kv_service.rs");
    assert_eq!(source.matches("#[cfg(test)]").count(), 1);
}

#[tokio::test]
async fn set_then_get_personal_value_round_trips() {
    let _guard = test_support::app_test_guard();
    test_support::clear_personal_value().await.unwrap();

    set_personal_value("Kal'tsit").await.unwrap();

    let value = get_personal_value().await.unwrap();
    assert_eq!(value, "Kal'tsit");

    test_support::clear_personal_value().await.unwrap();
}

#[tokio::test]
async fn get_personal_value_returns_error_when_missing() {
    let _guard = test_support::app_test_guard();
    test_support::clear_personal_value().await.unwrap();

    let err = get_personal_value().await.unwrap_err();
    assert!(err.to_string().contains("key not found"));
}

#[tokio::test]
async fn set_then_get_user_profile_round_trips() {
    let _guard = test_support::app_test_guard();
    test_support::clear_user_profile().await.unwrap();

    set_user_profile("Doctor profile in markdown")
        .await
        .unwrap();

    let value = get_user_profile().await.unwrap();
    assert_eq!(value, "Doctor profile in markdown");

    test_support::clear_user_profile().await.unwrap();
}

#[tokio::test]
async fn get_user_profile_returns_empty_string_when_missing() {
    let _guard = test_support::app_test_guard();
    test_support::clear_user_profile().await.unwrap();

    let value = get_user_profile().await.unwrap();
    assert_eq!(value, "");
}
