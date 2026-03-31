use super::chromiumoxide_runtime::{
    combine_cleanup_results, shutdown_handler_task, take_first_or_stale,
};
use anyhow::anyhow;
use std::time::Duration;

#[test]
fn take_first_or_stale_returns_stale_error_for_empty_lookup() {
    let error = take_first_or_stale::<i32>(vec![]).unwrap_err();

    assert_eq!(error.code, "element_id_stale");
    assert_eq!(error.message, "call browser_snapshot again");
}

#[test]
fn combine_cleanup_results_preserves_primary_error_and_mentions_cleanup_failure() {
    let error = combine_cleanup_results(
        Err(anyhow!("new page failed")),
        Err(anyhow!("handler cleanup failed")),
        "browser launch cleanup",
    )
    .unwrap_err();

    let rendered = error.to_string();
    assert!(rendered.contains("new page failed"));
    assert!(rendered.contains("browser launch cleanup also failed"));
    assert!(rendered.contains("handler cleanup failed"));
}

#[tokio::test]
async fn shutdown_handler_task_cancels_pending_task() {
    let handle = tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok::<(), anyhow::Error>(())
    });

    shutdown_handler_task(handle).await.unwrap();
}
