use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::{browser_schema, new_base_tool, placeholder_response};
use serde_json::json;

pub struct WaitTextTool {
    pub base_tool: BaseTool,
}

#[async_trait::async_trait]
impl LlmTool for WaitTextTool {
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
                "text": {
                    "type": "string",
                    "description": "Text to wait for",
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds",
                }
            }),
            &["text"],
        )
    }

    async fn deep_seek_call(&self, _tool_call: &ToolCall) -> String {
        placeholder_response(&self.base_tool)
    }
}

impl WaitTextTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("wait_text", "Placeholder for browser wait text."),
        }
    }
}
