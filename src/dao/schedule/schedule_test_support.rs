use super::NewScheduleEvent;
use crate::dao::schedule::models::CREATE_TABLE_SQL;
use chrono::{Local, TimeZone};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn build_event(
    content: &str,
    tag: Option<&str>,
    start_time: &str,
    end_time: Option<&str>,
) -> NewScheduleEvent {
    NewScheduleEvent {
        content: content.to_string(),
        tag: tag.map(String::from),
        start_time: start_time.to_string(),
        end_time: end_time.map(String::from),
    }
}

pub(super) fn unique_db_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("schedule-dao-{label}-{nanos}.db"))
}

pub(super) fn cleanup_db(path: &Path) {
    let _ = std::fs::remove_file(path);
}

pub(super) fn seed_raw_event(
    path: &Path,
    id: i64,
    content: &str,
    tag: Option<&str>,
    start_time: &str,
    end_time: Option<&str>,
    created_at: &str,
    updated_at: &str,
) {
    let conn = Connection::open(path).unwrap();
    conn.execute(CREATE_TABLE_SQL, []).unwrap();
    conn.execute(
        "insert into schedule_events
         (id, content, tag, start_time, end_time, created_at, updated_at)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (
            id, content, tag, start_time, end_time, created_at, updated_at,
        ),
    )
    .unwrap();
}

pub(super) fn local_rfc3339(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> String {
    Local
        .with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .unwrap()
        .to_rfc3339()
}
