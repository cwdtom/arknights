use crate::dao::base_dao::BaseDao;
use anyhow::Context;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::{Path, PathBuf};

const CREATE_TABLE_SQL: &str = r#"
create table if not exists kv_store
(
    key        TEXT primary key,
    value      TEXT not null,
    created_at TEXT not null,
    updated_at TEXT not null
);
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvEntry {
    pub key: String,
    pub value: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct KvDao {
    base: BaseDao,
}

impl KvDao {
    pub fn new() -> anyhow::Result<Self> {
        let base = BaseDao::new()?;
        init_schema(&base)?;
        Ok(Self { base })
    }

    pub fn with_path<P>(db_path: P) -> anyhow::Result<Self>
    where
        P: Into<PathBuf>,
    {
        let base = BaseDao::with_path(db_path)?;
        init_schema(&base)?;
        Ok(Self { base })
    }

    pub async fn create(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let key = key.to_owned();
        let value = value.to_owned();

        self.base
            .run_blocking(move |conn| insert_with_conn(conn, &key, &value))
            .await
    }

    pub async fn get(&self, key: &str) -> anyhow::Result<Option<KvEntry>> {
        let key = key.to_owned();

        self.base
            .run_blocking(move |conn| get_with_conn(conn, &key))
            .await
    }

    fn map_row(row: &Row<'_>) -> rusqlite::Result<KvEntry> {
        Ok(KvEntry {
            key: row.get(0)?,
            value: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
        })
    }

    pub fn db_path(&self) -> &Path {
        self.base.db_path()
    }
}

fn init_schema(base: &BaseDao) -> anyhow::Result<()> {
    base.with_connection(|conn| {
        conn.execute(CREATE_TABLE_SQL, [])
            .context("create kv_store table failed")?;
        Ok(())
    })?;

    Ok(())
}

fn insert_with_conn(conn: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
    let timestamp = Utc::now().to_rfc3339();

    conn.execute(
        "insert into kv_store (key, value, created_at, updated_at)
         values (?1, ?2, ?3, ?4)",
        params![key, value, timestamp, timestamp],
    )
    .with_context(|| format!("insert kv_store entry failed for key {key}"))?;

    Ok(())
}

fn get_with_conn(conn: &Connection, key: &str) -> anyhow::Result<Option<KvEntry>> {
    conn.query_row(
        "select key, value, created_at, updated_at
         from kv_store
         where key = ?1",
        params![key],
        KvDao::map_row,
    )
    .optional()
    .context(format!("select kv_store entry failed for key {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_db_path(prefix: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time went backwards")
            .as_nanos();
        path.push(format!("{prefix}-arknights-kv-{nanos}.db"));
        path
    }

    struct TempDb {
        path: PathBuf,
    }

    impl TempDb {
        fn new(prefix: &str) -> Self {
            Self {
                path: unique_db_path(prefix),
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            for suffix in ["", "-shm", "-wal"] {
                let candidate = format!("{}{}", self.path.display(), suffix);
                let _ = std::fs::remove_file(candidate);
            }
        }
    }

    #[tokio::test]
    async fn create_then_get_returns_entry() -> Result<()> {
        let temp_db = TempDb::new("kv");
        let dao = KvDao::with_path(temp_db.path())?;

        dao.create("app.mode", "prod").await?;

        let entry = dao
            .get("app.mode")
            .await?
            .expect("KV entry not found after create");

        assert_eq!(entry.key, "app.mode");
        assert_eq!(entry.value, "prod");
        assert!(!entry.created_at.is_empty());
        assert!(!entry.updated_at.is_empty());
        assert_eq!(entry.created_at, entry.updated_at);

        Ok(())
    }
}
