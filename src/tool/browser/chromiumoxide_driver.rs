use crate::tool::browser::chromiumoxide_runtime::{self, ChromiumoxideRuntime};
use crate::tool::browser::driver::{BrowserDriver, ScrollDirection, ScrollRequest};
use crate::tool::browser::error::{BrowserToolError, BrowserToolResult, BrowserToolUnitResult};
use crate::tool::browser::snapshot_js::SNAPSHOT_JS;
use anyhow::anyhow;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::Page;
use chromiumoxide::page::ScreenshotParams;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

const BROWSER_TIMEOUT: Duration = Duration::from_secs(60);
const SCREENSHOT_TYPE: &str = "image/png";

pub(crate) struct ChromiumoxideBrowserDriver {
    runtime: Option<ChromiumoxideRuntime>,
    page: Page,
    scope_dir: PathBuf,
    screenshot_index: u32,
}

impl ChromiumoxideBrowserDriver {
    pub(crate) async fn launch(scope_id: &str) -> anyhow::Result<Self> {
        let scope_dir = std::env::current_dir()?
            .join(".cache/browser")
            .join(scope_id);
        tokio::fs::create_dir_all(scope_dir.join("profile")).await?;
        let config = BrowserConfig::builder()
            .user_data_dir(scope_dir.join("profile"))
            .request_timeout(BROWSER_TIMEOUT)
            .build()
            .map_err(|err| anyhow!(err))?;
        let (browser, handler) = Browser::launch(config).await?;
        let runtime = ChromiumoxideRuntime::new(browser, handler);
        let page = match runtime.new_page("about:blank").await {
            Ok(page) => page,
            Err(err) => return Err(runtime.cleanup_launch_failure(err).await),
        };
        Ok(Self {
            runtime: Some(runtime),
            page,
            scope_dir,
            screenshot_index: 0,
        })
    }

    async fn next_screenshot_path(&mut self) -> Result<PathBuf, BrowserToolError> {
        self.screenshot_index += 1;
        let shots_dir = self.scope_dir.clone();
        tokio::fs::create_dir_all(&shots_dir)
            .await
            .map_err(|err| chromiumoxide_runtime::op_error("screenshot_failed", err))?;
        Ok(shots_dir.join(format!("shot-{:03}.png", self.screenshot_index)))
    }

    async fn wait_for_text(&self, text: &str, timeout_ms: Option<u64>) -> BrowserToolResult {
        let timeout =
            Duration::from_millis(timeout_ms.unwrap_or(BROWSER_TIMEOUT.as_millis() as u64));
        let deadline = tokio::time::Instant::now() + timeout;
        let quoted = serde_json::to_string(text)
            .map_err(|err| chromiumoxide_runtime::op_error("wait_text_failed", err))?;
        while tokio::time::Instant::now() <= deadline {
            let found = self
                .page
                .evaluate(format!(
                    "document.body && document.body.innerText.includes({quoted})"
                ))
                .await
                .map_err(|err| chromiumoxide_runtime::op_error("wait_text_failed", err))?
                .into_value::<bool>()
                .map_err(|err| chromiumoxide_runtime::op_error("wait_text_failed", err))?;
            if found {
                return Ok(
                    json!({ "text": text, "page": self.page_meta().await.map_err(|err| chromiumoxide_runtime::op_error("wait_text_failed", err))? }),
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(BrowserToolError::new(
            "wait_text_timeout",
            format!("text did not appear before timeout: {text}"),
        ))
    }

    async fn shutdown(&mut self) -> BrowserToolUnitResult {
        let runtime = self
            .runtime
            .take()
            .ok_or_else(chromiumoxide_runtime::session_closed_error)?;
        runtime
            .shutdown()
            .await
            .map_err(|err| chromiumoxide_runtime::op_error("close_failed", err))?;
        Ok(())
    }

    async fn page_meta(&self) -> anyhow::Result<Value> {
        Ok(json!({
            "url": self.page.url().await?,
            "title": self.page.get_title().await?,
        }))
    }
}

#[async_trait::async_trait]
impl BrowserDriver for ChromiumoxideBrowserDriver {
    async fn navigate(&mut self, url: &str) -> BrowserToolResult {
        self.page
            .goto(url)
            .await
            .map_err(|err| chromiumoxide_runtime::op_error("navigate_failed", err))?;
        Ok(
            json!({ "url": self.page.url().await.map_err(|err| chromiumoxide_runtime::op_error("navigate_failed", err))?, "page": self.page_meta().await.map_err(|err| chromiumoxide_runtime::op_error("navigate_failed", err))? }),
        )
    }

    async fn snapshot(&mut self) -> BrowserToolResult {
        self.page
            .evaluate_function(SNAPSHOT_JS)
            .await
            .map_err(|err| chromiumoxide_runtime::op_error("snapshot_failed", err))?
            .into_value::<Value>()
            .map_err(|err| chromiumoxide_runtime::op_error("snapshot_failed", err))
    }

    async fn click(&mut self, element_id: &str) -> BrowserToolResult {
        chromiumoxide_runtime::find_element(&self.page, element_id, "click_failed")
            .await?
            .click()
            .await
            .map_err(|err| chromiumoxide_runtime::op_error("click_failed", err))?;
        Ok(
            json!({ "element_id": element_id, "page": self.page_meta().await.map_err(|err| chromiumoxide_runtime::op_error("click_failed", err))? }),
        )
    }

    async fn fill(&mut self, element_id: &str, value: &str) -> BrowserToolResult {
        let script = format!(
            "function() {{ const value = {}; if ('value' in this) {{ this.value = value; }} else {{ this.textContent = value; }} this.dispatchEvent(new Event('input', {{ bubbles: true }})); this.dispatchEvent(new Event('change', {{ bubbles: true }})); return true; }}",
            serde_json::to_string(value)
                .map_err(|err| chromiumoxide_runtime::op_error("fill_failed", err))?
        );
        chromiumoxide_runtime::find_element(&self.page, element_id, "fill_failed")
            .await?
            .call_js_fn(script, true)
            .await
            .map_err(|err| chromiumoxide_runtime::op_error("fill_failed", err))?;
        Ok(
            json!({ "element_id": element_id, "value": value, "page": self.page_meta().await.map_err(|err| chromiumoxide_runtime::op_error("fill_failed", err))? }),
        )
    }

    async fn scroll(&mut self, request: ScrollRequest) -> BrowserToolResult {
        match request {
            ScrollRequest::Direction { direction, pages } => {
                let delta = match direction {
                    ScrollDirection::Up => -1_i64,
                    ScrollDirection::Down => 1_i64,
                } * i64::from(pages);
                self.page
                    .evaluate_function(format!(
                        "() => {{ window.scrollBy(0, window.innerHeight * {delta}); return true; }}"
                    ))
                    .await
                    .map_err(|err| chromiumoxide_runtime::op_error("scroll_failed", err))?;
                Ok(
                    json!({ "direction": if matches!(direction, ScrollDirection::Up) { "up" } else { "down" }, "pages": pages, "page": self.page_meta().await.map_err(|err| chromiumoxide_runtime::op_error("scroll_failed", err))? }),
                )
            }
            ScrollRequest::Element { element_id } => {
                chromiumoxide_runtime::find_element(&self.page, &element_id, "scroll_failed")
                    .await?
                    .scroll_into_view()
                    .await
                    .map_err(|err| chromiumoxide_runtime::op_error("scroll_failed", err))?;
                Ok(
                    json!({ "element_id": element_id, "page": self.page_meta().await.map_err(|err| chromiumoxide_runtime::op_error("scroll_failed", err))? }),
                )
            }
        }
    }

    async fn wait_text(&mut self, text: &str, timeout_ms: Option<u64>) -> BrowserToolResult {
        self.wait_for_text(text, timeout_ms).await
    }

    async fn get_text(&mut self, element_id: Option<&str>) -> BrowserToolResult {
        let text = match element_id {
            Some(id) => chromiumoxide_runtime::find_element(&self.page, id, "get_text_failed")
                .await?
                .inner_text()
                .await
                .map_err(|err| chromiumoxide_runtime::op_error("get_text_failed", err))?
                .unwrap_or_default(),
            None => self
                .page
                .evaluate("document.body ? document.body.innerText : ''")
                .await
                .map_err(|err| chromiumoxide_runtime::op_error("get_text_failed", err))?
                .into_value::<String>()
                .map_err(|err| chromiumoxide_runtime::op_error("get_text_failed", err))?,
        };
        Ok(
            json!({ "element_id": element_id, "text": text, "page": self.page_meta().await.map_err(|err| chromiumoxide_runtime::op_error("get_text_failed", err))? }),
        )
    }

    async fn get_html(&mut self, element_id: Option<&str>) -> BrowserToolResult {
        let html = match element_id {
            Some(id) => chromiumoxide_runtime::find_element(&self.page, id, "get_html_failed")
                .await?
                .outer_html()
                .await
                .map_err(|err| chromiumoxide_runtime::op_error("get_html_failed", err))?
                .unwrap_or_default(),
            None => self
                .page
                .content()
                .await
                .map_err(|err| chromiumoxide_runtime::op_error("get_html_failed", err))?,
        };
        Ok(
            json!({ "element_id": element_id, "html": html, "page": self.page_meta().await.map_err(|err| chromiumoxide_runtime::op_error("get_html_failed", err))? }),
        )
    }

    async fn screenshot(&mut self, element_id: Option<&str>) -> BrowserToolResult {
        let path = self.next_screenshot_path().await?;
        let params = ScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png)
            .build();
        match element_id {
            Some(id) => {
                let _image =
                    chromiumoxide_runtime::find_element(&self.page, id, "screenshot_failed")
                        .await?
                        .save_screenshot(CaptureScreenshotFormat::Png, &path)
                        .await
                        .map_err(|err| chromiumoxide_runtime::op_error("screenshot_failed", err))?;
            }
            None => {
                let _image = self
                    .page
                    .save_screenshot(params, path.as_path())
                    .await
                    .map_err(|err| chromiumoxide_runtime::op_error("screenshot_failed", err))?;
            }
        };
        Ok(
            json!({ "element_id": element_id, "path": path.to_string_lossy(), "type": SCREENSHOT_TYPE, "page": self.page_meta().await.map_err(|err| chromiumoxide_runtime::op_error("screenshot_failed", err))? }),
        )
    }

    async fn close(&mut self) -> BrowserToolUnitResult {
        self.shutdown().await
    }
}
