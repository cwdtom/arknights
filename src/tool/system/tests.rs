use super::bash::MAX_BASH_RESULT_LEN;
use super::*;
use crate::im::base_im::{self, Im};
use crate::llm::base_llm::{FunctionCall, ToolCall};
use crate::test_support;
use crate::tool::base_tool::LlmTool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn date_tool_new_sets_correct_fields() {
    let tool = DateTool::new();
    assert_eq!(tool.base_tool.name, "system_date");
    assert_eq!(tool.base_tool.group_name, "system");
}

#[test]
fn date_tool_group_name() {
    let tool = DateTool::new();
    assert_eq!(tool.group_name(), "system");
}

#[test]
fn date_tool_schema() {
    let tool = DateTool::new();
    let schema = tool.deep_seek_schema();
    assert_eq!(schema.name, "system_date");
    assert!(!schema.description.is_empty());
    assert_eq!(schema.parameters.r#type, "object");
}

#[tokio::test]
async fn date_tool_call_returns_parseable_datetime() {
    let tool = DateTool::new();
    let result = tool.deep_seek_call(&tool_call("system_date", "{}")).await;

    assert_eq!(result.len(), 19);
    assert!(chrono::NaiveDateTime::parse_from_str(&result, "%Y-%m-%d %H:%M:%S").is_ok());
}

#[test]
fn bash_tool_new_uses_env_flag() {
    let _guard = test_support::lock_test_env();
    unsafe {
        std::env::set_var("BASH_TOOL_ENABLE", "true");
    }

    let tool = BashTool::new();

    assert!(tool.enable);

    unsafe {
        std::env::remove_var("BASH_TOOL_ENABLE");
    }
}

#[tokio::test]
async fn bash_tool_returns_permission_error_when_disabled() {
    let _guard = test_support::app_test_guard().await;
    unsafe {
        std::env::remove_var("BASH_TOOL_ENABLE");
    }
    install_fake_im(Arc::new(Mutex::new(Vec::new()))).await;

    let tool = BashTool::new();
    let result = tool
        .deep_seek_call(&tool_call("system_bash", r#"{"command":"echo hello"}"#))
        .await;

    assert_eq!(result, "user not allowed execute bash command.");
}

#[tokio::test]
async fn bash_tool_returns_error_with_exit_code_for_failed_command() {
    let _guard = test_support::app_test_guard().await;
    unsafe {
        std::env::set_var("BASH_TOOL_ENABLE", "true");
    }
    install_fake_im(Arc::new(Mutex::new(Vec::new()))).await;

    let tool = BashTool::new();
    let result = tool
        .deep_seek_call(&tool_call(
            "system_bash",
            r#"{"command":"echo boom >&2; exit 7"}"#,
        ))
        .await;

    assert!(result.starts_with("Error: bash exit code Some(7)"));
    assert!(result.contains("stderr:"));
    assert!(result.contains("boom"));

    unsafe {
        std::env::remove_var("BASH_TOOL_ENABLE");
    }
}

#[tokio::test]
async fn bash_tool_limits_combined_output_length() {
    let _guard = test_support::app_test_guard().await;
    unsafe {
        std::env::set_var("BASH_TOOL_ENABLE", "true");
    }
    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    install_fake_im(Arc::clone(&sent_messages)).await;

    let tool = BashTool::new();
    let result = tool
        .deep_seek_call(&tool_call(
            "system_bash",
            r#"{"command":"perl -e 'print \"a\" x 1500; print STDERR \"b\" x 1500'"}"#,
        ))
        .await;

    assert!(result.len() <= MAX_BASH_RESULT_LEN);
    assert!(result.contains("stdout:"));
    assert!(result.contains("stderr:"));

    test_support::wait_until_async("bash exec message", 20, Duration::from_millis(10), {
        let sent_messages = Arc::clone(&sent_messages);
        move || {
            let sent_messages = Arc::clone(&sent_messages);
            async move { Ok(!sent_messages.lock().unwrap().is_empty()) }
        }
    })
    .await
    .unwrap();

    assert!(
        sent_messages
            .lock()
            .unwrap()
            .iter()
            .any(|message| message.starts_with("EXEC perl -e"))
    );

    unsafe {
        std::env::remove_var("BASH_TOOL_ENABLE");
    }
}

fn tool_call(name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: format!("call_{name}"),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: name.to_string(),
            arguments: arguments.to_string(),
        },
    }
}

async fn install_fake_im(sent_messages: Arc<Mutex<Vec<String>>>) {
    base_im::install_test_im(Box::new(FakeIm { sent_messages })).await;
}

struct FakeIm {
    sent_messages: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl Im for FakeIm {
    async fn send(&mut self, content: String) -> anyhow::Result<()> {
        self.sent_messages.lock().unwrap().push(content);
        Ok(())
    }

    async fn ask_user(&mut self, _question: String) -> anyhow::Result<String> {
        anyhow::bail!("ask_user should not be called in BashTool tests")
    }

    async fn reply_emoji(&mut self, _message_id: String, _emoji: String) -> anyhow::Result<()> {
        Ok(())
    }
}
