use super::*;
use crate::llm::base_llm::{FunctionCall, ToolCall};
use crate::test_support;

#[test]
fn get_user_profile_tool_schema_has_no_required_params() {
    let tool = GetUserProfileTool::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "memory_get_user_profile");
    assert_eq!(schema.description, "Get user profile.");
    assert!(schema.parameters.required.is_empty());
    assert_eq!(schema.parameters.properties, serde_json::json!({}));
}

#[tokio::test]
async fn get_user_profile_tool_returns_stored_profile() {
    let _guard = test_support::app_test_guard();
    test_support::clear_user_profile().await.unwrap();
    kv_service::set_user_profile("Prefers concise answers")
        .await
        .unwrap();

    let tool = GetUserProfileTool::new();
    let result = tool
        .deep_seek_call(&tool_call("memory_get_user_profile", "{}"))
        .await;

    assert_eq!(result, "Prefers concise answers");

    test_support::clear_user_profile().await.unwrap();
}

#[test]
fn rewrite_user_profile_tool_schema_requires_markdown() {
    let tool = RewriteUserProfileTool::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "memory_rewrite_user_profile");
    assert_eq!(schema.description, "Rewrite user profile.");
    assert_eq!(schema.parameters.required, vec!["markdown".to_string()]);
    assert!(schema.parameters.properties["markdown"].is_object());
}

#[tokio::test]
async fn rewrite_user_profile_tool_persists_profile_markdown() {
    let _guard = test_support::app_test_guard();
    test_support::clear_user_profile().await.unwrap();

    let tool = RewriteUserProfileTool::new();
    let result = tool
        .deep_seek_call(&tool_call(
            "memory_rewrite_user_profile",
            r#"{"markdown":"Doctor profile"}"#,
        ))
        .await;

    assert_eq!(result, "Successfully rewrite user profile.");
    assert_eq!(
        kv_service::get_user_profile().await.unwrap(),
        "Doctor profile"
    );

    test_support::clear_user_profile().await.unwrap();
}

#[tokio::test]
async fn rewrite_user_profile_tool_returns_parse_error_for_invalid_arguments() {
    let _guard = test_support::app_test_guard();
    let tool = RewriteUserProfileTool::new();

    let result = tool
        .deep_seek_call(&tool_call("memory_rewrite_user_profile", "{"))
        .await;

    assert!(result.starts_with("Error: invalid arguments:"));
}

fn tool_call(name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: format!("call_{name}"),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: name.to_string(),
            arguments: arguments.to_string(),
        },
    }
}
