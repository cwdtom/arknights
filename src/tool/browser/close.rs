use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::{
    browser_schema, new_base_tool, parse_tool_args, run_browser_close_result,
};
use serde::Deserialize;
use serde_json::json;

pub struct CloseTool {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
struct CloseArgs {}

#[async_trait::async_trait]
impl LlmTool for CloseTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> crate::llm::base_llm::Function {
        browser_schema(&self.base_tool, json!({}), &[])
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let _: CloseArgs = match parse_tool_args(tool_call, self.base_tool.name.as_str()) {
            Ok(args) => args,
            Err(err) => return err,
        };

        run_browser_close_result("close").await
    }
}

impl CloseTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("close", "Close the current browser session."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::base_llm::{FunctionCall, ToolCall};
    use crate::tool::base_tool::LlmTool;
    use crate::tool::browser::driver::{BrowserDriver, ScrollRequest};
    use crate::tool::browser::error::{BrowserToolResult, BrowserToolUnitResult};
    use crate::tool::browser::session::{BrowserDriverFactory, run_with_browser_scope};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct CloseFactory {
        close_count: Arc<Mutex<usize>>,
    }

    struct CloseDriver {
        close_count: Arc<Mutex<usize>>,
    }

    #[async_trait::async_trait]
    impl BrowserDriverFactory for CloseFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            Ok(Box::new(CloseDriver {
                close_count: self.close_count.clone(),
            }))
        }
    }

    #[async_trait::async_trait]
    impl BrowserDriver for CloseDriver {
        async fn navigate(&mut self, _url: &str) -> BrowserToolResult {
            panic!("unexpected navigate call")
        }

        async fn snapshot(&mut self) -> BrowserToolResult {
            panic!("unexpected snapshot call")
        }

        async fn click(&mut self, _element_id: &str) -> BrowserToolResult {
            panic!("unexpected click call")
        }

        async fn fill(&mut self, _element_id: &str, _value: &str) -> BrowserToolResult {
            panic!("unexpected fill call")
        }

        async fn scroll(&mut self, _request: ScrollRequest) -> BrowserToolResult {
            panic!("unexpected scroll call")
        }

        async fn wait_text(&mut self, _text: &str, _timeout_ms: Option<u64>) -> BrowserToolResult {
            panic!("unexpected wait_text call")
        }

        async fn get_text(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            panic!("unexpected get_text call")
        }

        async fn get_html(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            panic!("unexpected get_html call")
        }

        async fn screenshot(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            panic!("unexpected screenshot call")
        }

        async fn close(&mut self) -> BrowserToolUnitResult {
            let mut guard = self.close_count.lock().expect("lock poisoned");
            *guard += 1;
            Ok(())
        }
    }

    fn close_call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call_close".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "browser_close".to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[test]
    fn close_schema_has_no_required_fields() {
        let tool = CloseTool::new();
        let schema = tool.deep_seek_schema();

        assert_eq!(schema.name, "browser_close");
        assert!(schema.parameters.required.is_empty());
        assert_eq!(schema.parameters.properties, serde_json::json!({}));
    }

    #[tokio::test]
    async fn close_returns_invalid_arguments_for_bad_json() {
        let tool = CloseTool::new();
        let result = tool.deep_seek_call(&close_call("{")).await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn close_calls_driver_and_wraps_unit_result() {
        let factory = Arc::new(CloseFactory::default());
        let tool = CloseTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(tool.deep_seek_call(&close_call("{}")).await)
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"], serde_json::json!({}));
        assert_eq!(*factory.close_count.lock().expect("lock poisoned"), 1);
    }
}
