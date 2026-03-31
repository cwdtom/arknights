use super::*;
use crate::dao::schedule::models::CREATE_TABLE_SQL;
use chrono::{DateTime, Local, TimeZone, Utc};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn schedule_dao_create_returns_saved_event() {
    let path = unique_db_path("create");
    let dao = ScheduleDao::with_path(&path).unwrap();
    let event = NewScheduleEvent {
        id: "sch_1".to_string(),
        content: "team meeting".to_string(),
        tag: Some("work".to_string()),
        start_time: local_rfc3339(2026, 4, 1, 14, 0, 0),
        end_time: Some(local_rfc3339(2026, 4, 1, 15, 0, 0)),
    };

    dao.create(&event).await.unwrap();

    let row = dao.get("sch_1").await.unwrap().unwrap();
    assert_eq!(row.id, event.id);
    assert_eq!(row.content, event.content);
    assert_eq!(row.tag, event.tag);
    assert_eq!(row.start_time, event.start_time);
    assert_eq!(row.end_time, event.end_time);
    assert!(!row.created_at.is_empty());
    assert_eq!(row.created_at, row.updated_at);

    cleanup_db(&path);
}

#[tokio::test]
async fn schedule_dao_list_by_range_filters_by_start_time() {
    let path = unique_db_path("range");
    let dao = ScheduleDao::with_path(&path).unwrap();
    dao.create(&build_event(
        "sch_before",
        "before",
        None,
        &local_rfc3339(2026, 3, 31, 8, 0, 0),
        None,
    ))
    .await
    .unwrap();
    dao.create(&build_event(
        "sch_in",
        "in range",
        None,
        &local_rfc3339(2026, 4, 1, 10, 0, 0),
        None,
    ))
    .await
    .unwrap();
    dao.create(&build_event(
        "sch_after",
        "after",
        None,
        &local_rfc3339(2026, 4, 2, 20, 0, 0),
        None,
    ))
    .await
    .unwrap();

    let rows = dao
        .list_by_range(
            &local_rfc3339(2026, 4, 1, 0, 0, 0),
            &local_rfc3339(2026, 4, 1, 23, 59, 59),
        )
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "sch_in");

    cleanup_db(&path);
}

#[tokio::test]
async fn schedule_dao_list_by_range_includes_overlapping_event() {
    let path = unique_db_path("overlap");
    let dao = ScheduleDao::with_path(&path).unwrap();
    dao.create(&build_event(
        "sch_overlap",
        "overnight maintenance",
        None,
        &local_rfc3339(2026, 4, 1, 23, 0, 0),
        Some(&local_rfc3339(2026, 4, 2, 1, 0, 0)),
    ))
    .await
    .unwrap();

    let rows = dao
        .list_by_range(
            &local_rfc3339(2026, 4, 2, 0, 0, 0),
            &local_rfc3339(2026, 4, 2, 23, 59, 59),
        )
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "sch_overlap");

    cleanup_db(&path);
}

#[tokio::test]
async fn schedule_dao_search_matches_content_substring() {
    let path = unique_db_path("search");
    let dao = ScheduleDao::with_path(&path).unwrap();
    dao.create(&build_event(
        "sch_1",
        "team meeting",
        None,
        &local_rfc3339(2026, 4, 1, 10, 0, 0),
        None,
    ))
    .await
    .unwrap();
    dao.create(&build_event(
        "sch_2",
        "lunch break",
        None,
        &local_rfc3339(2026, 4, 1, 12, 0, 0),
        None,
    ))
    .await
    .unwrap();

    let rows = dao.search("meeting").await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "sch_1");

    cleanup_db(&path);
}

#[tokio::test]
async fn schedule_dao_list_by_tag_filters_exact_match() {
    let path = unique_db_path("tag");
    let dao = ScheduleDao::with_path(&path).unwrap();
    dao.create(&build_event(
        "sch_1",
        "standup",
        Some("work"),
        &local_rfc3339(2026, 4, 1, 9, 0, 0),
        None,
    ))
    .await
    .unwrap();
    dao.create(&build_event(
        "sch_2",
        "gym",
        Some("personal"),
        &local_rfc3339(2026, 4, 1, 18, 0, 0),
        None,
    ))
    .await
    .unwrap();
    dao.create(&build_event(
        "sch_3",
        "no tag",
        None,
        &local_rfc3339(2026, 4, 1, 20, 0, 0),
        None,
    ))
    .await
    .unwrap();

    let rows = dao.list_by_tag("work").await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "sch_1");

    cleanup_db(&path);
}

#[tokio::test]
async fn schedule_dao_update_rewrites_mutable_fields() {
    let path = unique_db_path("update");
    let dao = ScheduleDao::with_path(&path).unwrap();
    dao.create(&build_event(
        "sch_1",
        "old",
        None,
        &local_rfc3339(2026, 4, 1, 10, 0, 0),
        None,
    ))
    .await
    .unwrap();
    let before = dao.get("sch_1").await.unwrap().unwrap();
    let before_updated = DateTime::parse_from_rfc3339(&before.updated_at).unwrap();
    while Utc::now().timestamp_millis() <= before_updated.timestamp_millis() {
        sleep(Duration::from_millis(1)).await;
    }

    let updated = UpdateScheduleEvent {
        id: "sch_1".to_string(),
        content: "new content".to_string(),
        tag: Some("tag".to_string()),
        start_time: local_rfc3339(2026, 4, 1, 11, 0, 0),
        end_time: Some(local_rfc3339(2026, 4, 1, 12, 0, 0)),
    };
    dao.update(&updated).await.unwrap();

    let row = dao.get("sch_1").await.unwrap().unwrap();
    let after_updated = DateTime::parse_from_rfc3339(&row.updated_at).unwrap();
    assert_eq!(row.content, "new content");
    assert_eq!(row.tag, Some("tag".to_string()));
    assert_eq!(row.start_time, local_rfc3339(2026, 4, 1, 11, 0, 0));
    assert_eq!(row.end_time, Some(local_rfc3339(2026, 4, 1, 12, 0, 0)));
    assert_eq!(row.created_at, before.created_at);
    assert!(after_updated > before_updated);

    cleanup_db(&path);
}

#[tokio::test]
async fn schedule_dao_remove_deletes_existing_event() {
    let path = unique_db_path("remove");
    let dao = ScheduleDao::with_path(&path).unwrap();
    dao.create(&build_event(
        "sch_1",
        "temp",
        None,
        &local_rfc3339(2026, 4, 1, 10, 0, 0),
        None,
    ))
    .await
    .unwrap();

    dao.remove("sch_1").await.unwrap();

    let row = dao.get("sch_1").await.unwrap();
    assert!(row.is_none());

    cleanup_db(&path);
}

#[tokio::test]
async fn schedule_dao_get_returns_none_for_missing_id() {
    let path = unique_db_path("missing");
    let dao = ScheduleDao::with_path(&path).unwrap();

    let row = dao.get("missing").await.unwrap();

    assert!(row.is_none());

    cleanup_db(&path);
}

#[tokio::test]
async fn schedule_dao_init_preserves_existing_timestamp_values() {
    let path = unique_db_path("preserve-existing");
    seed_raw_event(
        &path,
        "sch_existing",
        "existing row",
        Some("work"),
        "2026-04-01T06:00:00.000Z",
        Some("2026-04-01T07:00:00.000Z"),
        "2026-03-31T06:54:51.644Z",
        "2026-03-31T06:54:51.644Z",
    );

    let dao = ScheduleDao::with_path(&path).unwrap();
    let row = dao.get("sch_existing").await.unwrap().unwrap();

    assert_eq!(row.start_time, "2026-04-01T06:00:00.000Z");
    assert_eq!(row.end_time, Some("2026-04-01T07:00:00.000Z".to_string()));
    assert_eq!(row.created_at, "2026-03-31T06:54:51.644Z");
    assert_eq!(row.updated_at, "2026-03-31T06:54:51.644Z");

    cleanup_db(&path);
}

fn build_event(
    id: &str,
    content: &str,
    tag: Option<&str>,
    start_time: &str,
    end_time: Option<&str>,
) -> NewScheduleEvent {
    NewScheduleEvent {
        id: id.to_string(),
        content: content.to_string(),
        tag: tag.map(String::from),
        start_time: start_time.to_string(),
        end_time: end_time.map(String::from),
    }
}

fn unique_db_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("schedule-dao-{label}-{nanos}.db"))
}

fn cleanup_db(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn seed_raw_event(
    path: &Path,
    id: &str,
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
        (id, content, tag, start_time, end_time, created_at, updated_at),
    )
    .unwrap();
}

fn local_rfc3339(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> String {
    Local
        .with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .unwrap()
        .to_rfc3339()
}
