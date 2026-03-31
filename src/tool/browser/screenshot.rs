use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::{browser_schema, new_base_tool, parse_tool_args, run_browser_result};
use serde::Deserialize;
use serde_json::json;
pub struct ScreenshotTool {
    pub base_tool: BaseTool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotArgs {
    element_id: Option<String>,
}
#[async_trait::async_trait]
impl LlmTool for ScreenshotTool {
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
                    "description": "Optional element identifier",
                }
            }),
            &[],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: ScreenshotArgs = match parse_tool_args(tool_call, self.base_tool.name.as_str()) {
            Ok(args) => args,
            Err(err) => return err,
        };

        run_browser_result("screenshot", |session| async move {
            let mut driver = session.lock_driver().await;
            Ok(driver.screenshot(args.element_id.as_deref()).await)
        })
        .await
    }
}

impl ScreenshotTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("screenshot", "Capture screenshot of page or element."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::base_llm::{FunctionCall, ToolCall};
    use crate::tool::base_tool::LlmTool;
    use crate::tool::browser::driver::{BrowserDriver, ScrollRequest};
    use crate::tool::browser::error::{BrowserToolError, BrowserToolResult, BrowserToolUnitResult};
    use crate::tool::browser::session::{BrowserDriverFactory, run_with_browser_scope};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct ScreenshotFactory {
        last_element_id: Arc<Mutex<Option<Option<String>>>>,
    }
    const SCREENSHOT_PATH: &str = "/tmp/ark-browser/shot-001.png";
    struct ScreenshotDriver {
        last_element_id: Arc<Mutex<Option<Option<String>>>>,
    }

    #[async_trait::async_trait]
    impl BrowserDriverFactory for ScreenshotFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            Ok(Box::new(ScreenshotDriver {
                last_element_id: self.last_element_id.clone(),
            }))
        }
    }

    #[async_trait::async_trait]
    impl BrowserDriver for ScreenshotDriver {
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

        async fn screenshot(&mut self, element_id: Option<&str>) -> BrowserToolResult {
            *self.last_element_id.lock().expect("lock poisoned") =
                Some(element_id.map(ToString::to_string));
            Ok(serde_json::json!({
                "path": SCREENSHOT_PATH,
                "type": "image/png",
                "title": "Example",
                "element_id": element_id
            }))
        }

        async fn close(&mut self) -> BrowserToolUnitResult {
            Ok(())
        }
    }

    struct ErrorDriver;

    #[async_trait::async_trait]
    impl BrowserDriver for ErrorDriver {
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

        async fn screenshot(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            Err(BrowserToolError::new(
                "screenshot_failed",
                "failed to capture screenshot",
            ))
        }

        async fn close(&mut self) -> BrowserToolUnitResult {
            Ok(())
        }
    }

    struct ErrorFactory;

    #[async_trait::async_trait]
    impl BrowserDriverFactory for ErrorFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            Ok(Box::new(ErrorDriver))
        }
    }

    fn screenshot_call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call_screenshot".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "browser_screenshot".to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[test]
    fn screenshot_schema_exposes_optional_element_id() {
        let tool = ScreenshotTool::new();
        let schema = tool.deep_seek_schema();

        assert_eq!(schema.name, "browser_screenshot");
        assert!(schema.parameters.required.is_empty());
        assert_eq!(schema.parameters.properties["element_id"]["type"], "string");
    }

    #[tokio::test]
    async fn screenshot_returns_invalid_arguments_for_bad_json() {
        let tool = ScreenshotTool::new();
        let result = tool.deep_seek_call(&screenshot_call("{")).await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn screenshot_rejects_unknown_arguments() {
        let tool = ScreenshotTool::new();
        let result = tool
            .deep_seek_call(&screenshot_call(r#"{"element_id":"node-1","extra":"x"}"#))
            .await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn screenshot_tool_returns_absolute_file_path() {
        let factory = Arc::new(ScreenshotFactory::default());
        let tool = ScreenshotTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(tool.deep_seek_call(&screenshot_call("{}")).await)
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["path"], SCREENSHOT_PATH);
        assert_eq!(value["result"]["type"], "image/png");
        assert_eq!(value["result"]["title"], "Example");
        assert!(value["result"]["element_id"].is_null());
        assert_eq!(
            *factory.last_element_id.lock().expect("lock poisoned"),
            Some(None)
        );
    }

    #[tokio::test]
    async fn screenshot_calls_driver_with_element_id() {
        let factory = Arc::new(ScreenshotFactory::default());
        let tool = ScreenshotTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(&screenshot_call(r#"{"element_id":"node-1"}"#))
                    .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["path"], SCREENSHOT_PATH);
        assert_eq!(value["result"]["type"], "image/png");
        assert_eq!(value["result"]["title"], "Example");
        assert_eq!(value["result"]["element_id"], "node-1");
        assert_eq!(
            *factory.last_element_id.lock().expect("lock poisoned"),
            Some(Some("node-1".to_string()))
        );
    }

    #[tokio::test]
    async fn screenshot_wraps_driver_error_as_browser_error_json() {
        let tool = ScreenshotTool::new();
        let raw = run_with_browser_scope(Arc::new(ErrorFactory), async {
            Ok::<_, anyhow::Error>(tool.deep_seek_call(&screenshot_call("{}")).await)
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "screenshot_failed");
    }
}
