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
    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(serde_json::json!({}), vec![])
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
