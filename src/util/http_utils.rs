use anyhow::{Context, anyhow};
use reqwest::Client;
use reqwest::header::{ACCEPT_ENCODING, AUTHORIZATION, CONNECTION, CONTENT_ENCODING, CONTENT_TYPE};
use serde::Serialize;
use std::future::Future;
use std::time::Duration;

const RESPONSE_BODY_READ_RETRY_LIMIT: usize = 1;
const HTTP_ERROR_BODY_SNIPPET_LIMIT: usize = 512;

fn build_client() -> anyhow::Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(60))
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .build()
        .context("build reqwest client failed")
}

pub async fn post<T: Serialize + ?Sized>(
    url: &str,
    api_key: &str,
    body: &T,
) -> anyhow::Result<String> {
    let client = build_client()?;
    send_request_with_response_body_retry(url, "send POST request failed", || {
        let mut request = client
            .post(url)
            .header(ACCEPT_ENCODING, "identity")
            .header(CONNECTION, "close")
            .header("Content-Type", "application/json");

        if !api_key.trim().is_empty() {
            request = request.header(AUTHORIZATION, format!("Bearer {}", api_key));
        }

        request.json(body).send()
    })
    .await
}

pub async fn get(url: &str) -> anyhow::Result<String> {
    let client = build_client()?;
    send_request_with_response_body_retry(url, "send GET request failed", || {
        client
            .get(url)
            .header(ACCEPT_ENCODING, "identity")
            .header(CONNECTION, "close")
            .send()
    })
    .await
}

async fn send_request_with_response_body_retry<F, Fut>(
    url: &str,
    send_error_context: &str,
    mut send_request: F,
) -> anyhow::Result<String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = reqwest::Result<reqwest::Response>>,
{
    let mut retries_remaining = RESPONSE_BODY_READ_RETRY_LIMIT;

    loop {
        let response = send_request()
            .await
            .with_context(|| format!("{send_error_context}: {url}"))?;

        match read_response_body(url, response).await {
            Ok(raw) => return Ok(raw),
            Err(ResponseReadError::Retryable(_err)) if retries_remaining > 0 => {
                retries_remaining -= 1;
            }
            Err(ResponseReadError::Retryable(err)) | Err(ResponseReadError::Fatal(err)) => {
                return Err(err);
            }
        }
    }
}

async fn read_response_body(
    url: &str,
    response: reqwest::Response,
) -> Result<String, ResponseReadError> {
    let meta = ResponseMeta::from_response(&response);
    let body = response
        .bytes()
        .await
        .map_err(|err| classify_body_read_error(url, &meta, err))?;
    let raw = String::from_utf8_lossy(&body).into_owned();

    if !meta.status.is_success() {
        return Err(ResponseReadError::Fatal(http_status_error(url, &meta, &raw)));
    }
    Ok(raw)
}

fn header_value(headers: &reqwest::header::HeaderMap, name: reqwest::header::HeaderName) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

fn classify_body_read_error(
    url: &str,
    meta: &ResponseMeta,
    err: reqwest::Error,
) -> ResponseReadError {
    let error = anyhow!(
        "read response body failed: url={url}, status={}, content_type={}, content_encoding={}: {err}",
        meta.status,
        meta.content_type,
        meta.content_encoding
    );

    if err.is_body() || err.is_decode() {
        ResponseReadError::Retryable(error)
    } else {
        ResponseReadError::Fatal(error)
    }
}

fn http_status_error(url: &str, meta: &ResponseMeta, raw: &str) -> anyhow::Error {
    let snippet = raw
        .chars()
        .take(HTTP_ERROR_BODY_SNIPPET_LIMIT)
        .collect::<String>();
    anyhow!(
        "http request failed: url={url}, status={}, content_type={}, content_encoding={}, body={}",
        meta.status,
        meta.content_type,
        meta.content_encoding,
        snippet
    )
}

struct ResponseMeta {
    status: reqwest::StatusCode,
    content_type: String,
    content_encoding: String,
}

impl ResponseMeta {
    fn from_response(response: &reqwest::Response) -> Self {
        let headers = response.headers();
        Self {
            status: response.status(),
            content_type: header_value(headers, CONTENT_TYPE),
            content_encoding: header_value(headers, CONTENT_ENCODING),
        }
    }
}

enum ResponseReadError {
    Retryable(anyhow::Error),
    Fatal(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;
    use http_body::Frame;
    use http_body_util::StreamBody;
    use reqwest::ResponseBuilderExt;
    use std::collections::VecDeque;
    use std::io;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn send_request_retries_once_when_response_body_read_fails() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(Mutex::new(VecDeque::from([
            response_with_body_read_error("http://example.com/retry"),
            successful_response("http://example.com/retry", 200, "ok"),
        ])));

        let result = send_request_with_response_body_retry(
            "http://example.com/retry",
            "send test request failed",
            {
                let attempts = Arc::clone(&attempts);
                let responses = Arc::clone(&responses);
                move || {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    let response = responses.lock().unwrap().pop_front().unwrap();
                    async move { Ok(response) }
                }
            },
        )
        .await;

        assert_eq!(result.unwrap(), "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn send_request_does_not_retry_http_status_errors() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(Mutex::new(VecDeque::from([successful_response(
            "http://example.com/status-error",
            500,
            "boom",
        )])));

        let result = send_request_with_response_body_retry(
            "http://example.com/status-error",
            "send test request failed",
            {
                let attempts = Arc::clone(&attempts);
                let responses = Arc::clone(&responses);
                move || {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    let response = responses.lock().unwrap().pop_front().unwrap();
                    async move { Ok(response) }
                }
            },
        )
        .await;

        let error = result.unwrap_err().to_string();
        assert!(error.contains("status=500"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
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
}
