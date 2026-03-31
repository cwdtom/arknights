use crate::llm::base_llm::{Function, Parameters};
use crate::tool::base_tool::BaseTool;
use serde_json::Value;

pub const GROUP_NAME: &str = "browser";
pub const GROUP_DESC: &str = "Browser tools for real page interaction.";

mod click;
mod close;
pub(crate) mod driver;
pub(crate) mod error;
mod fill;
mod get_html;
mod get_text;
mod navigate;
mod screenshot;
mod scroll;
mod session;
mod snapshot;
mod wait_text;

pub use click::ClickTool;
pub use close::CloseTool;
pub use fill::FillTool;
pub use get_html::GetHtmlTool;
pub use get_text::GetTextTool;
pub use navigate::NavigateTool;
pub use screenshot::ScreenshotTool;
pub use scroll::ScrollTool;
pub use snapshot::SnapshotTool;
pub use wait_text::WaitTextTool;

pub(crate) use session::run_with_default_browser_scope;

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

pub(crate) fn placeholder_response(base_tool: &BaseTool) -> String {
    format!("Error: {} not implemented yet.", base_tool.name)
}
