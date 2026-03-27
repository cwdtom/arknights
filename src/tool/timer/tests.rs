use super::*;
use crate::llm::base_llm::{FunctionCall, ToolCall};
use crate::test_support;
use crate::tool::base_tool::LlmTool;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn insert_tool_schema_requires_crud_fields() {
    let tool = Insert::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "timer_insert");
    assert_eq!(
        schema.parameters.required,
        vec!["id", "prompt", "cron_expr", "remaining_runs"]
    );
    assert!(schema.parameters.properties["id"].is_object());
    assert!(schema.parameters.properties["prompt"].is_object());
    assert!(schema.parameters.properties["cron_expr"].is_object());
    assert!(schema.parameters.properties["remaining_runs"].is_object());
}

#[test]
fn list_tool_schema_has_no_required_fields() {
    let tool = List::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "timer_list");
    assert!(schema.parameters.required.is_empty());
    assert_eq!(schema.parameters.properties, serde_json::json!({}));
}

#[tokio::test]
async fn insert_tool_returns_parse_error_for_invalid_arguments() {
    let tool = Insert::new();

    let result = tool.deep_seek_call(&tool_call("timer_insert", "{")).await;

    assert!(result.starts_with("Error: invalid arguments:"));
}

#[tokio::test]
async fn timer_tools_support_full_crud_flow() {
    let _guard = test_support::app_test_guard().await;
    let id = unique_timer_id("crud");

    let insert = Insert::new();
    let inserted = insert
        .deep_seek_call(&tool_call(
            "timer_insert",
            &format!(
                r#"{{"id":"{id}","prompt":"drink water","cron_expr":"0 0 9 * * *","remaining_runs":2}}"#
            ),
        ))
        .await;
    assert_eq!(inserted, format!("Successfully inserted timer task: {id}"));

    let get = Get::new();
    let fetched = get
        .deep_seek_call(&tool_call("timer_get", &format!(r#"{{"id":"{id}"}}"#)))
        .await;
    let fetched: serde_json::Value = serde_json::from_str(&fetched).unwrap();
    assert_eq!(fetched["prompt"], "drink water");

    let update = Update::new();
    let updated = update
        .deep_seek_call(&tool_call(
            "timer_update",
            &format!(
                r#"{{"id":"{id}","prompt":"stretch","cron_expr":"0 30 9 * * *","remaining_runs":0}}"#
            ),
        ))
        .await;
    assert_eq!(updated, format!("Successfully updated timer task: {id}"));

    let list = List::new();
    let listed = list.deep_seek_call(&tool_call("timer_list", "{}")).await;
    let listed: serde_json::Value = serde_json::from_str(&listed).unwrap();
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == id)
    );

    let fetched = get
        .deep_seek_call(&tool_call("timer_get", &format!(r#"{{"id":"{id}"}}"#)))
        .await;
    let fetched: serde_json::Value = serde_json::from_str(&fetched).unwrap();
    assert_eq!(fetched["prompt"], "stretch");
    assert_eq!(fetched["remaining_runs"], 0);

    let remove = Remove::new();
    let removed = remove
        .deep_seek_call(&tool_call("timer_remove", &format!(r#"{{"id":"{id}"}}"#)))
        .await;
    let removed: serde_json::Value = serde_json::from_str(&removed).unwrap();
    assert_eq!(removed["removed"], true);

    let missing = get
        .deep_seek_call(&tool_call("timer_get", &format!(r#"{{"id":"{id}"}}"#)))
        .await;
    assert_eq!(missing, format!("Error: timer task not found: {id}"));
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

fn unique_timer_id(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("timer-tool-{label}-{nanos}")
}
