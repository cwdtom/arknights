use super::*;
use crate::test_support;
use bytes::Bytes;
use futures::stream;
use http_body::Frame;
use http_body_util::StreamBody;
use reqwest::ResponseBuilderExt;
use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn send_request_does_not_retry_when_response_body_read_fails() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let responses = Arc::new(Mutex::new(VecDeque::from([
        response_with_body_read_error("http://example.com/retry"),
        successful_response("http://example.com/retry", 200, "ok"),
    ])));

    let result = send_test_request_once("http://example.com/retry", {
        let attempts = Arc::clone(&attempts);
        let responses = Arc::clone(&responses);
        move || {
            attempts.fetch_add(1, Ordering::SeqCst);
            let response = responses.lock().unwrap().pop_front().unwrap();
            async move { Ok(response) }
        }
    })
    .await;

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("read response body failed")
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn send_request_does_not_retry_http_status_errors() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let responses = Arc::new(Mutex::new(VecDeque::from([successful_response(
        "http://example.com/status-error",
        500,
        "boom",
    )])));

    let result = send_test_request_once("http://example.com/status-error", {
        let attempts = Arc::clone(&attempts);
        let responses = Arc::clone(&responses);
        move || {
            attempts.fetch_add(1, Ordering::SeqCst);
            let response = responses.lock().unwrap().pop_front().unwrap();
            async move { Ok(response) }
        }
    })
    .await;

    let error = result.unwrap_err().to_string();
    assert!(error.contains("status=500"));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn log_multipart_request_writes_summary_to_logs() {
    let _guard = test_support::lock_test_env();
    test_support::init_test_logging();

    let request_summary = serde_json::json!({
        "file_type": "file",
        "file_name": "report.md",
        "mime_type": "text/markdown; charset=utf-8",
    });

    log_multipart_request("http://example.com/upload", true, &request_summary);

    let logs = test_support::read_test_logs();
    assert!(logs.contains("multipart_request"));
    assert!(logs.contains("http://example.com/upload"));
    assert!(logs.contains("has_auth=true"));
    assert!(logs.contains("\"file_type\":\"file\""));
    assert!(logs.contains("\"file_name\":\"report.md\""));
}

#[tokio::test]
async fn read_response_body_with_log_logs_success_response() {
    let _guard = test_support::app_test_guard().await;
    test_support::init_test_logging();

    let raw = read_response_body_with_log(
        "http://example.com/upload",
        successful_response("http://example.com/upload", 200, "{\"ok\":true}"),
    )
    .await
    .unwrap();

    assert_eq!(raw, "{\"ok\":true}");
    let logs = test_support::wait_for_test_logs_contains(&[
        "multipart_response".to_string(),
        "status=200 OK".to_string(),
        "{\"ok\":true}".to_string(),
    ])
    .await
    .unwrap();
    assert!(logs.contains("multipart_response"));
}

#[tokio::test]
async fn read_response_body_with_log_logs_error_response_before_returning_error() {
    let _guard = test_support::app_test_guard().await;
    test_support::init_test_logging();

    let err = read_response_body_with_log(
        "http://example.com/upload",
        successful_response("http://example.com/upload", 400, "{\"code\":234001}"),
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("status=400"));
    let logs = test_support::wait_for_test_logs_contains(&[
        "multipart_response".to_string(),
        "status=400 Bad Request".to_string(),
        "{\"code\":234001}".to_string(),
    ])
    .await
    .unwrap();
    assert!(logs.contains("multipart_response"));
}

async fn send_test_request_once<F, Fut>(url: &str, send_request: F) -> anyhow::Result<String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = reqwest::Result<reqwest::Response>>,
{
    send_request_once(url, "send test request failed", send_request).await
}

fn successful_response(url: &str, status: u16, body: &str) -> reqwest::Response {
    http::Response::builder()
        .status(status)
        .url(url.parse().unwrap())
        .body(reqwest::Body::from(body.to_string()))
        .unwrap()
        .into()
}

fn response_with_body_read_error(url: &str) -> reqwest::Response {
    let stream = stream::iter([
        Ok(Frame::data(Bytes::from_static(b"partial"))),
        Err(io::Error::other("stream read failed")),
    ]);
    let body = reqwest::Body::wrap(StreamBody::new(stream));

    http::Response::builder()
        .status(200)
        .url(url.parse().unwrap())
        .body(body)
        .unwrap()
        .into()
}
