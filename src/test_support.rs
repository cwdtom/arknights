use crate::dao::kv_dao::KvDao;
use anyhow::anyhow;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};

const PERSONAL_KEY: &str = "PERSONAL";
const USER_PROFILE_KEY: &str = "USER_PROFILE";
const TEST_LARK_APP_ID: &str = "test-app-id";
const TEST_LARK_APP_SECRET: &str = "test-app-secret";
const TEST_LARK_USER_OPEN_ID: &str = "test-open-id";

static TEST_DB_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    std::env::temp_dir().join(format!("arknights-test-{pid}-{nanos}.db"))
});
static TEST_ENV_LOCK: LazyLock<AsyncMutex<()>> = LazyLock::new(|| AsyncMutex::new(()));
static TEST_LOGS: LazyLock<Mutex<Vec<u8>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static TEST_LOG_SUBSCRIBER: OnceLock<()> = OnceLock::new();

pub(crate) fn lock_test_env() -> AsyncMutexGuard<'static, ()> {
    TEST_ENV_LOCK.blocking_lock()
}

pub(crate) async fn lock_test_env_async() -> AsyncMutexGuard<'static, ()> {
    TEST_ENV_LOCK.lock().await
}

pub(crate) async fn app_test_guard() -> AsyncMutexGuard<'static, ()> {
    let guard = lock_test_env_async().await;
    configure_app_test_env();
    guard
}

pub(crate) fn configure_app_test_env() {
    unsafe {
        std::env::set_var("ARKNIGHTS_DB_PATH", test_db_path());
        std::env::set_var("LARK_APP_ID", TEST_LARK_APP_ID);
        std::env::set_var("LARK_APP_SECRET", TEST_LARK_APP_SECRET);
        std::env::set_var("LARK_USER_OPEN_ID", TEST_LARK_USER_OPEN_ID);
        std::env::remove_var("DEEPSEEK_API_KEY");
    }
}

pub(crate) fn disable_rag_for_test() {
    unsafe {
        std::env::remove_var("ARKNIGHTS_RAG_MODEL");
    }
}

pub(crate) fn set_rag_model(model: &str) {
    unsafe {
        std::env::set_var("ARKNIGHTS_RAG_MODEL", model);
    }
}

pub(crate) fn test_db_path() -> &'static Path {
    TEST_DB_PATH.as_path()
}

pub(crate) fn unique_test_token(scope: &str, label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{scope}-{label}-{nanos}")
}

pub(crate) fn assert_timestamped_message(actual: &str, expected_suffix: &str) {
    let (prefix, suffix) = actual
        .split_once("] ")
        .expect("message should contain RFC3339 prefix");
    assert!(prefix.starts_with('['));
    chrono::DateTime::parse_from_rfc3339(&prefix[1..]).unwrap();
    assert_eq!(suffix, expected_suffix);
}

pub(crate) fn init_test_logging() {
    clear_test_logs();
    TEST_LOG_SUBSCRIBER.get_or_init(|| {
        let _ = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(make_test_log_writer)
            .try_init();
    });
}

pub(crate) fn read_test_logs() -> String {
    let logs = TEST_LOGS.lock().unwrap_or_else(|err| err.into_inner());
    String::from_utf8_lossy(&logs).into_owned()
}

pub(crate) async fn wait_for_test_logs_contains(needles: &[String]) -> anyhow::Result<String> {
    let expected = needles.to_vec();

    wait_until_async("test log records", 20, Duration::from_millis(25), || {
        let expected = expected.clone();
        async move {
            let logs = read_test_logs();
            Ok(expected.iter().all(|needle| logs.contains(needle)))
        }
    })
    .await?;

    Ok(read_test_logs())
}

pub(crate) async fn wait_until_async<F, Fut>(
    description: &str,
    attempts: usize,
    delay: Duration,
    mut condition: F,
) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<bool>>,
{
    let attempts = attempts.max(1);

    for attempt in 0..attempts {
        if condition().await? {
            return Ok(());
        }

        if attempt + 1 < attempts {
            tokio::time::sleep(delay).await;
        }
    }

    Err(anyhow!("timed out waiting for {description}"))
}

pub(crate) async fn clear_personal_value() -> anyhow::Result<()> {
    clear_kv_value(PERSONAL_KEY).await
}

pub(crate) async fn clear_user_profile() -> anyhow::Result<()> {
    clear_kv_value(USER_PROFILE_KEY).await
}

async fn clear_kv_value(key: &str) -> anyhow::Result<()> {
    let dao = KvDao::new()?;

    if dao.get(key).await?.is_some() {
        dao.delete(key).await?;
    }

    Ok(())
}

fn clear_test_logs() {
    TEST_LOGS
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clear();
}

fn make_test_log_writer() -> TestLogWriter {
    TestLogWriter
}

struct TestLogWriter;

impl io::Write for TestLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        TEST_LOGS
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    #[test]
    fn unique_test_token_includes_scope_and_label() {
        let token = unique_test_token("scope", "label");
        assert!(token.starts_with("scope-label-"));
    }

    #[test]
    fn assert_timestamped_message_accepts_rfc3339_prefix() {
        assert_timestamped_message("[2026-03-27T12:34:56+08:00] hello", "hello");
    }

    #[tokio::test]
    async fn wait_until_async_retries_until_condition_matches() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&attempts);

        wait_until_async(
            "eventual condition",
            5,
            Duration::from_millis(1),
            move || {
                let observed = Arc::clone(&observed);
                async move {
                    let current = observed.fetch_add(1, Ordering::SeqCst);
                    Ok(current >= 2)
                }
            },
        )
        .await
        .unwrap();

        assert!(attempts.load(Ordering::SeqCst) >= 3);
    }

    #[test]
    fn init_test_logging_captures_logs_in_memory() {
        let _guard = lock_test_env();
        let token = unique_test_token("test-support", "logs");
        init_test_logging();

        tracing::info!("captured-log-{token}");

        let logs = read_test_logs();
        assert!(logs.contains(&token));
    }
}
