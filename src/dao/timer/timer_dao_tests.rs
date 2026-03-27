use super::*;
use chrono::{DateTime, Local, TimeZone, Utc};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn timer_dao_create_returns_saved_task() {
    let path = unique_db_path("create");
    let dao = TimerDao::with_path(&path).unwrap();
    let task = NewTimerTask {
        id: "timer_1".to_string(),
        prompt: "每天早上看一下新闻".to_string(),
        cron_expr: "0 0 9 * * *".to_string(),
        remaining_runs: 3,
        next_trigger_at: local_rfc3339(2026, 3, 26, 9, 0, 0),
    };

    dao.create(&task).await.unwrap();

    let row = dao.get("timer_1").await.unwrap().unwrap();
    assert_eq!(row.id, task.id);
    assert_eq!(row.prompt, task.prompt);
    assert_eq!(row.cron_expr, task.cron_expr);
    assert_eq!(row.remaining_runs, task.remaining_runs);
    assert_eq!(row.next_trigger_at, Some(task.next_trigger_at));
    assert!(row.last_completed_at.is_none());
    assert!(row.last_result.is_none());
    assert!(!row.created_at.is_empty());
    assert_eq!(row.created_at, row.updated_at);

    cleanup_db(&path);
}

#[tokio::test]
async fn timer_dao_list_active_returns_only_positive_remaining_runs() {
    let path = unique_db_path("list_active");
    let dao = TimerDao::with_path(&path).unwrap();
    dao.create(&build_task(
        "timer_active",
        2,
        &local_rfc3339(2026, 3, 26, 9, 0, 0),
    ))
    .await
    .unwrap();
    dao.create(&build_task(
        "timer_inactive",
        0,
        &local_rfc3339(2026, 3, 26, 10, 0, 0),
    ))
    .await
    .unwrap();

    let rows = dao.list_active().await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "timer_active");

    cleanup_db(&path);
}

#[tokio::test]
async fn timer_dao_list_returns_all_tasks_sorted_by_next_trigger_at() {
    let path = unique_db_path("list");
    let dao = TimerDao::with_path(&path).unwrap();
    dao.create(&build_task(
        "timer_later",
        2,
        &local_rfc3339(2026, 3, 26, 10, 0, 0),
    ))
    .await
    .unwrap();
    dao.create(&build_task(
        "timer_earlier",
        1,
        &local_rfc3339(2026, 3, 26, 9, 0, 0),
    ))
    .await
    .unwrap();

    let rows = dao.list().await.unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "timer_earlier");
    assert_eq!(rows[1].id, "timer_later");

    cleanup_db(&path);
}

#[tokio::test]
async fn timer_dao_cancel_sets_remaining_runs_to_zero() {
    let path = unique_db_path("cancel");
    let dao = TimerDao::with_path(&path).unwrap();
    dao.create(&build_task(
        "timer_active",
        2,
        &local_rfc3339(2026, 3, 26, 9, 0, 0),
    ))
    .await
    .unwrap();

    dao.cancel("timer_active").await.unwrap();

    let row = dao.get("timer_active").await.unwrap().unwrap();
    assert_eq!(row.remaining_runs, 0);

    cleanup_db(&path);
}

#[tokio::test]
async fn timer_dao_update_rewrites_mutable_fields() {
    let path = unique_db_path("update");
    let dao = TimerDao::with_path(&path).unwrap();
    dao.create(&build_task(
        "timer_update",
        2,
        &local_rfc3339(2026, 3, 26, 9, 0, 0),
    ))
    .await
    .unwrap();
    let before = dao.get("timer_update").await.unwrap().unwrap();
    let before_updated = DateTime::parse_from_rfc3339(&before.updated_at).unwrap();
    while Utc::now().timestamp_millis() <= before_updated.timestamp_millis() {
        sleep(Duration::from_millis(1)).await;
    }

    let updated = UpdateTimerTask {
        id: "timer_update".to_string(),
        prompt: "updated prompt".to_string(),
        cron_expr: "0 30 9 * * *".to_string(),
        remaining_runs: 5,
        next_trigger_at: local_rfc3339(2026, 3, 26, 9, 30, 0),
    };
    dao.update(&updated).await.unwrap();

    let row = dao.get("timer_update").await.unwrap().unwrap();
    let after_updated = DateTime::parse_from_rfc3339(&row.updated_at).unwrap();
    assert_eq!(row.prompt, updated.prompt);
    assert_eq!(row.cron_expr, updated.cron_expr);
    assert_eq!(row.remaining_runs, updated.remaining_runs);
    assert_eq!(row.next_trigger_at, Some(updated.next_trigger_at));
    assert_eq!(row.last_completed_at, None);
    assert_eq!(row.last_result, None);
    assert_eq!(row.created_at, before.created_at);
    assert!(after_updated > before_updated);

    cleanup_db(&path);
}

#[tokio::test]
async fn timer_dao_get_returns_none_for_missing_id() {
    let path = unique_db_path("missing");
    let dao = TimerDao::with_path(&path).unwrap();

    let row = dao.get("missing").await.unwrap();

    assert!(row.is_none());

    cleanup_db(&path);
}

#[tokio::test]
async fn timer_dao_remove_deletes_existing_task() {
    let path = unique_db_path("remove");
    let dao = TimerDao::with_path(&path).unwrap();
    dao.create(&build_task(
        "timer_remove",
        2,
        &local_rfc3339(2026, 3, 26, 9, 0, 0),
    ))
    .await
    .unwrap();

    dao.remove("timer_remove").await.unwrap();

    let row = dao.get("timer_remove").await.unwrap();
    assert!(row.is_none());

    cleanup_db(&path);
}

#[tokio::test]
async fn timer_dao_list_due_returns_only_ready_active_tasks() {
    let path = unique_db_path("due");
    let dao = TimerDao::with_path(&path).unwrap();
    dao.create(&build_task(
        "timer_due_a",
        2,
        &local_rfc3339(2026, 3, 26, 8, 59, 0),
    ))
    .await
    .unwrap();
    dao.create(&build_task(
        "timer_due_b",
        1,
        &local_rfc3339(2026, 3, 26, 9, 0, 0),
    ))
    .await
    .unwrap();
    dao.create(&build_task(
        "timer_future",
        1,
        &local_rfc3339(2026, 3, 26, 9, 1, 0),
    ))
    .await
    .unwrap();
    dao.create(&build_task(
        "timer_inactive",
        0,
        &local_rfc3339(2026, 3, 26, 8, 58, 0),
    ))
    .await
    .unwrap();

    let now = parse_local("2026-03-26T09:00:00+08:00");
    let rows = dao.list_due(now, 10).await.unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "timer_due_a");
    assert_eq!(rows[1].id, "timer_due_b");

    cleanup_db(&path);
}

#[tokio::test]
async fn timer_dao_mark_run_completed_updates_result_and_remaining_runs() {
    let path = unique_db_path("complete");
    let dao = TimerDao::with_path(&path).unwrap();
    dao.create(&build_task(
        "timer_active",
        3,
        &local_rfc3339(2026, 3, 26, 9, 0, 0),
    ))
    .await
    .unwrap();

    dao.mark_run_completed(
        "timer_active",
        2,
        &local_rfc3339(2026, 3, 27, 9, 0, 0),
        &local_rfc3339(2026, 3, 26, 9, 0, 5),
        "news changed",
    )
    .await
    .unwrap();

    let row = dao.get("timer_active").await.unwrap().unwrap();
    assert_eq!(row.remaining_runs, 2);
    assert_eq!(
        row.next_trigger_at,
        Some(local_rfc3339(2026, 3, 27, 9, 0, 0))
    );
    assert_eq!(
        row.last_completed_at.as_deref(),
        Some(local_rfc3339(2026, 3, 26, 9, 0, 5).as_str())
    );
    assert_eq!(row.last_result.as_deref(), Some("news changed"));
    assert_eq!(row.updated_at, local_rfc3339(2026, 3, 26, 9, 0, 5));

    cleanup_db(&path);
}

fn build_task(id: &str, remaining_runs: u32, next_trigger_at: &str) -> NewTimerTask {
    NewTimerTask {
        id: id.to_string(),
        prompt: format!("prompt for {id}"),
        cron_expr: "0 0 9 * * *".to_string(),
        remaining_runs,
        next_trigger_at: next_trigger_at.to_string(),
    }
}

fn unique_db_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("timer-dao-{label}-{nanos}.db"))
}

fn cleanup_db(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn parse_local(value: &str) -> DateTime<Local> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Local)
}

fn local_rfc3339(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> String {
    Local
        .with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .unwrap()
        .to_rfc3339()
}
