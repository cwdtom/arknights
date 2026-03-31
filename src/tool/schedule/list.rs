use super::{GROUP_DESC, GROUP_NAME, common};
use crate::llm;
use crate::llm::base_llm::ToolCall;
use crate::schedule::schedule_service;
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct List {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct ListArgs {
    start: String,
    end: String,
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
        common::build_schema(
            &self.base_tool,
            serde_json::json!({
                "start": {
                    "type": "string",
                    "description": "range start time in RFC3339 format"
                },
                "end": {
                    "type": "string",
                    "description": "range end time in RFC3339 format"
                }
            }),
            vec!["start".to_string(), "end".to_string()],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: ListArgs = match common::parse_args(tool_call, "schedule list") {
            Ok(v) => v,
            Err(msg) => return msg,
        };

        match schedule_service::list_by_range(args.start, args.end).await {
            Ok(events) => common::to_json(&events, "schedule list"),
            Err(err) => format!("Error: list schedule events: {err}"),
        }
    }
}

impl List {
    pub fn new() -> Self {
        Self {
            base_tool: common::new_base_tool(
                GROUP_NAME,
                GROUP_DESC,
                "list",
                "List schedule events by date range.",
            ),
        }
    }
}
