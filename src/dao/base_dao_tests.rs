use super::*;
use std::fs;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[tokio::test]
async fn new_reads_env_path_and_creates_db_file() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path = unique_db_path_with_parent("env");
    let original = std::env::var(DB_PATH_ENV_VAR).ok();

    unsafe {
        std::env::set_var(DB_PATH_ENV_VAR, &path);
    }

    let dao = BaseDao::new().unwrap();
    assert_eq!(dao.db_path(), path.as_path());
    assert!(path.exists());

    dao.with_connection(|conn| {
        conn.execute(
            "create table if not exists test (id integer primary key)",
            [],
        )
        .context("create test table failed")?;
        Ok(())
    })
    .unwrap();

    restore_db_path_env(original);
    cleanup_db(&path);
    cleanup_parent_dir(&path);
}

#[tokio::test]
async fn in_memory_database_reuses_same_connection() {
    let dao = BaseDao::with_path(":memory:").unwrap();

    dao.run_blocking(|conn| {
        conn.execute("create table test (value text not null)", [])
            .context("create test table failed")?;
        conn.execute("insert into test (value) values ('hello')", [])
            .context("insert test row failed")?;
        Ok(())
    })
    .await
    .unwrap();

    let value = dao
        .run_blocking(|conn| {
            let value: String = conn.query_row("select value from test limit 1", [], |row| {
                row.get::<_, String>(0)
            })?;
            Ok(value)
        })
        .await
        .unwrap();
    assert_eq!(value, "hello");
}

#[test]
fn open_connection_registers_sqlite_vec_functions() {
    let dao = BaseDao::with_path(":memory:").unwrap();
    let vec_version = dao
        .with_connection(|conn| {
            let version: String = conn.query_row("select vec_version()", [], |row| row.get(0))?;
            Ok(version)
        })
        .unwrap();
    assert!(!vec_version.is_empty());
}

fn unique_db_path_with_parent(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    std::env::temp_dir()
        .join(format!("arknights_{prefix}_{nanos}"))
        .join("dao")
        .join("base.db")
}

fn cleanup_db(path: &Path) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(format!("{}-shm", path.to_string_lossy()));
    let _ = fs::remove_file(format!("{}-wal", path.to_string_lossy()));
}

fn cleanup_parent_dir(path: &Path) {
    if let Some(parent) = path.parent().and_then(Path::parent) {
        let _ = fs::remove_dir_all(parent);
    }
}

fn restore_db_path_env(original: Option<String>) {
    match original {
        Some(value) => unsafe {
            std::env::set_var(DB_PATH_ENV_VAR, value);
        },
        None => unsafe {
            std::env::remove_var(DB_PATH_ENV_VAR);
        },
    }
}

#[test]
fn compact_sql_for_log_removes_extra_whitespace() {
    let sql = "select id,\n       user_content\n  from chat_history\n where id = 1";
    assert_eq!(
        compact_sql_for_log(sql),
        "select id, user_content from chat_history where id = 1"
    );
}
