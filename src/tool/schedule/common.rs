use crate::llm;
use crate::llm::base_llm::{Parameters, ToolCall};
use crate::tool::base_tool::BaseTool;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::error;

pub(super) fn new_base_tool(
    group_name: &str,
    group_desc: &str,
    name: &str,
    description: &str,
) -> BaseTool {
    BaseTool {
        group_name: group_name.to_string(),
        group_description: group_desc.to_string(),
        name: format!("{group_name}_{name}"),
        description: description.to_string(),
    }
}

pub(super) fn required_insert_fields() -> Vec<String> {
    vec!["content".to_string(), "start_time".to_string()]
}

pub(super) fn required_update_fields() -> Vec<String> {
    vec![
        "id".to_string(),
        "content".to_string(),
        "start_time".to_string(),
    ]
}

pub(super) fn build_schema(
    base_tool: &BaseTool,
    properties: serde_json::Value,
    required: Vec<String>,
) -> llm::base_llm::Function {
    llm::base_llm::Function {
        name: base_tool.name.clone(),
        description: base_tool.description.clone(),
        parameters: Parameters::new(properties, required),
    }
}

pub(super) fn parse_args<T: DeserializeOwned>(
    tool_call: &ToolCall,
    label: &str,
) -> Result<T, String> {
    serde_json::from_str(&tool_call.function.arguments).map_err(|err| {
        error!("failed to parse {} arguments: {:?}", label, err);
        format!("Error: invalid arguments: {err}")
    })
}

pub(super) fn to_json<T: Serialize>(value: &T, label: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|err| {
        error!("failed to serialize {} result: {:?}", label, err);
        format!("Error: serialize {label}: {err}")
    })
}
