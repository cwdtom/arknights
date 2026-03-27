use super::*;
use crate::dao::timer_dao::{NewTimerTask, TimerDao};
use crate::test_support;
use chrono::{Local, TimeZone};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn make_notify_choice_returns_true_when_previous_result_is_missing() {
    let _guard = test_support::app_test_guard();
    let dao = TimerDao::new().unwrap();
    let task_id = unique_task_id("notify-first-run");
    let task = NewTimerTask {
        id: task_id.clone(),
        prompt: "每天检查一次新闻".to_string(),
        cron_expr: "0 0 9 * * *".to_string(),
        remaining_runs: 2,
        next_trigger_at: local_rfc3339(2026, 3, 26, 9, 0, 0),
    };
    dao.create(&task).await.unwrap();

    let should_notify = make_notify_choice("本次提醒内容".to_string(), task_id)
        .await
        .unwrap();

    assert!(should_notify);
}

fn unique_task_id(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("notify-check-{label}-{nanos}")
}

fn local_rfc3339(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> String {
    Local
        .with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .unwrap()
        .to_rfc3339()
}
