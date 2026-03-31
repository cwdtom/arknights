use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::{browser_schema, new_base_tool, placeholder_response};
use serde_json::json;

pub struct FillTool {
    pub base_tool: BaseTool,
}

#[async_trait::async_trait]
impl LlmTool for FillTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> crate::llm::base_llm::Function {
        browser_schema(
            &self.base_tool,
            json!({
                "element_id": {
                    "type": "string",
                    "description": "Element identifier to fill",
                },
                "value": {
                    "type": "string",
                    "description": "Value to enter",
                }
            }),
            &["element_id", "value"],
        )
    }

    async fn deep_seek_call(&self, _tool_call: &ToolCall) -> String {
        placeholder_response(&self.base_tool)
    }
}

impl FillTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("fill", "Placeholder for browser fill."),
        }
    }
}
