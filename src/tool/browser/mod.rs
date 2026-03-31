use crate::llm::base_llm::{Function, Parameters, ToolCall};
use crate::tool::base_tool::BaseTool;
use crate::tool::browser::error::{
    BrowserToolResult, browser_error_json, browser_tool_result_json,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;
use tracing::error;

pub const GROUP_NAME: &str = "browser";
pub const GROUP_DESC: &str = "Browser tools for real page interaction.";
const SESSION_ERROR_CODE: &str = "browser_session_error";

mod chromiumoxide_driver;
#[cfg(test)]
mod chromiumoxide_driver_tests;
mod chromiumoxide_runtime;
mod click;
pub(crate) mod driver;
pub(crate) mod error;
mod fill;
mod get_text;
mod navigate;
mod screenshot;
mod scroll;
mod session;
mod snapshot;
mod snapshot_js;
mod wait_text;

pub use click::ClickTool;
pub use fill::FillTool;
pub use get_text::GetTextTool;
pub use navigate::NavigateTool;
pub use screenshot::ScreenshotTool;
pub use scroll::ScrollTool;
pub use snapshot::SnapshotTool;
pub use wait_text::WaitTextTool;

pub(crate) async fn run_with_default_browser_scope<F, T>(future: F) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
{
    session::run_with_browser_scope(
        Arc::new(chromiumoxide_runtime::ChromiumoxideBrowserDriverFactory::new()),
        future,
    )
    .await
}

pub(crate) fn new_base_tool(name_suffix: &str, description: &str) -> BaseTool {
    BaseTool {
        group_name: GROUP_NAME.to_string(),
        group_description: GROUP_DESC.to_string(),
        name: format!("{}_{}", GROUP_NAME, name_suffix),
        description: description.to_string(),
    }
}

pub(crate) fn browser_schema(base_tool: &BaseTool, params: Value, required: &[&str]) -> Function {
    Function {
        name: base_tool.name.clone(),
        description: base_tool.description.clone(),
        parameters: Parameters::new(params, required.iter().map(|s| s.to_string()).collect()),
    }
}

pub(crate) fn invalid_arguments(err: impl std::fmt::Display) -> String {
    format!("Error: invalid arguments: {err}")
}

pub(crate) fn parse_tool_args<T: DeserializeOwned>(
    tool_call: &ToolCall,
    label: &str,
) -> Result<T, String> {
    serde_json::from_str(&tool_call.function.arguments).map_err(|err| {
        error!("failed to parse {} arguments: {:?}", label, err);
        invalid_arguments(err)
    })
}

pub(crate) async fn run_browser_result<F, Fut>(action: &str, operation: F) -> String
where
    F: FnOnce(Arc<session::BrowserSession>) -> Fut,
    Fut: Future<Output = anyhow::Result<BrowserToolResult>>,
{
    match session::with_browser_session(operation).await {
        Ok(result) => browser_tool_result_json(result),
        Err(err) => browser_session_error(action, err),
    }
}

fn browser_session_error(action: &str, err: anyhow::Error) -> String {
    error!("browser {} session failed: {:?}", action, err);
    browser_error_json(
        SESSION_ERROR_CODE,
        &format!("browser session error during {action}: {err}"),
    )
}
