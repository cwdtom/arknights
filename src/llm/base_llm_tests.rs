use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

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
    assert_eq!(
        serde_json::to_string(&Role::Assistant).unwrap(),
        r#""assistant""#
    );
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

#[tokio::test]
async fn llm_delegates_to_provider() {
    let recorded_messages = Arc::new(Mutex::new(vec![]));
    let mut llm = Llm {
        llm_provider: Box::new(TestLlmProvider::new(
            vec![chat_response("delegated-response")],
            recorded_messages.clone(),
        )),
    };

    llm.push_message(Message::new(Role::User, "first".to_string()));
    llm.extend_messages(vec![
        Message::new(Role::Assistant, "second".to_string()),
        Message::new(Role::Tool, "third".to_string()),
    ]);
    let response = llm.call().await.unwrap();

    assert_eq!(response.id, "test-chat-response");
    assert_eq!(response.choices[0].message.content, "delegated-response");
    let messages = recorded_messages.lock().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].content, "first");
    assert_eq!(messages[1].content, "second");
    assert_eq!(messages[2].content, "third");
}

struct TestLlmProvider {
    responses: VecDeque<ChatResponse>,
    recorded_messages: Arc<Mutex<Vec<Message>>>,
}

impl TestLlmProvider {
    fn new(responses: Vec<ChatResponse>, recorded_messages: Arc<Mutex<Vec<Message>>>) -> Self {
        Self {
            responses: responses.into(),
            recorded_messages,
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for TestLlmProvider {
    async fn call(&mut self) -> anyhow::Result<ChatResponse> {
        self.responses
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("test llm response queue is empty"))
    }

    fn push_message(&mut self, message: Message) {
        self.recorded_messages.lock().unwrap().push(message);
    }

    fn extend_messages(&mut self, messages: Vec<Message>) {
        self.recorded_messages.lock().unwrap().extend(messages);
    }
}

fn chat_response(content: &str) -> ChatResponse {
    serde_json::from_value(serde_json::json!({
        "id": "test-chat-response",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": content
            }
        }]
    }))
    .unwrap()
}
