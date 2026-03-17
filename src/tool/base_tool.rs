use serde::Serialize;
use crate::llm;
use crate::llm::base_llm::ToolCall;

#[derive(Serialize, Debug)]
pub struct BaseTool {
    pub group_name: String,
    pub group_description: String,
    pub name: String,
    pub description: String,
}

#[async_trait::async_trait]
pub trait LlmTool {
    /// generate deepseek tool schema
    fn deep_seek_schema(&self) -> llm::base_llm::Function;

    /// tool call
    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String;
}
