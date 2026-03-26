use crate::llm::deep_seek;
use serde::{Deserialize, Serialize};

/// base llm
pub struct Llm {
    pub llm_provider: Box<dyn LlmProvider>,
}

#[derive(Serialize, Debug)]
pub struct LlmBody {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    pub max_tokens: u16,
    pub response_format: ResponseFormat,
    pub temperature: f32,
    pub tools: Vec<Tool>,
    pub tool_choice: String,
}

#[async_trait::async_trait]
pub trait LlmProvider: Send {
    async fn call(&mut self) -> anyhow::Result<ChatResponse>;

    fn push_message(&mut self, message: Message);

    fn extend_messages(&mut self, messages: Vec<Message>);
}

impl Llm {
    pub fn new(messages: Vec<Message>, tools: Vec<Tool>) -> Self {
        // use deepseek
        deep_seek::init_deep_seek(messages, tools)
    }

    pub(crate) async fn call(&mut self) -> anyhow::Result<ChatResponse> {
        self.llm_provider.call().await
    }

    pub(crate) fn push_message(&mut self, message: Message) {
        self.llm_provider.push_message(message)
    }

    pub(crate) fn extend_messages(&mut self, messages: Vec<Message>) {
        self.llm_provider.extend_messages(messages)
    }
}

#[derive(Serialize, Debug)]
pub struct ResponseFormat {
    pub r#type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolResult {
    pub tool_call_id: Option<String>,
    pub content: String,
    pub done: bool,
    pub replan: bool,
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

#[cfg(test)]
#[path = "base_llm_tests.rs"]
mod tests;
