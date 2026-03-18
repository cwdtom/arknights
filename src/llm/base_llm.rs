use serde::{Deserialize, Serialize};

/// request body
#[derive(Serialize, Debug)]
pub struct Llm {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    pub max_tokens: u16,
    pub response_format: ResponseFormat,
    pub temperature: f32,
    pub tools: Vec<Tool>,
    pub tool_choice: String,
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

#[async_trait::async_trait]
pub trait LlmProvider: Send {
    async fn call(&mut self) -> anyhow::Result<ChatResponse>;

    fn push_message(&mut self, message: Message);

    fn extend_messages(&mut self, messages: Vec<Message>);
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
mod tests {
    use super::*;

    #[test]
    fn message_new_sets_defaults() {
        let msg = Message::new(Role::User, "hello".to_string());
        assert_eq!(msg.content, "hello");
        assert!(msg.tool_call_id.is_none());
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn tool_new_sets_type_function() {
        let func = Function {
            name: "test".to_string(),
            description: "desc".to_string(),
            parameters: Parameters::new(serde_json::json!({}), vec![]),
        };
        let tool = Tool::new(func);
        assert_eq!(tool.r#type, "function");
    }

    #[test]
    fn parameters_new_sets_type_object() {
        let params = Parameters::new(serde_json::json!({"a": "b"}), vec!["a".to_string()]);
        assert_eq!(params.r#type, "object");
        assert_eq!(params.required, vec!["a".to_string()]);
    }

    #[test]
    fn role_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Role::System).unwrap(), r#""system""#);
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), r#""user""#);
        assert_eq!(serde_json::to_string(&Role::Assistant).unwrap(), r#""assistant""#);
        assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), r#""tool""#);
    }

    #[test]
    fn message_serialization_skips_none_fields() {
        let msg = Message::new(Role::User, "hi".to_string());
        let json = serde_json::to_value(&msg).unwrap();
        assert!(!json.as_object().unwrap().contains_key("tool_call_id"));
        assert!(!json.as_object().unwrap().contains_key("tool_calls"));
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "hi");
    }

    #[test]
    fn message_deserialization_without_tool_calls() {
        let json = r#"{"role":"assistant","content":"hello"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content, "hello");
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn message_deserialization_with_tool_calls() {
        let json = r#"{
            "role":"assistant",
            "content":"",
            "tool_calls":[{
                "id":"call_1",
                "type":"function",
                "function":{"name":"test_fn","arguments":"{}"}
            }]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let calls = msg.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "test_fn");
    }

    #[test]
    fn chat_response_deserialization() {
        let json = r#"{
            "id":"chatcmpl-123",
            "choices":[{
                "message":{
                    "role":"assistant",
                    "content":"response text"
                }
            }]
        }"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "chatcmpl-123");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content, "response text");
    }

    #[test]
    fn tool_call_round_trip() {
        let tc = ToolCall {
            id: "call_abc".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "my_tool".to_string(),
                arguments: r#"{"key":"val"}"#.to_string(),
            },
        };
        let json = serde_json::to_string(&tc).unwrap();
        let tc2: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(tc2.id, "call_abc");
        assert_eq!(tc2.function.name, "my_tool");
        assert_eq!(tc2.function.arguments, r#"{"key":"val"}"#);
    }
}
