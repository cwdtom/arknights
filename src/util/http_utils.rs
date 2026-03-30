use anyhow::{Context, anyhow};
use reqwest::Client;
use reqwest::header::{ACCEPT_ENCODING, AUTHORIZATION, CONNECTION, CONTENT_ENCODING, CONTENT_TYPE};
use serde::Serialize;
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::time::Duration;
use tracing::info;

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
    send_request_once(url, "send POST request failed", || {
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

pub async fn post_multipart(
    url: &str,
    api_key: &str,
    form: reqwest::multipart::Form,
    request_summary: &Value,
) -> anyhow::Result<String> {
    let client = build_client()?;
    let has_auth = !api_key.trim().is_empty();
    info!(
        url,
        has_auth,
        body = %request_summary,
        "multipart_request"
    );
    let mut request = client
        .post(url)
        .header(ACCEPT_ENCODING, "identity")
        .header(CONNECTION, "close");

    if has_auth {
        request = request.header(AUTHORIZATION, format!("Bearer {}", api_key));
    }

    let response = request
        .multipart(form)
        .send()
        .await
        .with_context(|| format!("send POST request failed: {url}"))?;

    read_response_body_with_log(url, response).await
}

pub async fn get(url: &str) -> anyhow::Result<String> {
    let client = build_client()?;
    send_request_once(url, "send GET request failed", || {
        client
            .get(url)
            .header(ACCEPT_ENCODING, "identity")
            .header(CONNECTION, "close")
            .send()
    })
    .await
}

pub fn mime_type_for_upload(file_type: &str, file_name: &str) -> String {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or(file_type)
        .to_ascii_lowercase();

    match extension.as_str() {
        "csv" => "text/csv; charset=utf-8".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "json" => "application/json".to_string(),
        "md" => "text/markdown; charset=utf-8".to_string(),
        "mp3" => "audio/mpeg".to_string(),
        "mp4" => "video/mp4".to_string(),
        "pdf" => "application/pdf".to_string(),
        "png" => "image/png".to_string(),
        "txt" => "text/plain; charset=utf-8".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

async fn send_request_once<F, Fut>(
    url: &str,
    send_error_context: &str,
    mut send_request: F,
) -> anyhow::Result<String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = reqwest::Result<reqwest::Response>>,
{
    let response = send_request()
        .await
        .with_context(|| format!("{send_error_context}: {url}"))?;
    read_response_body(url, response).await
}

async fn read_response_body(url: &str, response: reqwest::Response) -> anyhow::Result<String> {
    let (meta, raw) = read_response_body_raw(url, response).await?;

    if !meta.status.is_success() {
        return Err(http_status_error(url, &meta, &raw));
    }
    Ok(raw)
}

async fn read_response_body_with_log(
    url: &str,
    response: reqwest::Response,
) -> anyhow::Result<String> {
    let (meta, raw) = read_response_body_raw(url, response).await?;
    info!(
        url,
        status = %meta.status,
        content_type = %meta.content_type,
        content_encoding = %meta.content_encoding,
        body = %body_snippet(&raw),
        "multipart_response"
    );

    if !meta.status.is_success() {
        return Err(http_status_error(url, &meta, &raw));
    }
    Ok(raw)
}

async fn read_response_body_raw(
    url: &str,
    response: reqwest::Response,
) -> anyhow::Result<(ResponseMeta, String)> {
    let meta = ResponseMeta::from_response(&response);
    let body = response
        .bytes()
        .await
        .map_err(|err| body_read_error(url, &meta, err))?;
    let raw = String::from_utf8_lossy(&body).into_owned();

    Ok((meta, raw))
}

fn header_value(headers: &reqwest::header::HeaderMap, name: reqwest::header::HeaderName) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

fn body_read_error(url: &str, meta: &ResponseMeta, err: reqwest::Error) -> anyhow::Error {
    anyhow!(
        "read response body failed: url={url}, status={}, content_type={}, content_encoding={}: {err}",
        meta.status,
        meta.content_type,
        meta.content_encoding
    )
}

fn http_status_error(url: &str, meta: &ResponseMeta, raw: &str) -> anyhow::Error {
    anyhow!(
        "http request failed: url={url}, status={}, content_type={}, content_encoding={}, body={}",
        meta.status,
        meta.content_type,
        meta.content_encoding,
        body_snippet(raw)
    )
}

fn body_snippet(raw: &str) -> String {
    raw.chars()
        .take(HTTP_ERROR_BODY_SNIPPET_LIMIT)
        .collect::<String>()
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

#[cfg(test)]
#[path = "http_utils_tests.rs"]
mod tests;
