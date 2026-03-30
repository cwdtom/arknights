pub(crate) mod base_tool;
mod internet;
mod memory;
mod process_control;
pub(crate) mod system;
mod timer;

use base_tool::LlmTool;
use std::collections::HashMap;
use std::sync::LazyLock;

/// static tool registry: name -> Box<dyn LlmTool>
static TOOL_REGISTRY: LazyLock<HashMap<String, Box<dyn LlmTool + Send + Sync>>> =
    LazyLock::new(|| {
        let date = system::DateTool::new();
        let bash = system::BashTool::new();
        let ask_user = process_control::AskUser::new();
        let done = process_control::Done::new();
        let replan = process_control::Replan::new();
        let search = internet::Search::new();
        let curl = internet::Curl::new();
        let memory_search_tool = memory::SearchTool::new();
        let memory_list_tool = memory::ListTool::new();
        let memory_get_user_profile_tool = memory::GetUserProfileTool::new();
        let memory_rewrite_user_profile_tool = memory::RewriteUserProfileTool::new();
        let timer_get = timer::Get::new();
        let timer_list = timer::List::new();
        let timer_insert = timer::Insert::new();
        let timer_update = timer::Update::new();
        let timer_remove = timer::Remove::new();

        let mut map: HashMap<String, Box<dyn LlmTool + Send + Sync>> = HashMap::new();
        map.insert(date.base_tool.name.clone(), Box::new(date));
        map.insert(bash.base_tool.name.clone(), Box::new(bash));
        map.insert(ask_user.base_tool.name.clone(), Box::new(ask_user));
        map.insert(done.base_tool.name.clone(), Box::new(done));
        map.insert(replan.base_tool.name.clone(), Box::new(replan));
        map.insert(search.base_tool.name.clone(), Box::new(search));
        map.insert(curl.base_tool.name.clone(), Box::new(curl));
        map.insert(
            memory_search_tool.base_tool.name.clone(),
            Box::new(memory_search_tool),
        );
        map.insert(
            memory_list_tool.base_tool.name.clone(),
            Box::new(memory_list_tool),
        );
        map.insert(
            memory_get_user_profile_tool.base_tool.name.clone(),
            Box::new(memory_get_user_profile_tool),
        );
        map.insert(
            memory_rewrite_user_profile_tool.base_tool.name.clone(),
            Box::new(memory_rewrite_user_profile_tool),
        );
        map.insert(timer_get.base_tool.name.clone(), Box::new(timer_get));
        map.insert(timer_list.base_tool.name.clone(), Box::new(timer_list));
        map.insert(timer_insert.base_tool.name.clone(), Box::new(timer_insert));
        map.insert(timer_update.base_tool.name.clone(), Box::new(timer_update));
        map.insert(timer_remove.base_tool.name.clone(), Box::new(timer_remove));

        map
    });

/// get tool by name
pub fn get_tool(name: &str) -> Option<&(dyn LlmTool + Send + Sync)> {
    TOOL_REGISTRY.get(name).map(|t| t.as_ref())
}

/// get all tools
pub fn all_tools() -> Vec<&'static (dyn LlmTool + Send + Sync)> {
    TOOL_REGISTRY.values().map(|t| t.as_ref()).collect()
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
        assert!(names.contains(&"memory_get_user_profile".to_string()));
        assert!(names.contains(&"memory_rewrite_user_profile".to_string()));
        assert!(names.contains(&"timer_get".to_string()));
        assert!(names.contains(&"timer_list".to_string()));
        assert!(names.contains(&"timer_insert".to_string()));
        assert!(names.contains(&"timer_update".to_string()));
        assert!(names.contains(&"timer_remove".to_string()));
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

    #[test]
    fn get_tool_returns_some_for_user_profile_tools() {
        assert!(get_tool("memory_get_user_profile").is_some());
        assert!(get_tool("memory_rewrite_user_profile").is_some());
        assert!(get_tool("timer_get").is_some());
        assert!(get_tool("timer_list").is_some());
        assert!(get_tool("timer_insert").is_some());
        assert!(get_tool("timer_update").is_some());
        assert!(get_tool("timer_remove").is_some());
    }

    #[test]
    fn get_tool_by_group_memory_includes_user_profile_tools() {
        let tools = get_tool_by_group("memory");
        let names: Vec<_> = tools.iter().map(|t| t.deep_seek_schema().name).collect();

        assert!(names.contains(&"memory_get_user_profile".to_string()));
        assert!(names.contains(&"memory_rewrite_user_profile".to_string()));
    }

    #[test]
    fn get_tool_by_group_timer_includes_timer_tools() {
        let tools = get_tool_by_group("timer");
        let names: Vec<_> = tools.iter().map(|t| t.deep_seek_schema().name).collect();

        assert!(names.contains(&"timer_get".to_string()));
        assert!(names.contains(&"timer_list".to_string()));
        assert!(names.contains(&"timer_insert".to_string()));
        assert!(names.contains(&"timer_update".to_string()));
        assert!(names.contains(&"timer_remove".to_string()));
    }
}
