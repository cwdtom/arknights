use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::{browser_schema, new_base_tool, parse_tool_args, run_browser_result};
use serde::Deserialize;
use serde_json::json;

pub struct ClickTool {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClickArgs {
    element_id: String,
}

#[async_trait::async_trait]
impl LlmTool for ClickTool {
    fn group_name(&self) -> &str {
        &self.base_tool.group_name
    }

    fn name(&self) -> &str {
        self.base_tool.name.as_str()
    }

    fn deep_seek_schema(&self) -> crate::llm::base_llm::Function {
        browser_schema(
            &self.base_tool,
            json!({
                "element_id": {
                    "type": "string",
                    "description": "Element identifier to click",
                }
            }),
            &["element_id"],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: ClickArgs = match parse_tool_args(tool_call, self.base_tool.name.as_str()) {
            Ok(args) => args,
            Err(err) => return err,
        };

        run_browser_result("click", |session| async move {
            let mut driver = session.lock_driver().await;
            Ok(driver.click(&args.element_id).await)
        })
        .await
    }
}

impl ClickTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("click", "Click an element by element_id."),
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
    struct ClickFactory {
        last_element: Arc<Mutex<Option<String>>>,
    }

    struct ClickDriver {
        last_element: Arc<Mutex<Option<String>>>,
    }

    #[async_trait::async_trait]
    impl BrowserDriverFactory for ClickFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            Ok(Box::new(ClickDriver {
                last_element: self.last_element.clone(),
            }))
        }
    }

    #[async_trait::async_trait]
    impl BrowserDriver for ClickDriver {
        async fn navigate(&mut self, _url: &str) -> BrowserToolResult {
            panic!("unexpected navigate call")
        }

        async fn snapshot(&mut self) -> BrowserToolResult {
            panic!("unexpected snapshot call")
        }

        async fn click(&mut self, element_id: &str) -> BrowserToolResult {
            *self.last_element.lock().expect("lock poisoned") = Some(element_id.to_string());
            Ok(serde_json::json!({ "clicked": element_id }))
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
            panic!("unexpected get_text call")
        }

        async fn screenshot(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            panic!("unexpected screenshot call")
        }

        async fn close(&mut self) -> BrowserToolUnitResult {
            Ok(())
        }
    }

    fn click_call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call_click".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "browser_click".to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[test]
    fn click_schema_requires_element_id() {
        let tool = ClickTool::new();
        let schema = tool.deep_seek_schema();

        assert_eq!(schema.name, "browser_click");
        assert_eq!(schema.parameters.required, vec!["element_id".to_string()]);
        assert_eq!(schema.parameters.properties["element_id"]["type"], "string");
    }

    #[tokio::test]
    async fn click_returns_invalid_arguments_for_bad_json() {
        let tool = ClickTool::new();
        let result = tool.deep_seek_call(&click_call("{")).await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn click_rejects_unknown_arguments() {
        let tool = ClickTool::new();
        let result = tool
            .deep_seek_call(&click_call(r#"{"element_id":"node-1","extra":"x"}"#))
            .await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn click_calls_driver_and_wraps_result() {
        let factory = Arc::new(ClickFactory::default());
        let tool = ClickTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(&click_call(r#"{"element_id":"node-1"}"#))
                    .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["clicked"], "node-1");
        assert_eq!(
            *factory.last_element.lock().expect("lock poisoned"),
            Some("node-1".to_string())
        );
    }
}
