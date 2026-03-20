use anyhow::{Context, anyhow};
use rusqlite::Connection;
use rusqlite::ffi::{SQLITE_OK, sqlite3_auto_extension};
use rusqlite::trace::{TraceEvent, TraceEventCodes};
use sqlite_vec::sqlite3_vec_init;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::task;
use tracing::info;

const DB_PATH_ENV_VAR: &str = "ARKNIGHTS_DB_PATH";
const DEFAULT_DB_PATH: &str = "arknights.db";
static SQLITE_VEC_REGISTERED: OnceLock<anyhow::Result<()>> = OnceLock::new();

#[derive(Clone)]
pub struct BaseDao {
    db_path: PathBuf,
    conn: Arc<Mutex<Connection>>,
}

impl fmt::Debug for BaseDao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BaseDao")
            .field("db_path", &self.db_path)
            .finish()
    }
}

impl BaseDao {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_path(resolve_db_path())
    }

    pub fn with_path<P>(db_path: P) -> anyhow::Result<Self>
    where
        P: Into<PathBuf>,
    {
        let db_path = db_path.into();
        prepare_db_path(&db_path)?;
        let conn = open_connection(&db_path)?;

        Ok(Self {
            db_path,
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn with_connection<T, F>(&self, op: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("dao connection mutex poisoned"))?;

        op(&conn)
    }

    pub async fn run_blocking<T, F>(&self, op: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);

        task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| anyhow!("dao connection mutex poisoned"))?;

            op(&conn)
        })
        .await
        .context("dao blocking task failed")?
    }
}

fn resolve_db_path() -> PathBuf {
    std::env::var(DB_PATH_ENV_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_DB_PATH))
}

fn prepare_db_path(db_path: &Path) -> anyhow::Result<()> {
    if is_special_db_path(db_path) {
        return Ok(());
    }

    if let Some(parent) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "create sqlite db parent directory failed: {}",
                parent.to_string_lossy()
            )
        })?;
    }

    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(db_path)
        .with_context(|| {
            format!(
                "create sqlite db file failed: {}",
                db_path.to_string_lossy()
            )
        })?;

    Ok(())
}

fn is_special_db_path(db_path: &Path) -> bool {
    let db_path = db_path.to_string_lossy();
    db_path == ":memory:" || db_path.starts_with("file:")
}

fn open_connection(db_path: &Path) -> anyhow::Result<Connection> {
    ensure_sqlite_vec_registered()?;
    let conn = Connection::open(db_path)
        .with_context(|| format!("open sqlite db failed: {}", db_path.to_string_lossy()))?;
    conn.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(log_sql_trace));
    Ok(conn)
}

fn ensure_sqlite_vec_registered() -> anyhow::Result<()> {
    SQLITE_VEC_REGISTERED
        .get_or_init(|| {
            let rc = unsafe {
                sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())))
            };

            if rc == SQLITE_OK {
                Ok(())
            } else {
                Err(anyhow!(
                    "register sqlite-vec extension failed: sqlite rc={rc}"
                ))
            }
        })
        .as_ref()
        .map(|_| ())
        .map_err(|err| anyhow!("{err:#}"))
}

fn log_sql_trace(event: TraceEvent<'_>) {
    if let TraceEvent::Stmt(stmt, sql) = event {
        let expanded_sql = stmt.expanded_sql().unwrap_or_else(|| sql.to_string());
        info!("sqlite execute: {}", compact_sql_for_log(&expanded_sql));
    }
}

fn compact_sql_for_log(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
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
                let value: String =
                    conn.query_row("select value from test limit 1", [], |row| {
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
                let version: String =
                    conn.query_row("select vec_version()", [], |row| row.get(0))?;
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
}
