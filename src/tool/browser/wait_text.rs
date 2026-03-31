use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::{browser_schema, new_base_tool, parse_tool_args, run_browser_result};
use serde::Deserialize;
use serde_json::json;

pub struct WaitTextTool {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitTextArgs {
    text: String,
    timeout_ms: Option<u64>,
}

#[async_trait::async_trait]
impl LlmTool for WaitTextTool {
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
                "text": {
                    "type": "string",
                    "description": "Text to wait for",
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds",
                }
            }),
            &["text"],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: WaitTextArgs = match parse_tool_args(tool_call, self.base_tool.name.as_str()) {
            Ok(args) => args,
            Err(err) => return err,
        };

        run_browser_result("wait_text", |session| async move {
            let mut driver = session.lock_driver().await;
            Ok(driver.wait_text(&args.text, args.timeout_ms).await)
        })
        .await
    }
}

impl WaitTextTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("wait_text", "Wait until text appears in current page."),
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

    type WaitTextCall = (String, Option<u64>);
    type WaitTextCallLog = Arc<Mutex<Option<WaitTextCall>>>;

    #[derive(Default)]
    struct WaitTextFactory {
        last_call: WaitTextCallLog,
    }

    struct WaitTextDriver {
        last_call: WaitTextCallLog,
    }

    #[async_trait::async_trait]
    impl BrowserDriverFactory for WaitTextFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            Ok(Box::new(WaitTextDriver {
                last_call: self.last_call.clone(),
            }))
        }
    }

    #[async_trait::async_trait]
    impl BrowserDriver for WaitTextDriver {
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

        async fn wait_text(&mut self, text: &str, timeout_ms: Option<u64>) -> BrowserToolResult {
            *self.last_call.lock().expect("lock poisoned") = Some((text.to_string(), timeout_ms));
            Ok(serde_json::json!({ "text": text, "timeout_ms": timeout_ms }))
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
            Err(BrowserToolError::new(
                "wait_text_timeout",
                "text did not appear before timeout",
            ))
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

    fn wait_text_call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call_wait_text".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "browser_wait_text".to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[test]
    fn wait_text_schema_requires_text() {
        let tool = WaitTextTool::new();
        let schema = tool.deep_seek_schema();

        assert_eq!(schema.name, "browser_wait_text");
        assert_eq!(schema.parameters.required, vec!["text".to_string()]);
        assert_eq!(schema.parameters.properties["text"]["type"], "string");
        assert_eq!(
            schema.parameters.properties["timeout_ms"]["type"],
            "integer"
        );
    }

    #[tokio::test]
    async fn wait_text_returns_invalid_arguments_for_bad_json() {
        let tool = WaitTextTool::new();
        let result = tool.deep_seek_call(&wait_text_call("{")).await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn wait_text_rejects_unknown_arguments() {
        let tool = WaitTextTool::new();
        let result = tool
            .deep_seek_call(&wait_text_call(
                r#"{"text":"ready","timeout_ms":5000,"extra":"x"}"#,
            ))
            .await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn wait_text_tool_forwards_timeout_ms() {
        let factory = Arc::new(WaitTextFactory::default());
        let tool = WaitTextTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(&wait_text_call(r#"{"text":"ready","timeout_ms":5000}"#))
                    .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["text"], "ready");
        assert_eq!(value["result"]["timeout_ms"], 5000);
        assert_eq!(
            *factory.last_call.lock().expect("lock poisoned"),
            Some(("ready".to_string(), Some(5000)))
        );
    }

    #[tokio::test]
    async fn wait_text_wraps_driver_error_as_browser_error_json() {
        let tool = WaitTextTool::new();
        let raw = run_with_browser_scope(Arc::new(ErrorFactory), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(&wait_text_call(r#"{"text":"ready"}"#))
                    .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "wait_text_timeout");
    }
}
