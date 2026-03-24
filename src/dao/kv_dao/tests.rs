use super::*;
use anyhow::Result;
use chrono::DateTime as ChronoDateTime;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};

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

#[tokio::test]
async fn create_fails_when_key_already_exists() -> Result<()> {
    let temp_db = TempDb::new("kv-duplicate");
    let dao = KvDao::with_path(temp_db.path())?;

    dao.create("app.mode", "prod").await?;

    let err = dao.create("app.mode", "dev").await.unwrap_err();
    let err_msg = err.to_string();

    assert!(err_msg.contains("kv_store key already exists: app.mode"));

    Ok(())
}

#[tokio::test]
async fn update_changes_value_and_updated_at() -> Result<()> {
    let temp_db = TempDb::new("kv-update");
    let dao = KvDao::with_path(temp_db.path())?;

    dao.create("app.mode", "prod").await?;

    let before = dao
        .get("app.mode")
        .await?
        .expect("KV entry should exist before update");

    sleep(Duration::from_millis(10)).await;

    dao.update("app.mode", "dev").await?;

    let after = dao
        .get("app.mode")
        .await?
        .expect("KV entry should exist after update");

    let before_updated = ChronoDateTime::parse_from_rfc3339(&before.updated_at)?;
    let after_updated = ChronoDateTime::parse_from_rfc3339(&after.updated_at)?;

    assert_eq!(after.value, "dev");
    assert_eq!(after.created_at, before.created_at);
    assert!(after_updated > before_updated);

    Ok(())
}

#[tokio::test]
async fn update_fails_when_key_missing() -> Result<()> {
    let temp_db = TempDb::new("kv-update-missing");
    let dao = KvDao::with_path(temp_db.path())?;

    let err = dao.update("missing.key", "dev").await.unwrap_err();
    let err_msg = err.to_string();

    assert!(err_msg.contains("kv_store key not found for update: missing.key"));

    Ok(())
}

#[tokio::test]
async fn delete_removes_entry() -> Result<()> {
    let temp_db = TempDb::new("kv-delete");
    let dao = KvDao::with_path(temp_db.path())?;

    dao.create("app.mode", "prod").await?;

    dao.delete("app.mode").await?;

    let entry = dao.get("app.mode").await?;

    assert!(entry.is_none());

    Ok(())
}

#[tokio::test]
async fn delete_fails_when_key_missing() -> Result<()> {
    let temp_db = TempDb::new("kv-delete-missing");
    let dao = KvDao::with_path(temp_db.path())?;

    let err = dao.delete("missing.key").await.unwrap_err();
    let err_msg = err.to_string();

    assert!(err_msg.contains("kv_store key not found for delete: missing.key"));

    Ok(())
}

#[tokio::test]
async fn in_memory_database_reuses_same_connection() -> Result<()> {
    let dao = KvDao::with_path(":memory:")?;

    dao.create("app.mode", "prod").await?;

    let entry = dao
        .get("app.mode")
        .await?
        .expect("KV entry should be present for in-memory DB");

    assert_eq!(entry.key, "app.mode");
    assert_eq!(entry.value, "prod");

    Ok(())
}
