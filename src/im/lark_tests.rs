use super::*;

#[test]
fn event_envelope_deserialization() {
    let json = r#"{
        "header": {
            "event_type": "im.message.receive_v1",
            "create_time": "1742374800"
        },
        "event": {
            "message": {
                "message_type": "text",
                "content": "{\"text\":\"hello world\"}",
                "message_id": "om_test_message"
            }
        }
    }"#;
    let envelope: EventEnvelope = serde_json::from_str(json).unwrap();
    assert_eq!(envelope.header.event_type, "im.message.receive_v1");
    assert_eq!(envelope.header.create_time, "1742374800");
    assert_eq!(envelope.event.message.message_type, "text");
    assert_eq!(envelope.event.message.message_id, "om_test_message");
    assert_eq!(envelope.event.message.content, r#"{"text":"hello world"}"#);
}

#[test]
fn text_content_deserialization() {
    let json = r#"{"text":"hello world"}"#;
    let content: TextContent = serde_json::from_str(json).unwrap();
    assert_eq!(content.text, "hello world");
}

#[test]
fn event_envelope_nested_text_extraction() {
    let json = r#"{
        "header": {
            "event_type": "im.message.receive_v1",
            "create_time": "1742374800"
        },
        "event": {
            "message": {
                "message_type": "text",
                "content": "{\"text\":\"test message\"}",
                "message_id": "om_test_message"
            }
        }
    }"#;
    let envelope: EventEnvelope = serde_json::from_str(json).unwrap();
    let text_content: TextContent = serde_json::from_str(&envelope.event.message.content).unwrap();
    assert_eq!(text_content.text, "test message");
}
