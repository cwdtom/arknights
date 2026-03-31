use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::{browser_schema, new_base_tool, parse_tool_args, run_browser_result};
use serde::Deserialize;
use serde_json::json;

pub struct NavigateTool {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NavigateArgs {
    url: String,
}

#[async_trait::async_trait]
impl LlmTool for NavigateTool {
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
                "url": {
                    "type": "string",
                    "description": "URL to navigate to",
                }
            }),
            &["url"],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: NavigateArgs = match parse_tool_args(tool_call, self.base_tool.name.as_str()) {
            Ok(args) => args,
            Err(err) => return err,
        };

        run_browser_result("navigate", |session| async move {
            let mut driver = session.lock_driver().await;
            Ok(driver.navigate(&args.url).await)
        })
        .await
    }
}

impl NavigateTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("navigate", "Navigate current page to a target URL."),
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
    struct RecordingFactory {
        last_url: Arc<Mutex<Option<String>>>,
    }

    #[async_trait::async_trait]
    impl BrowserDriverFactory for RecordingFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            Ok(Box::new(RecordingDriver {
                last_url: self.last_url.clone(),
            }))
        }
    }

    struct RecordingDriver {
        last_url: Arc<Mutex<Option<String>>>,
    }

    #[async_trait::async_trait]
    impl BrowserDriver for RecordingDriver {
        async fn navigate(&mut self, url: &str) -> BrowserToolResult {
            *self.last_url.lock().expect("lock poisoned") = Some(url.to_string());
            Ok(serde_json::json!({ "url": url }))
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

        async fn get_text(&mut self, _element_id: &str) -> BrowserToolResult {
            panic!("unexpected get_text call")
        }

        async fn screenshot(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            panic!("unexpected screenshot call")
        }

        async fn close(&mut self) -> BrowserToolUnitResult {
            Ok(())
        }
    }

    struct ErrorDriver;

    #[async_trait::async_trait]
    impl BrowserDriver for ErrorDriver {
        async fn navigate(&mut self, _url: &str) -> BrowserToolResult {
            Err(BrowserToolError::new(
                "navigate_failed",
                "failed to navigate",
            ))
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

        async fn get_text(&mut self, _element_id: &str) -> BrowserToolResult {
            panic!("unexpected get_text call")
        }

        async fn screenshot(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            panic!("unexpected screenshot call")
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

    fn navigate_call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call_navigate".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "browser_navigate".to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[test]
    fn navigate_schema_requires_url() {
        let tool = NavigateTool::new();
        let schema = tool.deep_seek_schema();

        assert_eq!(schema.name, "browser_navigate");
        assert_eq!(schema.parameters.required, vec!["url".to_string()]);
        assert_eq!(schema.parameters.properties["url"]["type"], "string");
    }

    #[tokio::test]
    async fn navigate_returns_invalid_arguments_for_bad_json() {
        let tool = NavigateTool::new();
        let result = tool.deep_seek_call(&navigate_call("{")).await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn navigate_rejects_unknown_arguments() {
        let tool = NavigateTool::new();
        let result = tool
            .deep_seek_call(&navigate_call(
                r#"{"url":"https://example.com","extra":"x"}"#,
            ))
            .await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn navigate_calls_driver_and_wraps_ok_result() {
        let factory = Arc::new(RecordingFactory::default());
        let tool = NavigateTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(&navigate_call(r#"{"url":"https://example.com"}"#))
                    .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["url"], "https://example.com");
        assert_eq!(
            *factory.last_url.lock().expect("lock poisoned"),
            Some("https://example.com".to_string())
        );
    }

    #[tokio::test]
    async fn navigate_wraps_driver_error_as_browser_error_json() {
        let tool = NavigateTool::new();
        let raw = run_with_browser_scope(Arc::new(ErrorFactory), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(&navigate_call(r#"{"url":"https://example.com"}"#))
                    .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "navigate_failed");
    }
}
