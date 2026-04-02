pub(crate) mod base_tool;
pub(crate) mod browser;
mod internet;
mod memory;
mod process_control;
mod schedule;
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
        let memory_search_tool = memory::SearchTool::new();
        let memory_list_tool = memory::ListTool::new();
        let memory_get_user_profile_tool = memory::GetUserProfileTool::new();
        let memory_rewrite_user_profile_tool = memory::RewriteUserProfileTool::new();
        let timer_get = timer::Get::new();
        let timer_list = timer::List::new();
        let timer_insert = timer::Insert::new();
        let timer_update = timer::Update::new();
        let timer_remove = timer::Remove::new();
        let browser_navigate = browser::NavigateTool::new();
        let browser_snapshot = browser::SnapshotTool::new();
        let browser_screenshot = browser::ScreenshotTool::new();
        let browser_click = browser::ClickTool::new();
        let browser_fill = browser::FillTool::new();
        let browser_get_text = browser::GetTextTool::new();
        let browser_scroll = browser::ScrollTool::new();
        let browser_wait_text = browser::WaitTextTool::new();
        let schedule_insert = schedule::Insert::new();
        let schedule_get = schedule::Get::new();
        let schedule_list = schedule::List::new();
        let schedule_search = schedule::Search::new();
        let schedule_list_by_tag = schedule::ListByTag::new();
        let schedule_update = schedule::Update::new();
        let schedule_remove = schedule::Remove::new();

        let mut map: HashMap<String, Box<dyn LlmTool + Send + Sync>> = HashMap::new();
        map.insert(date.base_tool.name.clone(), Box::new(date));
        map.insert(bash.base_tool.name.clone(), Box::new(bash));
        map.insert(ask_user.base_tool.name.clone(), Box::new(ask_user));
        map.insert(done.base_tool.name.clone(), Box::new(done));
        map.insert(replan.base_tool.name.clone(), Box::new(replan));
        map.insert(search.base_tool.name.clone(), Box::new(search));
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
        map.insert(
            browser_navigate.base_tool.name.clone(),
            Box::new(browser_navigate),
        );
        map.insert(
            browser_snapshot.base_tool.name.clone(),
            Box::new(browser_snapshot),
        );
        map.insert(
            browser_screenshot.base_tool.name.clone(),
            Box::new(browser_screenshot),
        );
        map.insert(
            browser_click.base_tool.name.clone(),
            Box::new(browser_click),
        );
        map.insert(browser_fill.base_tool.name.clone(), Box::new(browser_fill));
        map.insert(
            browser_get_text.base_tool.name.clone(),
            Box::new(browser_get_text),
        );
        map.insert(
            browser_scroll.base_tool.name.clone(),
            Box::new(browser_scroll),
        );
        map.insert(
            browser_wait_text.base_tool.name.clone(),
            Box::new(browser_wait_text),
        );
        map.insert(
            schedule_insert.base_tool.name.clone(),
            Box::new(schedule_insert),
        );
        map.insert(schedule_get.base_tool.name.clone(), Box::new(schedule_get));
        map.insert(
            schedule_list.base_tool.name.clone(),
            Box::new(schedule_list),
        );
        map.insert(
            schedule_search.base_tool.name.clone(),
            Box::new(schedule_search),
        );
        map.insert(
            schedule_list_by_tag.base_tool.name.clone(),
            Box::new(schedule_list_by_tag),
        );
        map.insert(
            schedule_update.base_tool.name.clone(),
            Box::new(schedule_update),
        );
        map.insert(
            schedule_remove.base_tool.name.clone(),
            Box::new(schedule_remove),
        );

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
        assert!(names.contains(&"schedule_insert".to_string()));
        assert!(names.contains(&"schedule_get".to_string()));
        assert!(names.contains(&"schedule_list".to_string()));
        assert!(names.contains(&"schedule_search".to_string()));
        assert!(names.contains(&"schedule_list_by_tag".to_string()));
        assert!(names.contains(&"schedule_update".to_string()));
        assert!(names.contains(&"schedule_remove".to_string()));
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
        assert!(get_tool("schedule_insert").is_some());
        assert!(get_tool("schedule_get").is_some());
        assert!(get_tool("schedule_list").is_some());
        assert!(get_tool("schedule_search").is_some());
        assert!(get_tool("schedule_list_by_tag").is_some());
        assert!(get_tool("schedule_update").is_some());
        assert!(get_tool("schedule_remove").is_some());
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

    #[test]
    fn get_tool_by_group_browser_includes_browser_tools() {
        let tools = get_tool_by_group("browser");
        let names: Vec<_> = tools.iter().map(|t| t.deep_seek_schema().name).collect();

        let expected: &[&str] = &[
            "browser_navigate",
            "browser_snapshot",
            "browser_screenshot",
            "browser_click",
            "browser_fill",
            "browser_get_text",
            "browser_scroll",
            "browser_wait_text",
        ];

        for name in expected {
            assert!(names.contains(&name.to_string()), "missing {}", name);
        }
        assert!(!names.contains(&"browser_get_html".to_string()));
        assert!(!names.contains(&"browser_close".to_string()));
    }

    #[test]
    fn get_tool_by_group_schedule_includes_schedule_tools() {
        let tools = get_tool_by_group("schedule");
        let names: Vec<_> = tools.iter().map(|t| t.deep_seek_schema().name).collect();

        assert!(names.contains(&"schedule_insert".to_string()));
        assert!(names.contains(&"schedule_get".to_string()));
        assert!(names.contains(&"schedule_list".to_string()));
        assert!(names.contains(&"schedule_search".to_string()));
        assert!(names.contains(&"schedule_list_by_tag".to_string()));
        assert!(names.contains(&"schedule_update".to_string()));
        assert!(names.contains(&"schedule_remove".to_string()));
    }
}
