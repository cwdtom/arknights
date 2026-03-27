use super::timer_service::{get_by_id, get_thread_local_timer_id, init_timer};
use crate::dao::timer_dao::{NewTimerTask, TimerDao};
use crate::test_support;
use chrono::{Local, TimeZone};
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
fn get_thread_local_timer_id_is_none_outside_timer_scope() {
    let _guard = TEST_LOCK.lock().unwrap();
    assert_eq!(get_thread_local_timer_id(), None);
}

#[tokio::test(flavor = "current_thread")]
async fn get_by_id_returns_created_timer_task() {
    let _guard = TEST_LOCK.lock().unwrap();
    let _env_guard = test_support::app_test_guard();
    let dao = TimerDao::new().unwrap();
    let task_id = unique_task_id("get-by-id");
    let task = NewTimerTask {
        id: task_id.clone(),
        prompt: "prompt".to_string(),
        cron_expr: "0 */30 * * * *".to_string(),
        remaining_runs: 2,
        next_trigger_at: local_rfc3339(2026, 3, 26, 1, 30, 0),
    };
    dao.create(&task).await.unwrap();

    let loaded = get_by_id(task_id.clone()).await.unwrap();
    assert!(loaded.is_some(), "expected task {task_id} to exist");
    let loaded = loaded.unwrap();
    assert_eq!(loaded.id, task_id);
    assert_eq!(loaded.prompt, task.prompt);
}

fn unique_task_id(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("timer-service-tests-{label}-{nanos}")
}

fn local_rfc3339(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> String {
    Local
        .with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .unwrap()
        .to_rfc3339()
}
