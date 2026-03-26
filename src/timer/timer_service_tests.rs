use super::timer_service::{init_timer, execute_tasks};
use crate::dao::timer_dao::{NewTimerTask, TimerDao};
use chrono::{DateTime, Local, TimeZone};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[tokio::test(flavor = "current_thread")]
async fn init_timer_starts_background_task_without_blocking_caller() {
    let _guard = TEST_LOCK.lock().unwrap();

    let start = Instant::now();
    init_timer();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(200),
        "init_timer blocked for {:?}",
        elapsed
    );
}

#[test]
fn timer_service_source_does_not_contain_test_hooks() {
    let source = std::fs::read_to_string(timer_service_source_path()).unwrap();
    assert!(
        !source.contains("tests::"),
        "timer_service.rs still references tests module"
    );
    assert!(
        !source.contains("#[cfg(test)]"),
        "timer_service.rs still contains cfg(test) code"
    );
}

#[test]
fn timer_service_source_uses_current_implementation_shape() {
    let source = std::fs::read_to_string(timer_service_source_path()).unwrap();
    assert!(
        source.contains("tokio::spawn"),
        "timer_service.rs should use tokio::spawn in current implementation"
    );
    assert!(
        source.contains("Schedule::from_str"),
        "timer_service.rs should use cron crate parser"
    );
    assert!(
        !source.contains("split_whitespace"),
        "timer_service.rs still contains manual cron parsing"
    );
    assert!(
        !source.contains("parse_time_part"),
        "timer_service.rs still contains manual cron parsing helper"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn process_due_tasks_once_returns_ok_for_empty_db() {
    let _guard = TEST_LOCK.lock().unwrap();
    let path = unique_db_path("empty");
    let dao = TimerDao::with_path(&path).unwrap();

    let now = parse_local(&local_rfc3339(2026, 3, 26, 1, 0, 0));
    execute_tasks(&dao, now).await.unwrap();

    cleanup_db(&path);
}

#[tokio::test(flavor = "current_thread")]
async fn process_due_tasks_once_ignores_future_tasks() {
    let _guard = TEST_LOCK.lock().unwrap();
    let path = unique_db_path("future");
    let dao = TimerDao::with_path(&path).unwrap();
    let task = NewTimerTask {
        id: "timer_future".to_string(),
        prompt: "prompt for timer_future".to_string(),
        cron_expr: "0 */30 * * * *".to_string(),
        remaining_runs: 2,
        next_trigger_at: local_rfc3339(2026, 3, 26, 1, 30, 0),
    };
    dao.create(&task).await.unwrap();

    let now = parse_local(&local_rfc3339(2026, 3, 26, 1, 0, 0));
    execute_tasks(&dao, now).await.unwrap();

    let row = dao.get("timer_future").await.unwrap().unwrap();
    assert_eq!(row.remaining_runs, task.remaining_runs);
    assert_eq!(row.next_trigger_at, Some(task.next_trigger_at));
    assert!(row.last_completed_at.is_none());
    assert!(row.last_result.is_none());

    cleanup_db(&path);
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

fn unique_db_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("timer-service-{label}-{nanos}.db"))
}

fn cleanup_db(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn timer_service_source_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/timer/timer_service.rs")
}
