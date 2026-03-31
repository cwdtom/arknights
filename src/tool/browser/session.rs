use crate::tool::browser::driver::BrowserDriver;
use anyhow::anyhow;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;

tokio::task_local! {
    static BROWSER_SCOPE: Arc<BrowserScope>;
}

#[async_trait::async_trait]
pub trait BrowserDriverFactory: Send + Sync {
    async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>>;
}

pub struct BrowserSession {
    driver: Mutex<Box<dyn BrowserDriver>>,
}

impl BrowserSession {
    fn new(driver: Box<dyn BrowserDriver>) -> Self {
        Self {
            driver: Mutex::new(driver),
        }
    }

    pub async fn lock_driver(&self) -> tokio::sync::MutexGuard<'_, Box<dyn BrowserDriver>> {
        self.driver.lock().await
    }

    async fn close(&self) -> anyhow::Result<()> {
        let mut driver = self.lock_driver().await;
        driver.close().await.map_err(browser_close_error)
    }
}

pub struct BrowserScope {
    session: Mutex<Option<Arc<BrowserSession>>>,
    factory: Arc<dyn BrowserDriverFactory>,
}

impl BrowserScope {
    fn new(factory: Arc<dyn BrowserDriverFactory>) -> Self {
        Self {
            session: Mutex::new(None),
            factory,
        }
    }

    async fn get_or_create_session(&self) -> anyhow::Result<Arc<BrowserSession>> {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.as_ref() {
            return Ok(session.clone());
        }
        let driver = self.factory.create().await?;
        let session = Arc::new(BrowserSession::new(driver));
        guard.replace(session.clone());
        Ok(session)
    }

    async fn close(&self) -> anyhow::Result<()> {
        let session = {
            let mut guard = self.session.lock().await;
            guard.take()
        };
        if let Some(session) = session {
            session.close().await?;
        }
        Ok(())
    }
}

fn browser_close_error(error: crate::tool::browser::error::BrowserToolError) -> anyhow::Error {
    anyhow!(
        "browser session close failed (code: {}): {}",
        error.code,
        error.message
    )
}

#[derive(Debug)]
struct MainExecutionErrorWithCleanupContext {
    main: anyhow::Error,
    cleanup: anyhow::Error,
}

impl MainExecutionErrorWithCleanupContext {
    fn new(main: anyhow::Error, cleanup: anyhow::Error) -> Self {
        Self { main, cleanup }
    }
}

impl Display for MainExecutionErrorWithCleanupContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}; browser scope cleanup also failed: {}",
            self.main, self.cleanup
        )
    }
}

impl StdError for MainExecutionErrorWithCleanupContext {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.main.as_ref())
    }
}

pub async fn run_with_browser_scope<F, T>(
    factory: Arc<dyn BrowserDriverFactory>,
    future: F,
) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
{
    let scope = Arc::new(BrowserScope::new(factory));
    let main_result = BROWSER_SCOPE.scope(scope.clone(), future).await;
    let cleanup_result = close_scope(scope).await;

    match (main_result, cleanup_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(main_error), Ok(())) => Err(main_error),
        (Err(main_error), Err(cleanup_error)) => Err(anyhow::Error::new(
            MainExecutionErrorWithCleanupContext::new(main_error, cleanup_error),
        )),
    }
}

pub async fn with_browser_session<F, Fut, T>(operation: F) -> anyhow::Result<T>
where
    F: FnOnce(Arc<BrowserSession>) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let scope = current_scope()?;
    let session = scope.get_or_create_session().await?;
    operation(session).await
}

fn current_scope() -> anyhow::Result<Arc<BrowserScope>> {
    BROWSER_SCOPE
        .try_with(|scope| scope.clone())
        .map_err(|_| anyhow!("browser scope unavailable"))
}

async fn close_scope(scope: Arc<BrowserScope>) -> anyhow::Result<()> {
    scope.close().await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{BrowserDriverFactory, run_with_browser_scope, with_browser_session};
    use crate::tool::browser::driver::{BrowserDriver, ScrollRequest};
    use crate::tool::browser::error::{BrowserToolResult, BrowserToolUnitResult};
    use std::error::Error as StdError;
    use std::fmt::{Display, Formatter};
    use std::sync::Arc;

    #[derive(Default)]
    struct FakeDriverFactory {
        create_count: Arc<AtomicUsize>,
        close_count: Arc<AtomicUsize>,
    }

    impl FakeDriverFactory {
        fn create_count(&self) -> usize {
            self.create_count.load(Ordering::SeqCst)
        }

        fn close_count(&self) -> usize {
            self.close_count.load(Ordering::SeqCst)
        }
    }

    struct FakeDriver {
        close_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl BrowserDriver for FakeDriver {
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

        async fn get_html(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn screenshot(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn close(&mut self) -> BrowserToolUnitResult {
            self.close_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl BrowserDriverFactory for FakeDriverFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            self.create_count.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeDriver {
                close_count: self.close_count.clone(),
            }))
        }
    }

    #[derive(Default)]
    struct FailingCloseDriverFactory {
        close_count: Arc<AtomicUsize>,
    }

    impl FailingCloseDriverFactory {
        fn close_count(&self) -> usize {
            self.close_count.load(Ordering::SeqCst)
        }
    }

    struct FailingCloseDriver {
        close_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl BrowserDriver for FailingCloseDriver {
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

        async fn get_html(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn screenshot(&mut self, _element_id: Option<&str>) -> BrowserToolResult {
            Ok(serde_json::json!({}))
        }

        async fn close(&mut self) -> BrowserToolUnitResult {
            self.close_count.fetch_add(1, Ordering::SeqCst);
            Err(crate::tool::browser::error::BrowserToolError::new(
                "close_failed",
                "cleanup failed",
            ))
        }
    }

    #[async_trait::async_trait]
    impl BrowserDriverFactory for FailingCloseDriverFactory {
        async fn create(&self) -> anyhow::Result<Box<dyn BrowserDriver>> {
            Ok(Box::new(FailingCloseDriver {
                close_count: self.close_count.clone(),
            }))
        }
    }

    #[tokio::test]
    async fn browser_scope_creates_one_session_per_execute() {
        let factory = Arc::new(FakeDriverFactory::default());

        run_with_browser_scope(factory.clone(), async {
            assert_eq!(factory.create_count(), 0);
            with_browser_session(|_| async { Ok(()) }).await.unwrap();
            assert_eq!(factory.create_count(), 1);
            with_browser_session(|_| async { Ok(()) }).await.unwrap();
            assert_eq!(factory.create_count(), 1);
            Ok::<_, anyhow::Error>(())
        })
        .await
        .unwrap();

        assert_eq!(factory.create_count(), 1);
    }

    #[tokio::test]
    async fn browser_scope_closes_session_on_exit() {
        let factory = Arc::new(FakeDriverFactory::default());

        run_with_browser_scope(factory.clone(), async {
            with_browser_session(|_| async { Ok(()) }).await.unwrap();
            Ok::<_, anyhow::Error>(())
        })
        .await
        .unwrap();

        assert_eq!(factory.close_count(), 1);
    }

    #[tokio::test]
    async fn browser_scope_preserves_main_error_when_cleanup_also_fails() {
        #[derive(Debug)]
        struct MainFailure;

        impl Display for MainFailure {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "main failure")
            }
        }

        impl StdError for MainFailure {}

        let factory = Arc::new(FailingCloseDriverFactory::default());

        let result: anyhow::Result<()> = run_with_browser_scope(factory.clone(), async {
            with_browser_session(|_| async { Ok(()) }).await.unwrap();
            Err::<(), anyhow::Error>(MainFailure.into())
        })
        .await;

        assert_eq!(factory.close_count(), 1);
        let err = result.unwrap_err();
        let rendered = err.to_string();
        assert!(
            rendered.starts_with("main failure"),
            "main error must stay primary in display output"
        );
        assert!(
            rendered.contains("cleanup failed"),
            "cleanup failure must be visible in normal display output"
        );

        let mut chain = err.chain();
        chain.next();
        let main_in_chain = chain
            .find_map(|cause| cause.downcast_ref::<MainFailure>())
            .is_some();
        assert!(main_in_chain, "main failure must stay in the error chain");
    }
}
