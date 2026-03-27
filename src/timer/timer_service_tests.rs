use super::timer_service::{
    CreateTimerTask, UpdateTimerTask, create, get_by_id, get_thread_local_timer_id, init_timer,
    list, remove, update,
};
use crate::dao::timer_dao::{NewTimerTask, TimerDao};
use crate::test_support;
use chrono::{Local, TimeZone};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[tokio::test(flavor = "current_thread")]
async fn init_timer_starts_background_task_without_blocking_caller() {
    let _guard = test_support::app_test_guard().await;

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
    let _guard = test_support::lock_test_env();
    assert_eq!(get_thread_local_timer_id(), None);
}

#[tokio::test(flavor = "current_thread")]
async fn get_by_id_returns_created_timer_task() {
    let _env_guard = test_support::app_test_guard().await;
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

#[tokio::test(flavor = "current_thread")]
async fn create_persists_timer_task_with_computed_next_trigger_at() {
    let _env_guard = test_support::app_test_guard().await;
    let input = CreateTimerTask {
        id: unique_task_id("create"),
        prompt: "每天早上提醒喝水".to_string(),
        cron_expr: "0 0 9 * * *".to_string(),
        remaining_runs: 3,
    };

    let task = create(input.clone()).await.unwrap();

    assert_eq!(task.id, input.id);
    assert_eq!(task.prompt, input.prompt);
    assert_eq!(task.cron_expr, input.cron_expr);
    assert_eq!(task.remaining_runs, input.remaining_runs);
    assert!(task.next_trigger_at.is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn list_returns_created_tasks() {
    let _env_guard = test_support::app_test_guard().await;
    let first = CreateTimerTask {
        id: unique_task_id("list-a"),
        prompt: "task a".to_string(),
        cron_expr: "0 0 9 * * *".to_string(),
        remaining_runs: 1,
    };
    let second = CreateTimerTask {
        id: unique_task_id("list-b"),
        prompt: "task b".to_string(),
        cron_expr: "0 0 10 * * *".to_string(),
        remaining_runs: 2,
    };
    create(first.clone()).await.unwrap();
    create(second.clone()).await.unwrap();

    let tasks = list().await.unwrap();
    let ids: Vec<_> = tasks.into_iter().map(|task| task.id).collect();

    assert!(ids.contains(&first.id));
    assert!(ids.contains(&second.id));
}

#[tokio::test(flavor = "current_thread")]
async fn update_rewrites_timer_task_fields() {
    let _env_guard = test_support::app_test_guard().await;
    let id = unique_task_id("update");
    create(CreateTimerTask {
        id: id.clone(),
        prompt: "old prompt".to_string(),
        cron_expr: "0 0 9 * * *".to_string(),
        remaining_runs: 1,
    })
    .await
    .unwrap();

    let updated = update(UpdateTimerTask {
        id: id.clone(),
        prompt: "new prompt".to_string(),
        cron_expr: "0 30 9 * * *".to_string(),
        remaining_runs: 4,
    })
    .await
    .unwrap();

    assert_eq!(updated.id, id);
    assert_eq!(updated.prompt, "new prompt");
    assert_eq!(updated.cron_expr, "0 30 9 * * *");
    assert_eq!(updated.remaining_runs, 4);
    assert!(updated.next_trigger_at.is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn update_allows_zero_remaining_runs_to_pause_task() {
    let _env_guard = test_support::app_test_guard().await;
    let id = unique_task_id("pause");
    create(CreateTimerTask {
        id: id.clone(),
        prompt: "old prompt".to_string(),
        cron_expr: "0 0 9 * * *".to_string(),
        remaining_runs: 1,
    })
    .await
    .unwrap();

    let updated = update(UpdateTimerTask {
        id: id.clone(),
        prompt: "paused".to_string(),
        cron_expr: "0 30 9 * * *".to_string(),
        remaining_runs: 0,
    })
    .await
    .unwrap();

    assert_eq!(updated.id, id);
    assert_eq!(updated.remaining_runs, 0);
    assert_eq!(updated.prompt, "paused");
}

#[tokio::test(flavor = "current_thread")]
async fn remove_deletes_timer_task() {
    let _env_guard = test_support::app_test_guard().await;
    let id = unique_task_id("remove");
    create(CreateTimerTask {
        id: id.clone(),
        prompt: "task".to_string(),
        cron_expr: "0 0 9 * * *".to_string(),
        remaining_runs: 1,
    })
    .await
    .unwrap();

    remove(id.clone()).await.unwrap();

    assert!(get_by_id(id).await.unwrap().is_none());
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
