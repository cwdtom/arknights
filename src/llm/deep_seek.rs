use crate::agent::re_act::ReActResp;
use crate::tool;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tracing::info;

const BASE_URL: &str = "https://api.deepseek.com/chat/completions";
static API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY not set"));

/// request body
#[derive(Serialize, Debug)]
pub struct DeepSeek {
    model: String,
    pub messages: Vec<Message>,
    stream: bool,
    max_tokens: u16,
    response_format: ResponseFormat,
    temperature: f32,
    pub tools: Vec<Tool>,
    tool_choice: String,
}

#[derive(Serialize, Debug)]
pub struct ResponseFormat {
    pub r#type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub tool_call_id: Option<String>,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Serialize, Debug)]
pub struct Tool {
    pub r#type: String,
    pub function: Function,
}

#[derive(Serialize, Debug)]
pub struct Function {
    pub name: String,
    pub description: String,
    pub parameters: Parameters,
}

#[derive(Serialize, Debug)]
pub struct Parameters {
    pub r#type: String,
    pub properties: serde_json::Value,
    pub required: Vec<String>,
}

/// chat completion response
#[derive(Deserialize, Debug)]
pub struct ChatResponse {
    pub id: String,
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
pub struct Choice {
    pub message: Message,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,
    pub function: FunctionCall,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

impl DeepSeek {
    pub fn new(messages: Vec<Message>, tools: Vec<Tool>) -> Self {
        DeepSeek {
            model: "deepseek-chat".to_string(),
            messages,
            stream: false,
            max_tokens: 8192,
            response_format: ResponseFormat {
                r#type: "json_object".to_string(),
            },
            temperature: 1.0,
            tools,
            tool_choice: "auto".to_string(),
        }
    }

    pub async fn call(&mut self) -> anyhow::Result<ReActResp> {
        info!("deepseek llm request: {:?}", self);

        let client = reqwest::Client::new();
        let raw = client
            .post(BASE_URL)
            .header("Authorization", format!("Bearer {}", *API_KEY))
            .json(self)
            .send()
            .await?
            .text()
            .await?;
        info!("deepseek llm response: {}", raw);

        let resp: ChatResponse = serde_json::from_str(&raw)?;

        let mut re_act_resp: ReActResp = ReActResp {
            content: "".to_string(),
            is_done: false,
        };
        for choice in resp.choices {
            // content or tool call
            let (messages, is_done) = self.build_new_message(choice).await?;

            if is_done {
                // return the last final answer
                let answer = match messages.last() {
                    Some(message) => message,
                    None => {
                        return Err(anyhow!("exception reAct done"));
                    }
                };

                re_act_resp = ReActResp {
                    content: answer.content.clone(),
                    is_done,
                };
                break;
            }

            self.messages.extend(messages);
        }

        Ok(re_act_resp)
    }

    /// build new message
    /// <new messages, is_done>
    async fn build_new_message(&self, choice: Choice) -> anyhow::Result<(Vec<Message>, bool)> {
        match choice.message.tool_calls {
            Some(calls) => {
                // build assistant message
                let assistant_message = Message {
                    role: Role::Assistant,
                    tool_call_id: None,
                    content: choice.message.content.clone(),
                    tool_calls: Some(calls.clone()),
                };

                // tool call
                let mut tools: Vec<Message> = vec![assistant_message];
                for call in calls {
                    let tool = tool::get_tool(&call.function.name);
                    match tool {
                        Some(tool) => {
                            let res = tool.deep_seek_call(&call).await;

                            // set resp to messages
                            let tool: Message = Message {
                                role: Role::Tool,
                                tool_call_id: Some(call.id),
                                content: res.to_string(),
                                tool_calls: None,
                            };

                            tools.push(tool);
                        }
                        None => {
                            // set resp to messages
                            let tool: Message = Message {
                                role: Role::Tool,
                                tool_call_id: Some(call.id),
                                content: "tool not found".to_string(),
                                tool_calls: None,
                            };

                            tools.push(tool);
                        },
                    }
                }

                Ok((tools, false))
            }
            None => {
                // set resp to messages
                let re_act_resp: ReActResp = serde_json::from_str(&choice.message.content)?;
                let assistant: Message = Message::new(Role::Assistant, re_act_resp.content);

                Ok((vec![assistant], re_act_resp.is_done))
            }
        }
    }
}

impl Tool {
    pub fn new(function: Function) -> Self {
        Tool {
            r#type: "function".to_string(),
            function,
        }
    }
}

impl Parameters {
    pub fn new(properties: serde_json::Value, required: Vec<String>) -> Self {
        Parameters {
            r#type: "object".to_string(),
            properties,
            required,
        }
    }
}

impl Message {
    pub fn new(role: Role, content: String) -> Self {
        Message {
            role,
            content,
            tool_call_id: None,
            tool_calls: None,
        }
    }
}
