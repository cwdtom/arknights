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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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

#[tokio::test]
async fn post_multipart_logs_request_and_success_response() {
    let _guard = test_support::app_test_guard().await;
    test_support::init_test_logging();

    let server = spawn_test_server(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
    )
    .await;
    let form = reqwest::multipart::Form::new()
        .text("file_type", "file")
        .text("file_name", "report.md")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"hello".to_vec())
                .file_name("report.md")
                .mime_str("text/markdown; charset=utf-8")
                .unwrap(),
        );
    let request_summary = serde_json::json!({
        "file_type": "file",
        "file_name": "report.md",
        "mime_type": "text/markdown; charset=utf-8",
    });

    let raw = post_multipart(&server, "test-token", form, &request_summary)
        .await
        .unwrap();

    assert_eq!(raw, "{\"ok\":true}");
    let logs = test_support::wait_for_test_logs_contains(&[
        "multipart_request".to_string(),
        "multipart_response".to_string(),
        "has_auth=true".to_string(),
        "\"file_type\":\"file\"".to_string(),
        "\"file_name\":\"report.md\"".to_string(),
        "status=200 OK".to_string(),
        "{\"ok\":true}".to_string(),
    ])
    .await
    .unwrap();
    assert!(logs.contains("multipart_request"));
    assert!(logs.contains("multipart_response"));
}

#[tokio::test]
async fn post_multipart_logs_error_response_before_returning_error() {
    let _guard = test_support::app_test_guard().await;
    test_support::init_test_logging();

    let server = spawn_test_server(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"code\":234001}",
    )
    .await;
    let form = reqwest::multipart::Form::new().text("file_type", "file");
    let request_summary = serde_json::json!({
        "file_type": "file",
    });

    let err = post_multipart(&server, "", form, &request_summary)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("status=400"));
    let logs = test_support::wait_for_test_logs_contains(&[
        "multipart_request".to_string(),
        "multipart_response".to_string(),
        "has_auth=false".to_string(),
        "\"file_type\":\"file\"".to_string(),
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

async fn spawn_test_server(response: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        drain_request(&mut stream).await.unwrap();
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    format!("http://{addr}")
}

async fn drain_request(stream: &mut TcpStream) -> anyhow::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }

        buf.extend_from_slice(&chunk[..read]);
        if let Some(headers_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let body_start = headers_end + 4;
            let headers = String::from_utf8_lossy(&buf[..body_start]);
            let content_length = headers
                .lines()
                .find_map(|line| line.split_once(':'))
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let remaining = content_length.saturating_sub(buf.len() - body_start);

            if remaining > 0 {
                let mut body = vec![0_u8; remaining];
                stream.read_exact(&mut body).await?;
            }

            return Ok(());
        }
    }
}
