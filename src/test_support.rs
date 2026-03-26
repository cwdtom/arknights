use crate::dao::kv_dao::KvDao;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};

const PERSONAL_KEY: &str = "PERSONAL";
const USER_PROFILE_KEY: &str = "USER_PROFILE";
const TEST_LARK_APP_ID: &str = "test-app-id";
const TEST_LARK_APP_SECRET: &str = "test-app-secret";
const TEST_LARK_USER_OPEN_ID: &str = "test-open-id";

static TEST_DB_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    std::env::temp_dir().join(format!("arknights-test-{pid}-{nanos}.db"))
});
static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(crate) fn lock_test_env() -> MutexGuard<'static, ()> {
    TEST_ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner())
}

pub(crate) fn app_test_guard() -> MutexGuard<'static, ()> {
    let guard = lock_test_env();
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
