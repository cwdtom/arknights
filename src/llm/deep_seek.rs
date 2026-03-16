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

#[derive(Deserialize, Debug, Clone)]
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

    pub async fn call(&mut self) -> anyhow::Result<ChatResponse> {
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

        Ok(serde_json::from_str(&raw)?)
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
