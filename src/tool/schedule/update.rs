use super::{GROUP_DESC, GROUP_NAME, common};
use crate::llm;
use crate::llm::base_llm::ToolCall;
use crate::schedule::schedule_service::{self, UpdateScheduleEvent};
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct Update {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct UpdateArgs {
    id: i64,
    content: String,
    tag: Option<String>,
    start_time: String,
    end_time: Option<String>,
}

#[async_trait::async_trait]
impl LlmTool for Update {
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
                },
                "content": {
                    "type": "string",
                    "description": "schedule event content"
                },
                "tag": {
                    "type": "string",
                    "description": "optional tag for categorization"
                },
                "start_time": {
                    "type": "string",
                    "description": "start time in RFC3339 format"
                },
                "end_time": {
                    "type": "string",
                    "description": "optional end time in RFC3339 format"
                }
            }),
            common::required_update_fields(),
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: UpdateArgs = match common::parse_args(tool_call, "schedule update") {
            Ok(v) => v,
            Err(msg) => return msg,
        };
        let input = UpdateScheduleEvent {
            id: args.id,
            content: args.content,
            tag: args.tag,
            start_time: args.start_time,
            end_time: args.end_time,
        };

        match schedule_service::update(input).await {
            Ok(event) => common::to_json(&event, "schedule update"),
            Err(err) => format!("Error: update schedule event: {err}"),
        }
    }
}

impl Update {
    pub fn new() -> Self {
        Self {
            base_tool: common::new_base_tool(
                GROUP_NAME,
                GROUP_DESC,
                "update",
                "Update schedule event.",
            ),
        }
    }
}
