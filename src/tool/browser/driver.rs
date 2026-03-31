use crate::tool::browser::error::{BrowserToolResult, BrowserToolUnitResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrollRequest {
    Direction {
        direction: ScrollDirection,
        pages: u32,
    },
    Element {
        element_id: String,
    },
}

#[async_trait::async_trait]
pub trait BrowserDriver: Send {
    async fn navigate(&mut self, url: &str) -> BrowserToolResult;
    async fn snapshot(&mut self) -> BrowserToolResult;
    async fn click(&mut self, element_id: &str) -> BrowserToolResult;
    async fn fill(&mut self, element_id: &str, value: &str) -> BrowserToolResult;
    async fn scroll(&mut self, request: ScrollRequest) -> BrowserToolResult;
    async fn wait_text(&mut self, text: &str, timeout_ms: Option<u64>) -> BrowserToolResult;
    async fn get_text(&mut self, element_id: Option<&str>) -> BrowserToolResult;
    async fn screenshot(&mut self, element_id: Option<&str>) -> BrowserToolResult;
    async fn close(&mut self) -> BrowserToolUnitResult;
}

#[cfg(test)]
mod tests {
    use super::{BrowserDriver, ScrollDirection, ScrollRequest};
    use crate::tool::browser::error::{
        BrowserToolError, BrowserToolResult, BrowserToolUnitResult, browser_tool_error_json,
    };
    use serde_json::Value;

    struct FailingCloseDriver;

    #[async_trait::async_trait]
    impl super::BrowserDriver for FailingCloseDriver {
        async fn navigate(&mut self, _url: &str) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn snapshot(&mut self) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn click(&mut self, _element_id: &str) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn fill(&mut self, _element_id: &str, _value: &str) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn scroll(&mut self, _request: ScrollRequest) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn wait_text(&mut self, _text: &str, _timeout_ms: Option<u64>) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn get_text(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn screenshot(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn close(&mut self) -> BrowserToolUnitResult {
            Err(BrowserToolError::new(
                "session_not_found",
                "browser session already closed",
            ))
        }
    }

    #[test]
    fn scroll_request_element_keeps_element_id() {
        let request = ScrollRequest::Element {
            element_id: "node-1".to_string(),
        };

        assert_eq!(
            request,
            ScrollRequest::Element {
                element_id: "node-1".to_string(),
            }
        );
    }

    #[test]
    fn scroll_request_direction_keeps_explicit_fields() {
        let request = ScrollRequest::Direction {
            direction: ScrollDirection::Down,
            pages: 2,
        };

        assert_eq!(
            request,
            ScrollRequest::Direction {
                direction: ScrollDirection::Down,
                pages: 2,
            }
        );
    }

    #[tokio::test]
    async fn close_error_preserves_stable_error_contract() {
        let mut driver = FailingCloseDriver;
        let err = driver.close().await.unwrap_err();
        let raw = browser_tool_error_json(&err);
        let value: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "session_not_found");
        assert_eq!(value["error"]["message"], "browser session already closed");
    }
}
