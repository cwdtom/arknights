use super::*;
use crate::test_support;
use crate::util::http_utils::mime_type_for_upload;
use std::fs::{File as StdFile, remove_file};

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

#[test]
fn mime_type_for_upload_uses_known_file_types() {
    assert_eq!(mime_type_for_upload("mp4", "测试视频.mp4"), "video/mp4");
    assert_eq!(mime_type_for_upload("png", "diagram.png"), "image/png");
    assert_eq!(
        mime_type_for_upload("unknown", "archive.bin"),
        "application/octet-stream"
    );
}

#[tokio::test]
async fn upload_file_rejects_files_larger_than_20mb() {
    const MAX_UPLOAD_FILE_SIZE_BYTES: u64 = 20 * 1024 * 1024;

    let _guard = test_support::app_test_guard().await;
    let path = std::env::temp_dir().join(format!(
        "{}.bin",
        test_support::unique_test_token("lark-tests", "oversize-upload")
    ));
    let file = StdFile::create(&path).unwrap();
    file.set_len(MAX_UPLOAD_FILE_SIZE_BYTES + 1).unwrap();

    let mut lark = Lark {
        access_token: "test-access-token".to_string(),
        update_time: chrono::Utc::now().timestamp(),
    };
    let result = lark
        .upload_file(File {
            r#type: "mp4".to_string(),
            name: "oversize.mp4".to_string(),
            path: path.to_string_lossy().into_owned(),
        })
        .await;

    remove_file(&path).unwrap();

    let err = result.unwrap_err().to_string();
    assert!(err.contains("20MB"));
}
