use super::{GROUP_DESC, GROUP_NAME, common};
use crate::llm;
use crate::llm::base_llm::ToolCall;
use crate::timer::timer_service;
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct List {
    pub base_tool: BaseTool,
}

#[async_trait::async_trait]
impl LlmTool for List {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        common::build_schema(&self.base_tool, serde_json::json!({}), vec![])
    }

    async fn deep_seek_call(&self, _: &ToolCall) -> String {
        match timer_service::list().await {
            Ok(tasks) => common::to_json(&tasks, "timer list"),
            Err(err) => format!("Error: list timer tasks: {err}"),
        }
    }
}

impl List {
    pub fn new() -> Self {
        Self {
            base_tool: common::new_base_tool(GROUP_NAME, GROUP_DESC, "list", "List timer tasks."),
        }
    }
}
