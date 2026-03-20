use crate::llm::base_llm::{Parameters, ToolCall};
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::{llm, memory};
use serde::{Deserialize, Serialize};
use tracing::error;

const GROUP_NAME: &str = "memory";
const GROUP_DESC: &str = "Memory tools.";

#[derive(Serialize, Debug)]
pub struct SearchTool {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct SearchToolArgs {
    keywords: Vec<String>,
}

#[async_trait::async_trait]
impl LlmTool for SearchTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(
                serde_json::json!({
                    "keywords": {
                            "type": "array",
                            "description": "list of keywords to search",
                            "items": {
                                "type": "string",
                                "description": "keywords to search"
                            }
                        }
                }),
                vec!["keywords".to_string()],
            ),
        }
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: SearchToolArgs = match serde_json::from_str(&tool_call.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse search tool arguments: {:?}", e);
                return format!("Error: invalid arguments: {}", e);
            }
        };

        memory::chat_history_service::fuzz_query(args.keywords)
            .await
            .unwrap_or_else(|_e| "search memory error".to_string())
    }
}

impl SearchTool {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_search_tool",
            description: "Get memories from rag database by keywords.".to_string(),
        };

        SearchTool { base_tool }
    }
}

#[derive(Serialize, Debug)]
pub struct ListTool {
    pub base_tool: BaseTool,
}

#[async_trait::async_trait]
impl LlmTool for ListTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(
                serde_json::json!({}),
                vec![],
            ),
        }
    }

    async fn deep_seek_call(&self, _: &ToolCall) -> String {
        match memory::chat_history_service::build_chat_history_messages(20).await {
            Ok(messages) => {
                let mut histories = vec![];
                for m in messages {
                    match serde_json::to_string(&m) {
                        Ok(json) => histories.push(json),
                        Err(_err) => continue,
                    }
                }

                histories.join("\n")
            },
            Err(e) => format!("Error: list chat histories: {}", e)
        }
    }
}

impl ListTool {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_list_tool",
            description: "Get recent chat histories.".to_string(),
        };

        ListTool { base_tool }
    }
}
