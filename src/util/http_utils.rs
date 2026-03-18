use reqwest::Client;
use serde::Serialize;
use std::sync::LazyLock;
use std::time::Duration;

static CLIENT: LazyLock<Client> = LazyLock::new(Client::new);

pub async fn post<T: Serialize + ?Sized>(
    url: &str,
    api_key: &str,
    body: &T,
) -> anyhow::Result<String> {
    let request = CLIENT.post(url);

    let raw = request
        .timeout(Duration::from_secs(60))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await?
        .text()
        .await?;

    Ok(raw)
}
