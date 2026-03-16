use std::collections::HashMap;
use std::sync::LazyLock;
use reqwest::Client;
use serde::Serialize;

static CLIENT: LazyLock<Client> = LazyLock::new(Client::new);

pub fn client() -> &'static Client {
    &CLIENT
}

pub async fn post<T: Serialize + ?Sized>(
    url: &str,
    headers: &HashMap<String, String>,
    body: &T,
) -> anyhow::Result<String> {
    let mut request = CLIENT.post(url);

    for (key, value) in headers {
        request = request.header(key.as_str(), value.as_str());
    }

    let raw = request
        .json(body)
        .send()
        .await?
        .text()
        .await?;

    Ok(raw)
}
