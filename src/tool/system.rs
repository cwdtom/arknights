use crate::llm;
use crate::llm::base_llm::{Parameters, ToolCall};
use crate::tool::base_tool::{BaseTool, LlmTool};
use chrono::Local;
use serde::Serialize;

const GROUP_NAME: &str = "system";
const GROUP_DESC: &str = "System tools(include `date`).";

#[derive(Serialize, Debug)]
pub struct DateTool {
    pub base_tool: BaseTool,
}

#[async_trait::async_trait]
impl LlmTool for DateTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(serde_json::json!({}), vec![]),
        }
    }

    async fn deep_seek_call(&self, _: &ToolCall) -> String {
        Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

impl DateTool {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_date",
            description: "Get system current date, format: yyyy-MM-dd HH:mm:ss".to_string(),
        };

        DateTool { base_tool }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::base_llm::{FunctionCall, ToolCall};

    #[test]
    fn date_tool_new_sets_correct_fields() {
        let tool = DateTool::new();
        assert_eq!(tool.base_tool.name, "system_date");
        assert_eq!(tool.base_tool.group_name, "system");
    }

    #[test]
    fn date_tool_group_name() {
        let tool = DateTool::new();
        assert_eq!(tool.group_name(), "system");
    }

    #[test]
    fn date_tool_schema() {
        let tool = DateTool::new();
        let schema = tool.deep_seek_schema();
        assert_eq!(schema.name, "system_date");
        assert!(!schema.description.is_empty());
        assert_eq!(schema.parameters.r#type, "object");
    }

    #[tokio::test]
    async fn date_tool_call_returns_parseable_datetime() {
        let tool = DateTool::new();
        let dummy_call = ToolCall {
            id: "call_1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "system_date".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let result = tool.deep_seek_call(&dummy_call).await;
        // Should match format "2026-03-18 12:34:56"
        assert_eq!(result.len(), 19);
        assert!(chrono::NaiveDateTime::parse_from_str(&result, "%Y-%m-%d %H:%M:%S").is_ok());
    }
}
