use crate::kv_service;
use anyhow::anyhow;

pub async fn execute(command: String) -> anyhow::Result<()> {
    let (command, arg) = command
        .trim()
        .split_once(" ")
        .ok_or_else(|| anyhow::anyhow!("invalid command"))?;

    match command {
        "/set_personal" => set_personal(arg.trim().to_string()).await,
        _ => Err(anyhow!("Invalid command")),
    }
}

async fn set_personal(text: String) -> anyhow::Result<()> {
    kv_service::set_personal_value(&text).await
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
