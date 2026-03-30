use crate::llm::base_llm::{Parameters, ToolCall};
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::{im, llm, timer};
use anyhow::anyhow;
use chrono::Local;
#[cfg(unix)]
use libc::{SIGKILL, killpg, pid_t};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::string::ToString;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{error, info};

const GROUP_NAME: &str = "system";
const GROUP_DESC: &str = "System tools(include `date`, `bash command`).";
const MAX_BASH_RESULT_LEN: usize = 2000;
const TRUNCATED_SUFFIX: &str = "\n...[truncated]";

#[derive(Serialize, Debug)]
pub struct DateTool {
    pub base_tool: BaseTool,
}

#[async_trait::async_trait]
impl LlmTool for DateTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(serde_json::json!({}), vec![]),
        }
    }

    async fn deep_seek_call(&self, _: &ToolCall) -> String {
        Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

impl DateTool {
    pub fn new() -> Self {
        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_date",
            description: "Get system current date, format: yyyy-MM-dd HH:mm:ss".to_string(),
        };

        DateTool { base_tool }
    }
}

#[derive(Serialize, Debug)]
pub struct BashTool {
    pub base_tool: BaseTool,
    pub enable: bool,
}

#[derive(Deserialize)]
struct BashToolArgs {
    command: String,
}

#[async_trait::async_trait]
impl LlmTool for BashTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> llm::base_llm::Function {
        llm::base_llm::Function {
            name: self.base_tool.name.clone(),
            description: self.base_tool.description.clone(),
            parameters: Parameters::new(
                serde_json::json!({
                    "command": {
                        "type": "string",
                        "description": "Single bash command."
                    }
                }),
                vec!["command".to_string()],
            ),
        }
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        if !self.enable {
            return "user not allowed execute bash command.".to_string();
        }

        let args: BashToolArgs = match serde_json::from_str(&tool_call.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse bash tool arguments: {:?}", e);
                return format!("Error: invalid arguments: {}", e);
            }
        };

        info!("execute bash command: {}", args.command);
        if timer::timer_service::get_thread_local_timer_id().is_none() {
            // just send first line
            im::base_im::async_send(format!(
                "EXEC {}",
                args.command.lines().next().unwrap_or("")
            ))
        }

        let mut command = Command::new("bash");
        #[cfg(unix)]
        command.process_group(0);

        let mut child = match command
            .arg("-c")
            .arg(args.command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(v) => v,
            Err(err) => {
                error!("failed to spawn bash: {}", err);
                return "fork child process failed".to_string();
            }
        };

        let stdout = match child.stdout.take() {
            Some(v) => v,
            None => {
                error!("cant extract stdout from child process");
                return "cant extract stdout from child process".to_string();
            }
        };
        let reader = BufReader::new(stdout);
        let stderr = match child.stderr.take() {
            Some(v) => v,
            None => {
                error!("cant extract stderr from child process");
                return "cant extract stderr from child process".to_string();
            }
        };
        let reader_err = BufReader::new(stderr);

        let stdout_task = tokio::spawn(read_stream_bounded(reader));
        let stderr_task = tokio::spawn(read_stream_bounded(reader_err));

        let status = match timeout(Duration::from_secs(300), child.wait()).await {
            Ok(Ok(status)) => status,
            Ok(Err(err)) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return format!("Error: wait bash failed: {err}");
            }
            Err(_) => {
                #[cfg(unix)]
                {
                    if let Err(err) = kill_child_process_group(&child) {
                        error!("kill bash process group failed: {}", err);
                        let _ = child.kill().await;
                    }
                }

                #[cfg(not(unix))]
                {
                    let _ = child.kill().await;
                }

                let _ = timeout(Duration::from_secs(2), child.wait()).await;

                stdout_task.abort();
                stderr_task.abort();
                let _ = stdout_task.await;
                let _ = stderr_task.await;

                return "Error: bash command timeout after 300s".to_string();
            }
        };

        let stdout = match stdout_task.await {
            Ok(v) => v,
            Err(err) => {
                return format!("Fetch command stdout error {err}");
            }
        };
        let stderr = match stderr_task.await {
            Ok(v) => v,
            Err(err) => {
                return format!("Fetch command stderr error {err}");
            }
        };

        let output = format!(
            r#"
            stdout:
            {stdout}

            stderr:
            {stderr}
        "#
        );
        let output = truncate_output(output);

        if !status.success() {
            format!("Error: bash exit code {:?}\n{}", status.code(), output)
        } else {
            output
        }
    }
}

async fn read_stream_bounded<R>(reader: BufReader<R>) -> String
where
    R: AsyncRead + Unpin,
{
    let mut lines = reader.lines();

    let mut res = String::with_capacity(MAX_BASH_RESULT_LEN);
    let mut size = 0;
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                size += line.len();
                if size <= MAX_BASH_RESULT_LEN {
                    res.push('\n');
                    res.push_str(&line);
                }
            }
            Ok(None) => {
                break;
            }
            Err(e) => {
                error!("bash command read error: {}", e);
                return format!("bash command read error, parts stdout:\n{res}");
            }
        }
    }

    if size > MAX_BASH_RESULT_LEN {
        res + "[truncated]"
    } else {
        res
    }
}

fn truncate_output(output: String) -> String {
    if output.len() <= MAX_BASH_RESULT_LEN {
        return output;
    }

    if MAX_BASH_RESULT_LEN <= TRUNCATED_SUFFIX.len() {
        return TRUNCATED_SUFFIX
            .chars()
            .take(MAX_BASH_RESULT_LEN)
            .collect();
    }

    let keep = MAX_BASH_RESULT_LEN - TRUNCATED_SUFFIX.len();
    let end = output
        .char_indices()
        .map(|(idx, ch)| idx + ch.len_utf8())
        .take_while(|next| *next <= keep)
        .last()
        .unwrap_or(0);

    format!("{}{}", &output[..end], TRUNCATED_SUFFIX)
}

#[cfg(unix)]
fn kill_child_process_group(child: &tokio::process::Child) -> anyhow::Result<()> {
    let pid = child
        .id()
        .ok_or_else(|| anyhow!("bash child pid unavailable"))? as pid_t;

    let rc = unsafe { killpg(pid, SIGKILL) };
    if rc == 0 {
        return Ok(());
    }

    Err(anyhow!(
        "kill bash process group failed: {}",
        std::io::Error::last_os_error()
    ))
}

impl BashTool {
    pub fn new() -> Self {
        let enable: bool = match std::env::var("BASH_TOOL_ENABLE") {
            Ok(s) => s.parse().unwrap_or(false),
            Err(_) => false,
        };

        let base_tool = BaseTool {
            group_name: GROUP_NAME.to_string(),
            group_description: GROUP_DESC.to_string(),
            name: GROUP_NAME.to_string() + "_bash",
            description: format!(
                r#"exec a single bash command, result max len {MAX_BASH_RESULT_LEN}, truncation exceeded.
                   All read and write operations should, to the greatest extent possible, occur only in the current path .cache directories.
                "#
            ),
        };

        BashTool { base_tool, enable }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::im::base_im::{self, Im};
    use crate::llm::base_llm::{FunctionCall, ToolCall};
    use crate::test_support;
    use std::sync::{Arc, Mutex};

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
        let dummy_call = ToolCall {
            id: "call_1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "system_date".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let result = tool.deep_seek_call(&dummy_call).await;
        // Should match format "2026-03-18 12:34:56"
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

        async fn reply_emoji(
            &mut self,
            _message_id: String,
            _emoji: String,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }
}
