pub(crate) mod base_tool;
pub(crate) mod system;

use base_tool::LlmTool;
use std::collections::HashMap;
use std::sync::LazyLock;

/// static tool registry: name -> Box<dyn LlmTool>
static TOOL_REGISTRY: LazyLock<HashMap<String, Box<dyn LlmTool + Send + Sync>>> =
    LazyLock::new(|| {
        let date = system::DateTool::new();

        let mut map: HashMap<String, Box<dyn LlmTool + Send + Sync>> = HashMap::new();
        map.insert(date.base_tool.name.clone(), Box::new(date));

        map
    });

/// get tool by name
pub fn get_tool(name: &str) -> Option<&(dyn LlmTool + Send + Sync)> {
    TOOL_REGISTRY.get(name).map(|t| t.as_ref())
}

/// get all tools
pub fn all_tools() -> Vec<&'static (dyn LlmTool + Send + Sync)> {
    TOOL_REGISTRY
        .values()
        .map(|t| t.as_ref())
        .collect()
}

/// get tool by group
pub fn get_tool_by_group(group: &str) -> Vec<&'static (dyn LlmTool + Send + Sync)> {
    TOOL_REGISTRY
        .values()
        .filter(|t| t.group_name() == group)
        .map(|t| t.as_ref())
        .collect()
}
