pub(crate) mod base_tool;
pub(crate) mod system;
mod process_control;
mod internet;

use base_tool::LlmTool;
use std::collections::HashMap;
use std::sync::LazyLock;

/// static tool registry: name -> Box<dyn LlmTool>
static TOOL_REGISTRY: LazyLock<HashMap<String, Box<dyn LlmTool + Send + Sync>>> =
    LazyLock::new(|| {
        let date = system::DateTool::new();
        let ask_user = process_control::AskUser::new();
        let done = process_control::Done::new();
        let replan = process_control::Replan::new();
        let search = internet::Search::new();

        let mut map: HashMap<String, Box<dyn LlmTool + Send + Sync>> = HashMap::new();
        map.insert(date.base_tool.name.clone(), Box::new(date));
        map.insert(ask_user.base_tool.name.clone(), Box::new(ask_user));
        map.insert(done.base_tool.name.clone(), Box::new(done));
        map.insert(replan.base_tool.name.clone(), Box::new(replan));
        map.insert(search.base_tool.name.clone(), Box::new(search));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_tool_returns_some_for_system_date() {
        assert!(get_tool("system_date").is_some());
    }

    #[test]
    fn get_tool_returns_none_for_nonexistent() {
        assert!(get_tool("nonexistent").is_none());
    }

    #[test]
    fn all_tools_is_non_empty() {
        let tools = all_tools();
        assert!(!tools.is_empty());
        let names: Vec<_> = tools.iter().map(|t| t.deep_seek_schema().name).collect();
        assert!(names.contains(&"system_date".to_string()));
    }

    #[test]
    fn get_tool_by_group_system() {
        let tools = get_tool_by_group("system");
        assert!(!tools.is_empty());
        for t in &tools {
            assert_eq!(t.group_name(), "system");
        }
    }

    #[test]
    fn get_tool_by_group_nonexistent() {
        let tools = get_tool_by_group("nonexistent");
        assert!(tools.is_empty());
    }
}
