use crate::{agent, util};
use chrono::Utc;
use open_lark::openlark_client;
use openlark_client::ws_client::{EventDispatcherHandler, LarkWsClient};
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

static PENDING_ASK: LazyLock<tokio::sync::Mutex<Option<oneshot::Sender<String>>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(None));
static PLAN_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

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
    create_time: String,
}

#[derive(Debug, Deserialize)]
struct EventBody {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    message_type: String,
    content: String,
    message_id: String,
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

    Ok(())
}

async fn process_payload_loop(mut payload_rx: mpsc::UnboundedReceiver<Vec<u8>>) {
    let mut last_ts = Utc::now().timestamp();

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
        // replay get
        async_reply_emoji(envelope.event.message.message_id.clone(), "Get".to_string());

        // if there is a pending ask_user, route the reply to it
        let pending = PENDING_ASK.lock().await.take();
        if let Some(tx) = pending {
            let _ = tx.send(text);
            continue;
        }

        // check process last timestamp, ignore outdate message
        let cur_ts: i64 = match envelope.header.create_time.parse() {
            Ok(ts) => ts,
            Err(err) => {
                error!("create_time format error: {:?}", err);
                continue;
            }
        };
        if last_ts > cur_ts {
            continue;
        } else {
            last_ts = cur_ts;
        }

        // start plan in a separate task so the payload loop stays free to receive replies
        // PLAN_LOCK ensures only one plan runs at a time
        tokio::spawn(async move {
            let _guard = PLAN_LOCK.lock().await;
            match agent::plan::Plan::new(text).await {
                Ok(mut plan) => {
                    if let Err(e) = plan.execute().await {
                        error!("plan execution error: {:?}", e);
                    }
                }
                Err(e) => error!("plan init error: {:?}", e),
            }

            // replay done
            async_reply_emoji(envelope.event.message.message_id, "DONE".to_string());
        });
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

/// Send a question to the user via lark and wait for their reply (5 min timeout).
pub async fn ask_user(question: String) -> anyhow::Result<String> {
    let (tx, rx) = oneshot::channel::<String>();
    PENDING_ASK.lock().await.replace(tx);

    send(question).await?;

    match tokio::time::timeout(Duration::from_secs(300), rx).await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(_)) => {
            anyhow::bail!("ask_user channel closed unexpectedly")
        }
        Err(_) => {
            // timeout — clean up the pending sender
            PENDING_ASK.lock().await.take();
            anyhow::bail!("ask_user timed out: no reply within 5 minutes")
        }
    }
}

pub async fn reply_emoji(message_id: String, emoji: String) -> anyhow::Result<()> {
    let base_url = format!(
        "https://open.feishu.cn/open-apis/im/v1/messages/{}/reactions",
        message_id
    );
    let body = json!({
        "reaction_type": {
          "emoji_type": emoji
        }
    });

    info!("Reply emoji request: {:?}", body);

    // send
    let raw = util::http_utils::post(
        base_url.as_str(),
        &LARK.lock().await.get_access_token().await?,
        &body,
    )
    .await?;

    info!("Reply emoji response: {}", raw);
    Ok(())
}

pub fn async_reply_emoji(message_id: String, emoji: String) {
    tokio::spawn(reply_emoji(message_id, emoji));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_envelope_deserialization() {
        let json = r#"{
            "header": {
                "event_type": "im.message.receive_v1",
                "create_time": "1742374800"
            },
            "event": {
                "message": {
                    "message_type": "text",
                    "content": "{\"text\":\"hello world\"}",
                    "message_id": "om_test_message"
                }
            }
        }"#;
        let envelope: EventEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.header.event_type, "im.message.receive_v1");
        assert_eq!(envelope.header.create_time, "1742374800");
        assert_eq!(envelope.event.message.message_type, "text");
        assert_eq!(envelope.event.message.message_id, "om_test_message");
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
            "header": {
                "event_type": "im.message.receive_v1",
                "create_time": "1742374800"
            },
            "event": {
                "message": {
                    "message_type": "text",
                    "content": "{\"text\":\"test message\"}",
                    "message_id": "om_test_message"
                }
            }
        }"#;
        let envelope: EventEnvelope = serde_json::from_str(json).unwrap();
        let text_content: TextContent =
            serde_json::from_str(&envelope.event.message.content).unwrap();
        assert_eq!(text_content.text, "test message");
    }
}
