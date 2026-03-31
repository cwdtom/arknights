use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::{default_browser_schema, new_base_tool, placeholder_response};

pub struct ScrollTool {
    pub base_tool: BaseTool,
}

#[async_trait::async_trait]
impl LlmTool for ScrollTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> crate::llm::base_llm::Function {
        default_browser_schema(&self.base_tool)
    }

    async fn deep_seek_call(&self, _tool_call: &ToolCall) -> String {
        placeholder_response(&self.base_tool)
    }
}

impl ScrollTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("scroll", "Placeholder for browser scroll."),
        }
    }
}
