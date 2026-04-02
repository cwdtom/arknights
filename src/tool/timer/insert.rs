use super::{GROUP_DESC, GROUP_NAME, common};
use crate::llm;
use crate::llm::base_llm::ToolCall;
use crate::timer::timer_service::{self, CreateTimerTask};
use crate::tool::base_tool::{BaseTool, LlmTool};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct Insert {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct InsertArgs {
    id: String,
    prompt: String,
    cron_expr: String,
    remaining_runs: u32,
}

#[async_trait::async_trait]
impl LlmTool for Insert {
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
                    "description": "remaining run count, if there is no explicit limit on the number of attempts, assign 40000000."
                }
            }),
            common::required_task_fields(),
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: InsertArgs = match common::parse_args(tool_call, "timer insert") {
            Ok(v) => v,
            Err(msg) => return msg,
        };
        let id = args.id.clone();
        let input = CreateTimerTask {
            id: args.id,
            prompt: args.prompt,
            cron_expr: args.cron_expr,
            remaining_runs: args.remaining_runs,
        };

        match timer_service::create(input).await {
            Ok(_) => format!("Successfully inserted timer task: {id}"),
            Err(err) => format!("Error: insert timer task: {err}"),
        }
    }
}

impl Insert {
    pub fn new() -> Self {
        Self {
            base_tool: common::new_base_tool(
                GROUP_NAME,
                GROUP_DESC,
                "insert",
                "Insert timer task.",
            ),
        }
    }
}
