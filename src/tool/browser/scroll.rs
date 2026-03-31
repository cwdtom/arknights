use crate::llm::base_llm::ToolCall;
use crate::tool::base_tool::{BaseTool, LlmTool};
use crate::tool::browser::driver::{ScrollDirection, ScrollRequest};
use crate::tool::browser::{
    browser_schema, invalid_arguments, new_base_tool, parse_tool_args, run_browser_result,
};
use serde::Deserialize;
use serde_json::json;

pub struct ScrollTool {
    pub base_tool: BaseTool,
}

const DEFAULT_SCROLL_PAGES: u32 = 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScrollArgs {
    direction: Option<String>,
    pages: Option<u32>,
    element_id: Option<String>,
}

#[async_trait::async_trait]
impl LlmTool for ScrollTool {
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
                "direction": {
                    "type": "string",
                    "description": "Scroll direction",
                    "enum": ["up", "down"],
                },
                "pages": {
                    "type": "integer",
                    "description": "Number of pages to scroll",
                    "minimum": 1,
                },
                "element_id": {
                    "type": "string",
                    "description": "Element identifier to scroll within",
                }
            }),
            &[],
        )
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        let args: ScrollArgs = match parse_tool_args(tool_call, self.base_tool.name.as_str()) {
            Ok(args) => args,
            Err(err) => return err,
        };
        let request = match build_scroll_request(args) {
            Ok(request) => request,
            Err(err) => return invalid_arguments(err),
        };

        run_browser_result("scroll", |session| async move {
            let mut driver = session.lock_driver().await;
            Ok(driver.scroll(request).await)
        })
        .await
    }
}

impl ScrollTool {
    pub fn new() -> Self {
        Self {
            base_tool: new_base_tool("scroll", "Scroll by direction/pages or within element_id."),
        }
    }
}

fn build_scroll_request(args: ScrollArgs) -> Result<ScrollRequest, String> {
    if let Some(element_id) = args.element_id {
        return Ok(ScrollRequest::Element { element_id });
    }

    let direction = match args.direction.as_deref() {
        Some("up") => ScrollDirection::Up,
        Some("down") => ScrollDirection::Down,
        Some(value) => return Err(format!("unsupported direction: {value}")),
        None => return Err("provide `element_id` or `direction`".to_string()),
    };
    let pages = args.pages.unwrap_or(DEFAULT_SCROLL_PAGES);
    if pages == 0 {
        return Err("`pages` must be >= 1".to_string());
    }

    Ok(ScrollRequest::Direction { direction, pages })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::base_llm::{FunctionCall, ToolCall};
    use crate::tool::base_tool::LlmTool;
    use crate::tool::browser::driver::{BrowserDriver, ScrollDirection, ScrollRequest};
    use crate::tool::browser::error::{BrowserToolResult, BrowserToolUnitResult};
    use crate::tool::browser::session::{BrowserDriverFactory, run_with_browser_scope};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct ScrollFactory {
        last_request: Arc<Mutex<Option<ScrollRequest>>>,
    }

    struct ScrollDriver {
        last_request: Arc<Mutex<Option<ScrollRequest>>>,
    }

    #[async_trait::async_trait]
    impl BrowserDriverFactory for ScrollFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            Ok(Box::new(ScrollDriver {
                last_request: self.last_request.clone(),
            }))
        }
    }

    #[async_trait::async_trait]
    impl BrowserDriver for ScrollDriver {
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

        async fn scroll(&mut self, request: ScrollRequest) -> BrowserToolResult {
            *self.last_request.lock().expect("lock poisoned") = Some(request.clone());
            match request {
                ScrollRequest::Direction { direction, pages } => {
                    let direction = match direction {
                        ScrollDirection::Up => "up",
                        ScrollDirection::Down => "down",
                    };
                    Ok(serde_json::json!({ "direction": direction, "pages": pages }))
                }
                ScrollRequest::Element { element_id } => {
                    Ok(serde_json::json!({ "element_id": element_id }))
                }
            }
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

    fn scroll_call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call_scroll".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "browser_scroll".to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[test]
    fn scroll_schema_exposes_direction_pages_and_element() {
        let tool = ScrollTool::new();
        let schema = tool.deep_seek_schema();

        assert_eq!(schema.name, "browser_scroll");
        assert!(schema.parameters.required.is_empty());
        assert!(schema.parameters.properties["direction"]["enum"].is_array());
        assert!(schema.parameters.properties["pages"].is_object());
        assert_eq!(schema.parameters.properties["pages"]["minimum"], 1);
        assert!(schema.parameters.properties["element_id"].is_object());
    }

    #[tokio::test]
    async fn scroll_returns_invalid_arguments_for_bad_json() {
        let tool = ScrollTool::new();
        let result = tool.deep_seek_call(&scroll_call("{")).await;

        assert!(result.starts_with("Error: invalid arguments:"));
    }

    #[tokio::test]
    async fn scroll_direction_mode_calls_driver_with_direction_request() {
        let factory = Arc::new(ScrollFactory::default());
        let tool = ScrollTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(&scroll_call(r#"{"direction":"down","pages":2}"#))
                    .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["direction"], "down");
        assert_eq!(value["result"]["pages"], 2);
        assert_eq!(
            *factory.last_request.lock().expect("lock poisoned"),
            Some(ScrollRequest::Direction {
                direction: ScrollDirection::Down,
                pages: 2,
            })
        );
    }

    #[tokio::test]
    async fn scroll_element_mode_calls_driver_with_element_request() {
        let factory = Arc::new(ScrollFactory::default());
        let tool = ScrollTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(&scroll_call(r#"{"element_id":"panel-1"}"#))
                    .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["element_id"], "panel-1");
        assert_eq!(
            *factory.last_request.lock().expect("lock poisoned"),
            Some(ScrollRequest::Element {
                element_id: "panel-1".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn scroll_prefers_element_id_when_direction_and_pages_are_also_provided() {
        let factory = Arc::new(ScrollFactory::default());
        let tool = ScrollTool::new();

        let raw = run_with_browser_scope(factory.clone(), async {
            Ok::<_, anyhow::Error>(
                tool.deep_seek_call(
                    &scroll_call(r#"{"element_id":"panel-1","direction":"down","pages":2}"#),
                )
                .await,
            )
        })
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["element_id"], "panel-1");
        assert_eq!(
            *factory.last_request.lock().expect("lock poisoned"),
            Some(ScrollRequest::Element {
                element_id: "panel-1".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn scroll_rejects_missing_direction_and_element_id() {
        let tool = ScrollTool::new();
        let result = tool.deep_seek_call(&scroll_call("{}")).await;

        assert_eq!(
            result,
            "Error: invalid arguments: provide `element_id` or `direction`"
        );
    }

    #[tokio::test]
    async fn scroll_rejects_zero_pages() {
        let tool = ScrollTool::new();
        let result = tool
            .deep_seek_call(&scroll_call(r#"{"direction":"down","pages":0}"#))
            .await;

        assert_eq!(result, "Error: invalid arguments: `pages` must be >= 1");
    }
}
