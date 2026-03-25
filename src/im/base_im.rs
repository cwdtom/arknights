use crate::im::lark;
use std::sync::OnceLock;
use tokio::sync::Mutex;
use tracing::error;

pub static IM: OnceLock<Mutex<Box<dyn Im>>> = OnceLock::new();

#[async_trait::async_trait]
pub trait Im: Send + Sync {
    async fn send(&mut self, content: String) -> anyhow::Result<()>;

    /// Send a question to the user via lark and wait for their reply (5 min timeout).
    async fn ask_user(&mut self, question: String) -> anyhow::Result<String>;

    /// reply by emoji
    async fn reply_emoji(&mut self, message_id: String, emoji: String) -> anyhow::Result<()>;
}

/// 启动时初始化
pub fn init_lark() {
    match IM.set(Mutex::new(Box::new(lark::Lark::new()))) {
        Ok(_) => {}
        Err(_) => {
            error!("init lark client error");
        }
    }
}

pub fn async_send(content: String) {
    tokio::spawn(async move {
        let mut im = IM.get().expect("IM not initialized").lock().await;
        if let Err(e) = im.send(content).await {
            error!("Send failed: {:?}", e);
        }
    });
}

pub fn async_reply_emoji(message_id: String, emoji: String) {
    tokio::spawn(async move {
        let mut im = IM.get().expect("IM not initialized").lock().await;
        if let Err(e) = im.reply_emoji(message_id, emoji).await {
            error!("reply emoji: {:?}", e);
        }
    });
}

pub async fn ask_user(question: String) -> anyhow::Result<String> {
    let mut im = IM.get().expect("IM not initialized").lock().await;
    im.ask_user(question).await
}
