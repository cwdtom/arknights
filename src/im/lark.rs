use crate::agent;
use open_lark::openlark_client;
use openlark_client::ws_client::{EventDispatcherHandler, LarkWsClient};
use serde::Deserialize;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info};

const BASE_URL: &str = "https://open.feishu.cn";
static LARK_APP_ID: LazyLock<String> =
    LazyLock::new(|| std::env::var("LARK_APP_ID").expect("LARK_APP_ID not set"));
static LARK_APP_SECRET: LazyLock<String> =
    LazyLock::new(|| std::env::var("LARK_APP_SECRET").expect("LARK_APP_SECRET not set"));

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
                return;
            }
        };

        if envelope.header.event_type != "im.message.receive_v1"
            || envelope.event.message.message_type != "text"
        {
            error!("cant process payload");
            return;
        }

        let content_json: TextContent = serde_json::from_str(&envelope.event.message.content)
            .expect("can't deserialize text content");
        let text = content_json.text;
        if text.trim().is_empty() {
            error!("text is empty");
            return;
        }

        info!("received message: {}", text);

        let mut plan = agent::plan::Plan::new(text).await.expect("plan init error");
        plan.execute().await.expect("plan execution error");
    }
}

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
