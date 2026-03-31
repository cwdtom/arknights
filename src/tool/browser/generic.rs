use crate::llm::base_llm::{Function, ToolCall};
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde_json::json;

pub struct BrowserPlaceholderTool {
    pub base_tool: BaseTool,
}

#[async_trait::async_trait]
impl LlmTool for BrowserPlaceholderTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> Function {
        Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(json!({}), vec![]),
        }
    }

    async fn deep_seek_call(&self, _tool_call: &ToolCall) -> String {
        format!("{} not implemented yet", self.base_tool.name)
    }
}

impl BrowserPlaceholderTool {
    pub fn new(name_suffix: &str, description: &str) -> Self {
        Self {
            base_tool: BaseTool {
                group_name: "browser".to_string(),
                group_description: "Browser tools".to_string(),
                name: format!("browser_{}", name_suffix),
                description: description.to_string(),
            },
        }
    }
}
