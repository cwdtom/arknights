use super::*;
use crate::test_support;
use crate::tool::base_tool::LlmTool;
use chrono::{Local, SecondsFormat};

#[path = "tool_test_support.rs"]
mod tool_test_support;

use tool_test_support::tool_call;

#[test]
fn insert_tool_schema_requires_content_and_start_time() {
    let tool = Insert::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "schedule_insert");
    assert_eq!(schema.parameters.required, vec!["content", "start_time"]);
    assert!(schema.parameters.properties["content"].is_object());
    assert!(schema.parameters.properties["start_time"].is_object());
}

#[test]
fn list_tool_schema_requires_start_and_end() {
    let tool = List::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "schedule_list");
    assert_eq!(schema.parameters.required, vec!["start", "end"]);
}

#[test]
fn search_tool_schema_requires_keyword() {
    let tool = Search::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "schedule_search");
    assert_eq!(schema.parameters.required, vec!["keyword"]);
}

#[test]
fn list_by_tag_tool_schema_requires_tag() {
    let tool = ListByTag::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "schedule_list_by_tag");
    assert_eq!(schema.parameters.required, vec!["tag"]);
}

#[test]
fn update_tool_schema_requires_id_content_start_time() {
    let tool = Update::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "schedule_update");
    assert_eq!(schema.parameters.properties["id"]["type"], "integer");
    assert_eq!(
        schema.parameters.required,
        vec!["id", "content", "start_time"]
    );
}

#[test]
fn get_tool_schema_requires_id() {
    let tool = Get::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "schedule_get");
    assert_eq!(schema.parameters.properties["id"]["type"], "integer");
    assert_eq!(schema.parameters.required, vec!["id"]);
}

#[test]
fn remove_tool_schema_requires_id() {
    let tool = Remove::new();
    let schema = tool.deep_seek_schema();

    assert_eq!(schema.name, "schedule_remove");
    assert_eq!(schema.parameters.properties["id"]["type"], "integer");
    assert_eq!(schema.parameters.required, vec!["id"]);
}

#[tokio::test]
async fn insert_tool_returns_parse_error_for_invalid_arguments() {
    let tool = Insert::new();

    let result = tool
        .deep_seek_call(&tool_call("schedule_insert", "{"))
        .await;

    assert!(result.starts_with("Error: invalid arguments:"));
}

#[tokio::test]
async fn insert_tool_rejects_invalid_rfc3339_time() {
    let _guard = test_support::app_test_guard().await;
    let tool = Insert::new();

    let result = tool
        .deep_seek_call(&tool_call(
            "schedule_insert",
            r#"{"content":"team meeting","start_time":"not-a-time"}"#,
        ))
        .await;

    assert!(result.contains("invalid schedule start_time"));
}

#[tokio::test]
async fn update_tool_rejects_end_time_before_start_time() {
    let _guard = test_support::app_test_guard().await;
    let insert = Insert::new();
    let get = Get::new();
    let remove = Remove::new();
    let update = Update::new();

    let inserted = insert
        .deep_seek_call(&tool_call(
            "schedule_insert",
            r#"{"content":"team meeting","start_time":"2026-04-01T14:00:00+08:00"}"#,
        ))
        .await;
    let inserted: serde_json::Value = serde_json::from_str(&inserted).unwrap();
    let id = inserted["id"].as_i64().unwrap();

    let result = update
        .deep_seek_call(&tool_call(
            "schedule_update",
            &format!(
                r#"{{"id":{id},"content":"team meeting","start_time":"2026-04-01T14:00:00+08:00","end_time":"2026-04-01T13:00:00+08:00"}}"#
            ),
        ))
        .await;

    assert!(result.contains("schedule end_time must be greater than or equal to start_time"));

    let fetched = get
        .deep_seek_call(&tool_call("schedule_get", &format!(r#"{{"id":{id}}}"#)))
        .await;
    let fetched: serde_json::Value = serde_json::from_str(&fetched).unwrap();
    let expected_start = chrono::DateTime::parse_from_rfc3339("2026-04-01T14:00:00+08:00")
        .unwrap()
        .with_timezone(&Local)
        .to_rfc3339_opts(SecondsFormat::Millis, false);
    assert_eq!(fetched["start_time"], expected_start);

    let removed = remove
        .deep_seek_call(&tool_call("schedule_remove", &format!(r#"{{"id":{id}}}"#)))
        .await;
    let removed: serde_json::Value = serde_json::from_str(&removed).unwrap();
    assert_eq!(removed["removed"], true);
}

#[tokio::test]
async fn list_tool_supports_mixed_timezones_and_returns_normalized_times() {
    let _guard = test_support::app_test_guard().await;
    let insert = Insert::new();
    let list = List::new();
    let remove = Remove::new();

    let inserted = insert
        .deep_seek_call(&tool_call(
            "schedule_insert",
            r#"{"content":"team meeting","start_time":"2026-04-02T01:30:00+08:00","end_time":"2026-04-02T02:30:00+08:00"}"#,
        ))
        .await;
    let inserted: serde_json::Value = serde_json::from_str(&inserted).unwrap();
    let id = inserted["id"].as_i64().unwrap();

    let listed = list
        .deep_seek_call(&tool_call(
            "schedule_list",
            r#"{"start":"2026-04-01T17:00:00Z","end":"2026-04-01T18:00:00Z"}"#,
        ))
        .await;
    let listed: serde_json::Value = serde_json::from_str(&listed).unwrap();
    let listed = listed.as_array().unwrap();
    let item = listed.iter().find(|item| item["id"] == id).unwrap();
    let expected_start = chrono::DateTime::parse_from_rfc3339("2026-04-02T01:30:00+08:00")
        .unwrap()
        .with_timezone(&Local)
        .to_rfc3339_opts(SecondsFormat::Millis, false);
    let expected_end = chrono::DateTime::parse_from_rfc3339("2026-04-02T02:30:00+08:00")
        .unwrap()
        .with_timezone(&Local)
        .to_rfc3339_opts(SecondsFormat::Millis, false);
    assert_eq!(item["start_time"], expected_start);
    assert_eq!(item["end_time"], expected_end);

    let removed = remove
        .deep_seek_call(&tool_call("schedule_remove", &format!(r#"{{"id":{id}}}"#)))
        .await;
    let removed: serde_json::Value = serde_json::from_str(&removed).unwrap();
    assert_eq!(removed["removed"], true);
}

#[tokio::test]
async fn schedule_tools_support_full_crud_flow() {
    let _guard = test_support::app_test_guard().await;

    let insert = Insert::new();
    let inserted = insert
        .deep_seek_call(&tool_call(
            "schedule_insert",
            r#"{"content":"team meeting","tag":"work","start_time":"2026-04-01T14:00:00+08:00","end_time":"2026-04-01T15:00:00+08:00"}"#,
        ))
        .await;
    let inserted: serde_json::Value = serde_json::from_str(&inserted).unwrap();
    let id = inserted["id"].as_i64().unwrap();
    assert!(id > 0);
    assert_eq!(inserted["content"], "team meeting");
    assert_eq!(inserted["tag"], "work");

    let get = Get::new();
    let fetched = get
        .deep_seek_call(&tool_call("schedule_get", &format!(r#"{{"id":{id}}}"#)))
        .await;
    let fetched: serde_json::Value = serde_json::from_str(&fetched).unwrap();
    assert_eq!(fetched["content"], "team meeting");

    let update = Update::new();
    let updated = update
        .deep_seek_call(&tool_call(
            "schedule_update",
            &format!(
                r#"{{"id":{id},"content":"standup","tag":"daily","start_time":"2026-04-01T09:00:00+08:00"}}"#
            ),
        ))
        .await;
    let updated: serde_json::Value = serde_json::from_str(&updated).unwrap();
    assert_eq!(updated["content"], "standup");
    assert_eq!(updated["tag"], "daily");

    let list = List::new();
    let listed = list
        .deep_seek_call(&tool_call(
            "schedule_list",
            r#"{"start":"2026-04-01T00:00:00+08:00","end":"2026-04-01T23:59:59+08:00"}"#,
        ))
        .await;
    let listed: serde_json::Value = serde_json::from_str(&listed).unwrap();
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == id)
    );

    let search = Search::new();
    let searched = search
        .deep_seek_call(&tool_call("schedule_search", r#"{"keyword":"standup"}"#))
        .await;
    let searched: serde_json::Value = serde_json::from_str(&searched).unwrap();
    assert!(
        searched
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == id)
    );

    let list_by_tag = ListByTag::new();
    let tagged = list_by_tag
        .deep_seek_call(&tool_call("schedule_list_by_tag", r#"{"tag":"daily"}"#))
        .await;
    let tagged: serde_json::Value = serde_json::from_str(&tagged).unwrap();
    assert!(
        tagged
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == id)
    );

    let remove = Remove::new();
    let removed = remove
        .deep_seek_call(&tool_call("schedule_remove", &format!(r#"{{"id":{id}}}"#)))
        .await;
    let removed: serde_json::Value = serde_json::from_str(&removed).unwrap();
    assert_eq!(removed["removed"], true);

    let missing = get
        .deep_seek_call(&tool_call("schedule_get", &format!(r#"{{"id":{id}}}"#)))
        .await;
    assert!(missing.starts_with("Error: schedule event not found:"));
}
