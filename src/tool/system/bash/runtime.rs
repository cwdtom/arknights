use super::{BASH_COMMAND_TIMEOUT, BASH_KILL_WAIT_TIMEOUT, MAX_BASH_RESULT_LEN, TRUNCATED_SUFFIX};
use crate::{im, timer};
use anyhow::anyhow;
#[cfg(unix)]
use libc::{SIGKILL, killpg, pid_t};
use std::process::{ExitStatus, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{error, info};

pub(super) async fn run_bash_command(command_text: String) -> String {
    info!("execute bash command: {}", command_text);
    announce_command_start(&command_text);

    let mut command = match spawn_bash_command(command_text) {
        Ok(command) => command,
        Err(message) => return message,
    };

    let status = match wait_for_command(&mut command).await {
        Ok(status) => status,
        Err(message) => return message,
    };

    build_command_result(command, status).await
}

fn announce_command_start(command_text: &str) {
    if timer::timer_service::get_thread_local_timer_id().is_none() {
        im::base_im::async_send(format!(
            "EXEC {}",
            command_text.lines().next().unwrap_or("")
        ));
    }
}

fn spawn_bash_command(command_text: String) -> Result<SpawnedCommand, String> {
    let mut command = Command::new("bash");
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command
        .arg("-c")
        .arg(command_text)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            error!("failed to spawn bash: {}", err);
            "fork child process failed".to_string()
        })?;

    let stdout = take_stdout(&mut child)?;
    let stderr = take_stderr(&mut child)?;

    Ok(SpawnedCommand {
        child,
        stdout_task: Some(spawn_stream_reader(stdout)),
        stderr_task: Some(spawn_stream_reader(stderr)),
    })
}

fn take_stdout(child: &mut Child) -> Result<BufReader<ChildStdout>, String> {
    child.stdout.take().map(BufReader::new).ok_or_else(|| {
        error!("cant extract stdout from child process");
        "cant extract stdout from child process".to_string()
    })
}

fn take_stderr(child: &mut Child) -> Result<BufReader<ChildStderr>, String> {
    child.stderr.take().map(BufReader::new).ok_or_else(|| {
        error!("cant extract stderr from child process");
        "cant extract stderr from child process".to_string()
    })
}

fn spawn_stream_reader<R>(reader: BufReader<R>) -> JoinHandle<String>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(read_stream_bounded(reader))
}

async fn wait_for_command(command: &mut SpawnedCommand) -> Result<ExitStatus, String> {
    match timeout(BASH_COMMAND_TIMEOUT, command.child.wait()).await {
        Ok(Ok(status)) => Ok(status),
        Ok(Err(err)) => {
            cleanup_child(command).await;
            Err(format!("Error: wait bash failed: {err}"))
        }
        Err(_) => handle_command_timeout(command).await,
    }
}

async fn cleanup_child(command: &mut SpawnedCommand) {
    let _ = command.child.kill().await;
    let _ = command.child.wait().await;
    let _ = await_optional_task(command.stdout_task.take()).await;
    let _ = await_optional_task(command.stderr_task.take()).await;
}

async fn handle_command_timeout(command: &mut SpawnedCommand) -> Result<ExitStatus, String> {
    #[cfg(unix)]
    {
        if let Err(err) = kill_child_process_group(&command.child) {
            error!("kill bash process group failed: {}", err);
            let _ = command.child.kill().await;
        }
    }

    #[cfg(not(unix))]
    {
        let _ = command.child.kill().await;
    }

    let _ = timeout(BASH_KILL_WAIT_TIMEOUT, command.child.wait()).await;

    abort_optional_task(command.stdout_task.take()).await;
    abort_optional_task(command.stderr_task.take()).await;

    Err("Error: bash command timeout after 300s".to_string())
}

async fn build_command_result(mut command: SpawnedCommand, status: ExitStatus) -> String {
    let stdout = match join_stream_output(command.stdout_task.take(), "stdout").await {
        Ok(stdout) => stdout,
        Err(message) => return message,
    };
    let stderr = match join_stream_output(command.stderr_task.take(), "stderr").await {
        Ok(stderr) => stderr,
        Err(message) => return message,
    };

    let output = truncate_output(format!(
        r#"
            stdout:
            {stdout}

            stderr:
            {stderr}
        "#
    ));

    if status.success() {
        output
    } else {
        format!("Error: bash exit code {:?}\n{}", status.code(), output)
    }
}

async fn join_stream_output(
    task: Option<JoinHandle<String>>,
    stream_name: &str,
) -> Result<String, String> {
    let task = task.ok_or_else(|| format!("missing command {stream_name} task"))?;
    task.await
        .map_err(|err| format!("Fetch command {stream_name} error {err}"))
}

async fn await_optional_task(
    task: Option<JoinHandle<String>>,
) -> Result<(), tokio::task::JoinError> {
    if let Some(task) = task {
        let _ = task.await?;
    }

    Ok(())
}

async fn abort_optional_task(task: Option<JoinHandle<String>>) {
    if let Some(task) = task {
        task.abort();
        let _ = task.await;
    }
}

async fn read_stream_bounded<R>(reader: BufReader<R>) -> String
where
    R: AsyncRead + Unpin,
{
    let mut lines = reader.lines();
    let mut output = String::with_capacity(MAX_BASH_RESULT_LEN);
    let mut size = 0;

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                size += line.len();
                if size <= MAX_BASH_RESULT_LEN {
                    output.push('\n');
                    output.push_str(&line);
                }
            }
            Ok(None) => break,
            Err(err) => {
                error!("bash command read error: {}", err);
                return format!("bash command read error, parts stdout:\n{output}");
            }
        }
    }

    if size > MAX_BASH_RESULT_LEN {
        output + "[truncated]"
    } else {
        output
    }
}

fn truncate_output(output: String) -> String {
    if output.len() <= MAX_BASH_RESULT_LEN {
        return output;
    }

    if MAX_BASH_RESULT_LEN <= TRUNCATED_SUFFIX.len() {
        return TRUNCATED_SUFFIX.chars().take(MAX_BASH_RESULT_LEN).collect();
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
fn kill_child_process_group(child: &Child) -> anyhow::Result<()> {
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

struct SpawnedCommand {
    child: Child,
    stdout_task: Option<JoinHandle<String>>,
    stderr_task: Option<JoinHandle<String>>,
}
