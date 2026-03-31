use crate::tool::browser::driver::BrowserDriver;
use crate::tool::browser::error::BrowserToolError;
use crate::tool::browser::session::BrowserDriverFactory;
use anyhow::{Context, anyhow};
use chromiumoxide::Handler;
use chromiumoxide::browser::Browser;
use chromiumoxide::element::Element;
use chromiumoxide::page::Page;
use futures::StreamExt;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task::JoinHandle;

const STALE_ELEMENT_CODE: &str = "element_id_stale";
const STALE_ELEMENT_MESSAGE: &str = "call browser_snapshot again";

pub(crate) struct ChromiumoxideBrowserDriverFactory {
    scope_id: String,
}

impl ChromiumoxideBrowserDriverFactory {
    pub(crate) fn new() -> Self {
        Self {
            scope_id: unique_scope_id(),
        }
    }
}

#[async_trait::async_trait]
impl BrowserDriverFactory for ChromiumoxideBrowserDriverFactory {
    async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
        Ok(Box::new(
            crate::tool::browser::chromiumoxide_driver::ChromiumoxideBrowserDriver::launch(
                &self.scope_id,
            )
            .await?,
        ))
    }
}

pub(crate) struct ChromiumoxideRuntime {
    browser: Browser,
    handler_task: JoinHandle<anyhow::Result<()>>,
}

impl ChromiumoxideRuntime {
    pub(crate) fn new(browser: Browser, mut handler: Handler) -> Self {
        let handler_task = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                event?;
            }
            Ok(())
        });
        Self {
            browser,
            handler_task,
        }
    }

    pub(crate) async fn new_page(&self, url: &str) -> anyhow::Result<Page> {
        self.browser
            .new_page(url)
            .await
            .map_err(anyhow::Error::from)
    }

    pub(crate) async fn cleanup_launch_failure(self, launch_error: anyhow::Error) -> anyhow::Error {
        combine_cleanup_results(
            Err(launch_error),
            self.shutdown().await,
            "browser launch cleanup",
        )
        .unwrap_err()
    }

    pub(crate) async fn shutdown(self) -> anyhow::Result<()> {
        let browser_result = shutdown_browser_process(self.browser).await;
        let handler_result = shutdown_handler_task(self.handler_task).await;
        combine_cleanup_results(browser_result, handler_result, "browser runtime shutdown")
    }
}

pub(crate) async fn find_element(
    page: &Page,
    element_id: &str,
    failure_code: &str,
) -> Result<Element, BrowserToolError> {
    page.find_elements(format!(r#"[data-ark-id="{element_id}"]"#))
        .await
        .map_err(|err| op_error(failure_code, err))
        .and_then(take_first_or_stale)
}

pub(crate) fn op_error(code: &str, err: impl std::fmt::Display) -> BrowserToolError {
    BrowserToolError::new(code, err.to_string())
}

pub(crate) fn session_closed_error() -> BrowserToolError {
    BrowserToolError::new("session_not_found", "browser session already closed")
}

pub(crate) fn take_first_or_stale<T>(items: Vec<T>) -> Result<T, BrowserToolError> {
    items.into_iter().next().ok_or_else(stale_element_error)
}

fn stale_element_error() -> BrowserToolError {
    BrowserToolError::new(STALE_ELEMENT_CODE, STALE_ELEMENT_MESSAGE)
}

async fn shutdown_browser_process(mut browser: Browser) -> anyhow::Result<()> {
    let close_result = browser
        .close()
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from);
    if close_result.is_err() {
        kill_browser_process(&mut browser).await?;
    }
    let wait_result = browser
        .wait()
        .await
        .context("failed waiting for browser process")
        .map(|_| ());
    combine_cleanup_results(close_result, wait_result, "browser process shutdown")
}

async fn kill_browser_process(browser: &mut Browser) -> anyhow::Result<()> {
    match browser.kill().await {
        Some(result) => result.context("failed to kill browser process"),
        None => Ok(()),
    }
}

pub(crate) async fn shutdown_handler_task(
    handler_task: JoinHandle<anyhow::Result<()>>,
) -> anyhow::Result<()> {
    handler_task.abort();
    match handler_task.await {
        Ok(result) => result.context("browser handler task failed"),
        Err(err) if err.is_cancelled() => Ok(()),
        Err(err) => Err(anyhow!(err)),
    }
}

pub(crate) fn combine_cleanup_results(
    primary: anyhow::Result<()>,
    cleanup: anyhow::Result<()>,
    action: &str,
) -> anyhow::Result<()> {
    match (primary, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary_error), Ok(())) => Err(primary_error),
        (Ok(()), Err(cleanup_error)) => Err(cleanup_error),
        (Err(primary_error), Err(cleanup_error)) => Err(anyhow!(
            "{primary_error}; {action} also failed: {cleanup_error}"
        )),
    }
}

fn unique_scope_id() -> String {
    format!(
        "scope-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}
