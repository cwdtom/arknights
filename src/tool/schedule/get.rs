use super::{GROUP_DESC, GROUP_NAME, common};
use crate::llm;
use crate::llm::base_llm::ToolCall;
use crate::schedule::schedule_service;
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct Get {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct IdArgs {
    id: i64,
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
                    "type": "integer",
                    "description": "schedule event id"
                }
            }),
            vec!["id".to_string()],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: IdArgs = match common::parse_args(tool_call, "schedule get") {
            Ok(v) => v,
            Err(msg) => return msg,
        };

        match schedule_service::get_by_id(args.id).await {
            Ok(Some(event)) => common::to_json(&event, "schedule get"),
            Ok(None) => format!("Error: schedule event not found: {}", args.id),
            Err(err) => format!("Error: get schedule event: {err}"),
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
                "Get schedule event by id.",
            ),
        }
    }
}
