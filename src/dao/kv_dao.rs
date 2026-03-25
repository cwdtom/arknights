use crate::dao::base_dao::BaseDao;
use anyhow::{anyhow, Context};
use chrono::{SecondsFormat, Utc};
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

    pub async fn get(&self, key: &str) -> anyhow::Result<Option<KvEntry>> {
        let key = key.to_owned();

        self.base
            .run_blocking(move |conn| get_with_conn(conn, &key))
            .await
    }

    pub async fn save(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let key = key.to_owned();
        let value = value.to_owned();

        self.base
            .run_blocking(move |conn| save_with_conn(conn, &key, &value))
            .await
    }

    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let key = key.to_owned();

        self.base
            .run_blocking(move |conn| delete_with_conn(conn, &key))
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

fn key_exists_with_conn(conn: &Connection, key: &str) -> anyhow::Result<bool> {
    conn.query_row(
        "select exists(select 1 from kv_store where key = ?1)",
        params![key],
        |row| row.get(0),
    )
    .with_context(|| format!("select kv_store existence failed for key {key}"))
}

fn insert_with_conn(conn: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
    let timestamp = current_timestamp();

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
    .with_context(|| format!("select kv_store entry failed for key {key}"))
}

fn save_with_conn(conn: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
    if key_exists_with_conn(conn, key)? {
        return update_existing_with_conn(conn, key, value);
    }

    insert_with_conn(conn, key, value)
}

fn update_existing_with_conn(conn: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
    let timestamp = current_timestamp();
    let rows = conn
        .execute(
            "update kv_store set value = ?1, updated_at = ?2 where key = ?3",
            params![value, timestamp, key],
        )
        .with_context(|| format!("save kv_store entry failed for key {key}"))?;

    if rows == 0 {
        return Err(anyhow!("kv_store key not found during save: {key}"));
    }

    Ok(())
}

fn delete_with_conn(conn: &Connection, key: &str) -> anyhow::Result<()> {
    if !key_exists_with_conn(conn, key)? {
        return Err(anyhow!("kv_store key not found for delete: {key}"));
    }

    let rows = conn
        .execute("delete from kv_store where key = ?1", params![key])
        .with_context(|| format!("delete kv_store entry failed for key {key}"))?;

    if rows == 0 {
        return Err(anyhow!("kv_store key not found for delete: {key}"));
    }

    Ok(())
}

fn current_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
#[path = "kv_dao_tests.rs"]
mod tests;
