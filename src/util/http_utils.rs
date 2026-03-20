use anyhow::{Context, anyhow, bail};
use reqwest::Client;
use reqwest::header::{ACCEPT_ENCODING, AUTHORIZATION, CONNECTION, CONTENT_ENCODING, CONTENT_TYPE};
use serde::Serialize;
use std::time::Duration;

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
    let mut request = client
        .post(url)
        .header(ACCEPT_ENCODING, "identity")
        .header(CONNECTION, "close")
        .header("Content-Type", "application/json");

    if !api_key.trim().is_empty() {
        request = request.header(AUTHORIZATION, format!("Bearer {}", api_key));
    }

    let response = request
        .json(body)
        .send()
        .await
        .with_context(|| format!("send POST request failed: {url}"))?;

    read_response_body(url, response).await
}

pub async fn get(url: &str) -> anyhow::Result<String> {
    let client = build_client()?;
    let response = client
        .get(url)
        .header(ACCEPT_ENCODING, "identity")
        .header(CONNECTION, "close")
        .send()
        .await
        .with_context(|| format!("send GET request failed: {url}"))?;

    read_response_body(url, response).await
}

async fn read_response_body(url: &str, response: reqwest::Response) -> anyhow::Result<String> {
    let status = response.status();
    let headers = response.headers().clone();
    let content_type = header_value(&headers, CONTENT_TYPE);
    let content_encoding = header_value(&headers, CONTENT_ENCODING);

    let body = response.bytes().await.map_err(|err| {
        anyhow!(
            "read response body failed: url={url}, status={}, content_type={}, content_encoding={}: {err}",
            status,
            content_type,
            content_encoding
        )
    })?;

    let raw = String::from_utf8_lossy(&body).into_owned();

    if !status.is_success() {
        let snippet = raw.chars().take(512).collect::<String>();
        bail!(
            "http request failed: url={url}, status={}, content_type={}, content_encoding={}, body={}",
            status,
            content_type,
            content_encoding,
            snippet
        );
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
