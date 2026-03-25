use super::*;
use anyhow::Result;
use chrono::{DateTime as ChronoDateTime, FixedOffset, Utc};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{Duration, Instant, sleep};

const TIMESTAMP_POLL_INTERVAL: Duration = Duration::from_millis(1);
const TIMESTAMP_POLL_TIMEOUT: Duration = Duration::from_secs(1);

async fn wait_for_timestamp_tick(previous: &ChronoDateTime<FixedOffset>) {
    let previous_millis = previous.timestamp_millis();
    let deadline = Instant::now() + TIMESTAMP_POLL_TIMEOUT;
    while Utc::now().timestamp_millis() <= previous_millis && Instant::now() < deadline {
        sleep(TIMESTAMP_POLL_INTERVAL).await;
    }
}

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
async fn save_then_get_returns_entry() -> Result<()> {
    let temp_db = TempDb::new("kv");
    let dao = KvDao::with_path(temp_db.path())?;

    dao.save("app.mode", "prod").await?;

    let entry = dao
        .get("app.mode")
        .await?
        .expect("KV entry not found after save");

    assert_eq!(entry.key, "app.mode");
    assert_eq!(entry.value, "prod");
    assert!(!entry.created_at.is_empty());
    assert!(!entry.updated_at.is_empty());
    assert_eq!(entry.created_at, entry.updated_at);

    Ok(())
}

#[tokio::test]
async fn save_updates_existing_entry() -> Result<()> {
    let temp_db = TempDb::new("kv-save-update");
    let dao = KvDao::with_path(temp_db.path())?;

    dao.save("app.mode", "prod").await?;

    let before = dao
        .get("app.mode")
        .await?
        .expect("KV entry should exist before second save");

    let before_updated = ChronoDateTime::parse_from_rfc3339(&before.updated_at)?;
    wait_for_timestamp_tick(&before_updated).await;

    dao.save("app.mode", "dev").await?;

    let after = dao
        .get("app.mode")
        .await?
        .expect("KV entry should exist after second save");

    let after_updated = ChronoDateTime::parse_from_rfc3339(&after.updated_at)?;

    assert_eq!(after.value, "dev");
    assert_eq!(after.created_at, before.created_at);
    assert!(after_updated > before_updated);

    Ok(())
}

#[tokio::test]
async fn delete_removes_entry() -> Result<()> {
    let temp_db = TempDb::new("kv-delete");
    let dao = KvDao::with_path(temp_db.path())?;

    dao.save("app.mode", "prod").await?;

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

    dao.save("app.mode", "prod").await?;

    let entry = dao
        .get("app.mode")
        .await?
        .expect("KV entry should be present for in-memory DB");

    assert_eq!(entry.key, "app.mode");
    assert_eq!(entry.value, "prod");

    Ok(())
}
