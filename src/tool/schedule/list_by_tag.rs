use super::{GROUP_DESC, GROUP_NAME, common};
use crate::llm;
use crate::llm::base_llm::ToolCall;
use crate::schedule::schedule_service;
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct ListByTag {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct TagArgs {
    tag: String,
}

#[async_trait::async_trait]
impl LlmTool for ListByTag {
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
                "tag": {
                    "type": "string",
                    "description": "tag to filter schedule events"
                }
            }),
            vec!["tag".to_string()],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: TagArgs = match common::parse_args(tool_call, "schedule list_by_tag") {
            Ok(v) => v,
            Err(msg) => return msg,
        };

        match schedule_service::list_by_tag(args.tag).await {
            Ok(events) => common::to_json(&events, "schedule list_by_tag"),
            Err(err) => format!("Error: list schedule events by tag: {err}"),
        }
    }
}

impl ListByTag {
    pub fn new() -> Self {
        Self {
            base_tool: common::new_base_tool(
                GROUP_NAME,
                GROUP_DESC,
                "list_by_tag",
                "List schedule events by tag.",
            ),
        }
    }
}
