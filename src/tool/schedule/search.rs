use super::{GROUP_DESC, GROUP_NAME, common};
use crate::llm;
use crate::llm::base_llm::ToolCall;
use crate::schedule::schedule_service;
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct Search {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct SearchArgs {
    keyword: String,
}

#[async_trait::async_trait]
impl LlmTool for Search {
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
                "keyword": {
                    "type": "string",
                    "description": "keyword to search in schedule event content"
                }
            }),
            vec!["keyword".to_string()],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: SearchArgs = match common::parse_args(tool_call, "schedule search") {
            Ok(v) => v,
            Err(msg) => return msg,
        };

        match schedule_service::search(args.keyword).await {
            Ok(events) => common::to_json(&events, "schedule search"),
            Err(err) => format!("Error: search schedule events: {err}"),
        }
    }
}

impl Search {
    pub fn new() -> Self {
        Self {
            base_tool: common::new_base_tool(
                GROUP_NAME,
                GROUP_DESC,
                "search",
                "Search schedule events by keyword in content.",
            ),
        }
    }
}
