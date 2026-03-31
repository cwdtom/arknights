use serde_json::Value;

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
    async fn navigate(&mut self, url: &str) -> anyhow::Result<Value>;
    async fn snapshot(&mut self) -> anyhow::Result<Value>;
    async fn click(&mut self, element_id: &str) -> anyhow::Result<Value>;
    async fn fill(&mut self, element_id: &str, value: &str) -> anyhow::Result<Value>;
    async fn scroll(&mut self, request: ScrollRequest) -> anyhow::Result<Value>;
    async fn wait_text(&mut self, text: &str, timeout_ms: Option<u64>) -> anyhow::Result<Value>;
    async fn get_text(&mut self, element_id: Option<&str>) -> anyhow::Result<Value>;
    async fn get_html(&mut self, element_id: Option<&str>) -> anyhow::Result<Value>;
    async fn screenshot(&mut self, element_id: Option<&str>) -> anyhow::Result<Value>;
    async fn close(&mut self) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::{ScrollDirection, ScrollRequest};

    #[test]
    fn scroll_request_direction_keeps_explicit_fields() {
        let request = ScrollRequest::Direction {
            direction: ScrollDirection::Down,
            pages: 300,
        };

        assert_eq!(
            request,
            ScrollRequest::Direction {
                direction: ScrollDirection::Down,
                pages: 300,
            }
        );
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
}
