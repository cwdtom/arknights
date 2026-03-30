mod runtime;

use super::{GROUP_DESC, GROUP_NAME};
use crate::llm;
use crate::llm::base_llm::{Parameters, ToolCall};
use crate::tool::base_tool::{BaseTool, LlmTool};
use runtime::run_bash_command;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::error;

const BASH_TOOL_ENABLE_ENV_VAR: &str = "BASH_TOOL_ENABLE";
pub(super) const BASH_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
pub(super) const BASH_KILL_WAIT_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const MAX_BASH_RESULT_LEN: usize = 2000;
pub(super) const TRUNCATED_SUFFIX: &str = "\n...[truncated]";

#[derive(Serialize, Debug)]
pub struct BashTool {
    pub base_tool: BaseTool,
    pub enable: bool,
}

#[derive(Deserialize)]
struct BashToolArgs {
    command: String,
}

impl BashTool {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_bash",
            description: format!(
                r#"exec a single bash command, result max len {MAX_BASH_RESULT_LEN}, truncation exceeded.
                   All read and write operations should, to the greatest extent possible, occur only in the current path .cache directories.
                "#
            ),
        };

        Self {
            base_tool,
            enable: bash_tool_enabled(),
        }
    }
}

#[async_trait::async_trait]
impl LlmTool for BashTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(
                serde_json::json!({
                    "command": {
                        "type": "string",
                        "description": "Single bash command."
                    }
                }),
                vec!["command".to_string()],
            ),
        }
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        if !self.enable {
            return "user not allowed execute bash command.".to_string();
        }

        let args = match parse_bash_args(&tool_call.function.arguments) {
            Ok(args) => args,
            Err(message) => return message,
        };

        run_bash_command(args.command).await
    }
}

fn bash_tool_enabled() -> bool {
    match std::env::var(BASH_TOOL_ENABLE_ENV_VAR) {
        Ok(value) => value.parse().unwrap_or(false),
        Err(_) => false,
    }
}

fn parse_bash_args(arguments: &str) -> Result<BashToolArgs, String> {
    serde_json::from_str(arguments).map_err(|err| {
        error!("failed to parse bash tool arguments: {:?}", err);
        format!("Error: invalid arguments: {err}")
    })
}
