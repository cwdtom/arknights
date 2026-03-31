use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::{browser_schema, new_base_tool, parse_tool_args, run_browser_result};
use serde::Deserialize;
use serde_json::json;

pub struct FillTool {
    pub base_tool: BaseTool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FillArgs {
    element_id: String,
    value: String,
}

#[async_trait::async_trait]
impl LlmTool for FillTool {
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
                    "description": "Element identifier to fill",
                },
                "value": {
                    "type": "string",
                    "description": "Value to enter",
                }
            }),
            &["element_id", "value"],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: FillArgs = match parse_tool_args(tool_call, self.base_tool.name.as_str()) {
            Ok(args) => args,
            Err(err) => return err,
        };

        run_browser_result("fill", |session| async move {
            let mut driver = session.lock_driver().await;
            Ok(driver.fill(&args.element_id, &args.value).await)
        })
        .await
    }
}

impl FillTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("fill", "Fill an element value by element_id."),
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
    struct FillFactory {
        last_fill: Arc<Mutex<Option<(String, String)>>>,
    }

    struct FillDriver {
        last_fill: Arc<Mutex<Option<(String, String)>>>,
    }

    #[async_trait::async_trait]
    impl BrowserDriverFactory for FillFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            Ok(Box::new(FillDriver {
                last_fill: self.last_fill.clone(),
            }))
        }
    }

    #[async_trait::async_trait]
    impl BrowserDriver for FillDriver {
        async fn navigate(&mut self, _url: &str) -> BrowserToolResult {
            panic!("unexpected navigate call")
        }

        async fn snapshot(&mut self) -> BrowserToolResult {
            panic!("unexpected snapshot call")
        }

        async fn click(&mut self, _element_id: &str) -> BrowserToolResult {
            panic!("unexpected click call")
        }

        async fn fill(&mut self, element_id: &str, value: &str) -> BrowserToolResult {
            *self.last_fill.lock().expect("lock poisoned") =
                Some((element_id.to_string(), value.to_string()));
            Ok(serde_json::json!({ "element_id": element_id, "value": value }))
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

    fn fill_call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call_fill".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "browser_fill".to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[test]
    fn fill_schema_requires_element_id_and_value() {
        let tool = FillTool::new();
        let schema = tool.deep_seek_schema();

        assert_eq!(schema.name, "browser_fill");
        assert_eq!(
            schema.parameters.required,
            vec!["element_id".to_string(), "value".to_string()]
        );
        assert_eq!(schema.parameters.properties["element_id"]["type"], "string");
        assert_eq!(schema.parameters.properties["value"]["type"], "string");
    }

    #[tokio::test]
    async fn fill_returns_invalid_arguments_for_bad_json() {
        let tool = FillTool::new();
        let result = tool.deep_seek_call(&fill_call("{")).await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn fill_rejects_unknown_arguments() {
        let tool = FillTool::new();
        let result = tool
            .deep_seek_call(&fill_call(
                r#"{"element_id":"query","value":"arknights","extra":"x"}"#,
            ))
            .await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn fill_calls_driver_and_wraps_result() {
        let factory = Arc::new(FillFactory::default());
        let tool = FillTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(&fill_call(r#"{"element_id":"query","value":"arknights"}"#))
                    .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["element_id"], "query");
        assert_eq!(value["result"]["value"], "arknights");
        assert_eq!(
            *factory.last_fill.lock().expect("lock poisoned"),
            Some(("query".to_string(), "arknights".to_string()))
        );
    }
}
