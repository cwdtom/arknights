use anyhow::{Context, anyhow};
use rusqlite::Connection;
use rusqlite::ffi::{SQLITE_OK, sqlite3, sqlite3_api_routines, sqlite3_auto_extension};
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
type SqliteExtensionEntryPoint =
    unsafe extern "C" fn(*mut sqlite3, *mut *mut std::ffi::c_char, *const sqlite3_api_routines)
        -> std::ffi::c_int;

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
            let entry_point = unsafe {
                std::mem::transmute::<*const (), SqliteExtensionEntryPoint>(
                    sqlite3_vec_init as *const (),
                )
            };
            let rc = unsafe { sqlite3_auto_extension(Some(entry_point)) };

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
#[path = "base_dao_tests.rs"]
mod tests;
