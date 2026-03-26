use crate::llm::base_llm::{Parameters, ToolCall};
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::{im, llm};
use serde::{Deserialize, Serialize};
use tracing::error;

const GROUP_NAME: &str = "process_control";
const GROUP_DESC: &str = "Process control";

#[derive(Serialize, Debug)]
pub struct AskUser {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct AskUserArgs {
    question: String,
}

#[async_trait::async_trait]
impl LlmTool for AskUser {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(
                serde_json::json!({
                    "question": {
                        "type": "string",
                        "description": "The question to ask the user"
                    }
                }),
                vec!["question".to_string()],
            ),
        }
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: AskUserArgs = match serde_json::from_str(&tool_call.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse ask_user arguments: {:?}", e);
                return format!("Error: invalid arguments: {}", e);
            }
        };

        im::base_im::ask_user(args.question)
            .await
            .unwrap_or_else(|e| {
                error!("ask_user failed: {:?}", e);
                format!("Error: {}", e)
            })
    }
}

impl AskUser {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_ask_user",
            description: "Ask the user a question via lark and wait for their reply.".to_string(),
        };

        AskUser { base_tool }
    }
}

#[derive(Serialize, Debug)]
pub struct Done {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct DoneArgs {
    answer: String,
}

#[async_trait::async_trait]
impl LlmTool for Done {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(
                serde_json::json!({
                    "answer": {
                        "type": "string",
                        "description": "final answer"
                    }
                }),
                vec!["answer".to_string()],
            ),
        }
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: DoneArgs = match serde_json::from_str(&tool_call.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse done arguments: {:?}", e);
                return format!("Error: invalid arguments: {}", e);
            }
        };

        args.answer
    }
}

impl Done {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_done",
            description: "ReAct task done.".to_string(),
        };

        Done { base_tool }
    }
}

#[derive(Serialize, Debug)]
pub struct Replan {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct ReplanArgs {
    reason: String,
}

#[async_trait::async_trait]
impl LlmTool for Replan {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(
                serde_json::json!({
                    "reason": {
                        "type": "string",
                        "description": "replan reason"
                    }
                }),
                vec!["reason".to_string()],
            ),
        }
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: ReplanArgs = match serde_json::from_str(&tool_call.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse replan arguments: {:?}", e);
                return format!("Error: invalid arguments: {}", e);
            }
        };

        args.reason
    }
}

impl Replan {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_replan",
            description: "Need replan.".to_string(),
        };

        Replan { base_tool }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::base_llm::{FunctionCall, ToolCall};

    #[test]
    fn ask_user_tool_schema_has_question_param() {
        let tool = AskUser::new();
        let schema = tool.deep_seek_schema();
        assert_eq!(schema.name, "process_control_ask_user");
        assert!(schema.parameters.required.contains(&"question".to_string()));
        assert!(schema.parameters.properties["question"].is_object());
    }

    #[test]
    fn ask_user_tool_group_name() {
        let tool = AskUser::new();
        assert_eq!(tool.group_name(), "process_control");
    }

    #[tokio::test]
    async fn done_tool_returns_answer() {
        let tool = Done::new();
        let tool_call = ToolCall {
            id: "call_done".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "process_control_done".to_string(),
                arguments: r#"{"answer":"all done"}"#.to_string(),
            },
        };

        let answer = tool.deep_seek_call(&tool_call).await;
        assert_eq!(answer, "all done");
    }

    #[tokio::test]
    async fn replan_tool_returns_reason() {
        let tool = Replan::new();
        let tool_call = ToolCall {
            id: "call_replan".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "process_control_replan".to_string(),
                arguments: r#"{"reason":"need more context"}"#.to_string(),
            },
        };

        let reason = tool.deep_seek_call(&tool_call).await;
        assert_eq!(reason, "need more context");
    }

    #[tokio::test]
    async fn done_tool_returns_parse_error_for_invalid_json() {
        let tool = Done::new();
        let tool_call = ToolCall {
            id: "call_done".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "process_control_done".to_string(),
                arguments: "{".to_string(),
            },
        };

        let result = tool.deep_seek_call(&tool_call).await;
        assert!(result.starts_with("Error: invalid arguments:"));
    }
}
