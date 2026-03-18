use crate::{agent, util};
use chrono::{Utc};
use open_lark::openlark_client;
use openlark_client::ws_client::{EventDispatcherHandler, LarkWsClient};
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, LazyLock};
use std::time::{Duration};
use tokio::sync::mpsc;
use tracing::{error, info};

const BASE_URL: &str = "https://open.feishu.cn";
const SEND_URL: &str = "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type=open_id";
const TOKEN_URL: &str = "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
static LARK_APP_ID: LazyLock<String> =
    LazyLock::new(|| std::env::var("LARK_APP_ID").expect("LARK_APP_ID not set"));
static LARK_APP_SECRET: LazyLock<String> =
    LazyLock::new(|| std::env::var("LARK_APP_SECRET").expect("LARK_APP_SECRET not set"));
static LARK_USER_OPEN_ID: LazyLock<String> =
    LazyLock::new(|| std::env::var("LARK_USER_OPEN_ID").expect("LARK_USER_OPEN_ID not set"));

#[derive(Debug, Deserialize)]
struct EventEnvelope {
    header: EventHeader,
    event: EventBody,
}

#[derive(Debug, Deserialize)]
struct EventHeader {
    event_type: String,
}

#[derive(Debug, Deserialize)]
struct EventBody {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    message_type: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct TextContent {
    text: String,
}

pub async fn build_wss() -> anyhow::Result<()> {
    let ws_config = openlark_client::Config::builder()
        .app_id(LARK_APP_ID.clone())
        .app_secret(LARK_APP_SECRET.clone())
        .base_url(BASE_URL)
        .timeout(Duration::from_secs(60))
        .build()?;

    let (payload_tx, payload_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    tokio::spawn(process_payload_loop(payload_rx));

    let event_handler = EventDispatcherHandler::builder()
        .payload_sender(payload_tx)
        .build();

    LarkWsClient::open(Arc::new(ws_config), event_handler).await?;
    info!("Built lark wss");

    Ok(())
}

async fn process_payload_loop(mut payload_rx: mpsc::UnboundedReceiver<Vec<u8>>) {
    while let Some(payload) = payload_rx.recv().await {
        let envelope: EventEnvelope = match serde_json::from_slice(&payload) {
            Ok(v) => v,
            Err(err) => {
                error!("payload format error: {:?}", err);
                continue;
            }
        };

        if envelope.header.event_type != "im.message.receive_v1"
            || envelope.event.message.message_type != "text"
        {
            error!("cant process payload");
            continue;
        }

        let content_json: TextContent = serde_json::from_str(&envelope.event.message.content)
            .expect("can't deserialize text content");
        let text = content_json.text;
        if text.trim().is_empty() {
            error!("text is empty");
            continue;
        }

        info!("received message: {}", text);

        let mut plan = agent::plan::Plan::new(text).await.expect("plan init error");
        plan.execute().await.expect("plan execution error");
    }
}

static LARK: LazyLock<tokio::sync::Mutex<Lark>> =
    LazyLock::new(|| tokio::sync::Mutex::new(Lark::new()));

struct Lark {
    access_token: String,
    // timestamp
    update_time: i64,
}

#[derive(Deserialize, Debug)]
struct AccessTokenResp {
    tenant_access_token: String,
}

impl Lark {
    fn new() -> Self {
        Lark {
            access_token: "".to_string(),
            update_time: 0,
        }
    }

    async fn get_access_token(&mut self) -> anyhow::Result<String> {
        // 1 hour expire
        if self.update_time + 3600 > Utc::now().timestamp() {
            return Ok(self.access_token.clone());
        }

        let body = json!({
            "app_id": LARK_APP_ID.clone(),
            "app_secret": LARK_APP_SECRET.clone(),
        });
        let raw = util::http_utils::post(TOKEN_URL, "", &body).await?;
        let resp: AccessTokenResp = serde_json::from_str(&raw)?;

        self.access_token = resp.tenant_access_token;
        self.update_time = Utc::now().timestamp();

        Ok(self.access_token.clone())
    }
}

pub async fn send(content: String) -> anyhow::Result<()> {
    // build content
    let message_content = json!({
        "text": content,
    });

    // build message req
    let message_request = json!({
        "receive_id": LARK_USER_OPEN_ID.clone(),
        "content": message_content.to_string(),
        "msg_type": "text"
    });

    info!("Sending request: {:?}", message_request);

    // send
    let raw = util::http_utils::post(
        SEND_URL,
        &LARK.lock().await.get_access_token().await?,
        &message_request,
    )
    .await?;

    info!("Sent response: {}", raw);
    Ok(())
}

pub fn async_send(content: String) {
    tokio::spawn(send(content));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_envelope_deserialization() {
        let json = r#"{
            "header": {
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "message": {
                    "message_type": "text",
                    "content": "{\"text\":\"hello world\"}"
                }
            }
        }"#;
        let envelope: EventEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.header.event_type, "im.message.receive_v1");
        assert_eq!(envelope.event.message.message_type, "text");
        assert_eq!(envelope.event.message.content, r#"{"text":"hello world"}"#);
    }

    #[test]
    fn text_content_deserialization() {
        let json = r#"{"text":"hello world"}"#;
        let content: TextContent = serde_json::from_str(json).unwrap();
        assert_eq!(content.text, "hello world");
    }

    #[test]
    fn event_envelope_nested_text_extraction() {
        let json = r#"{
            "header": {"event_type": "im.message.receive_v1"},
            "event": {
                "message": {
                    "message_type": "text",
                    "content": "{\"text\":\"test message\"}"
                }
            }
        }"#;
        let envelope: EventEnvelope = serde_json::from_str(json).unwrap();
        let text_content: TextContent =
            serde_json::from_str(&envelope.event.message.content).unwrap();
        assert_eq!(text_content.text, "test message");
    }
}
