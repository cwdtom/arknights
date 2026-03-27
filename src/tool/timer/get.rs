use super::{GROUP_DESC, GROUP_NAME, common};
use crate::llm;
use crate::llm::base_llm::ToolCall;
use crate::timer::timer_service;
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct Get {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[async_trait::async_trait]
impl LlmTool for Get {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        common::build_schema(
            &self.base_tool,
            serde_json::json!({
                "id": {
                    "type": "string",
                    "description": "timer task id"
                }
            }),
            vec!["id".to_string()],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: IdArgs = match common::parse_args(tool_call, "timer get") {
            Ok(v) => v,
            Err(msg) => return msg,
        };

        match timer_service::get_by_id(args.id.clone()).await {
            Ok(Some(task)) => common::to_json(&task, "timer get"),
            Ok(None) => format!("Error: timer task not found: {}", args.id),
            Err(err) => format!("Error: get timer task: {err}"),
        }
    }
}

impl Get {
    pub fn new() -> Self {
        Self {
            base_tool: common::new_base_tool(
                GROUP_NAME,
                GROUP_DESC,
                "get",
                "Get timer task by id.",
            ),
        }
    }
}
