use super::*;
use crate::test_support;

#[test]
fn personal_resp_deserializes_contents() {
    let json = r#"{
        "contents": ["第一句", "第二句"]
    }"#;

    let resp: PersonalResp = serde_json::from_str(json).unwrap();
    assert_eq!(resp.contents, vec!["第一句", "第二句"]);
}

#[tokio::test]
async fn personal_message_returns_error_when_personal_role_missing() {
    let _guard = test_support::app_test_guard().await;
    test_support::clear_personal_value().await.unwrap();

    let err = send_personal_message("需要改写的内容".to_string())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("key not found"));
}
