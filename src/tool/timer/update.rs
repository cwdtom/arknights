use super::{GROUP_DESC, GROUP_NAME, common};
use crate::llm;
use crate::llm::base_llm::ToolCall;
use crate::timer::timer_service::{self, UpdateTimerTask};
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct Update {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct UpdateArgs {
    id: String,
    prompt: String,
    cron_expr: String,
    remaining_runs: u32,
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
                    "type": "string",
                    "description": "timer task id"
                },
                "prompt": {
                    "type": "string",
                    "description": "task prompt, do not include specific times; it should be a description of an actionable task."
                },
                "cron_expr": {
                    "type": "string",
                    "description": "cron expression"
                },
                "remaining_runs": {
                    "type": "integer",
                    "description": "remaining run count, 0 means paused"
                }
            }),
            common::required_task_fields(),
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: UpdateArgs = match common::parse_args(tool_call, "timer update") {
            Ok(v) => v,
            Err(msg) => return msg,
        };
        let id = args.id.clone();
        let input = UpdateTimerTask {
            id: args.id,
            prompt: args.prompt,
            cron_expr: args.cron_expr,
            remaining_runs: args.remaining_runs,
        };

        match timer_service::update(input).await {
            Ok(_) => format!("Successfully updated timer task: {id}"),
            Err(err) => format!("Error: update timer task: {err}"),
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
                "Update timer task.",
            ),
        }
    }
}
