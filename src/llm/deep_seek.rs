use std::sync::LazyLock;
use tracing::info;
pub(crate) use crate::llm::base_llm::{ChatResponse, Llm, Message, ResponseFormat, Tool};
use crate::util;

const BASE_URL: &str = "https://api.deepseek.com/chat/completions";
static API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY not set"));

impl Llm {
    pub fn deep_seek_new(messages: Vec<Message>, tools: Vec<Tool>) -> Self {
        Llm {
            model: "deepseek-chat".to_string(),
            messages,
            stream: false,
            max_tokens: 8192,
            response_format: ResponseFormat {
                r#type: "json_object".to_string(),
            },
            temperature: 1.0,
            tools,
            tool_choice: "auto".to_string(),
        }
    }

    pub async fn deep_seek_call(&mut self) -> anyhow::Result<ChatResponse> {
        info!("deepseek llm request: {:?}", self);
        let raw = util::http_utils::post(BASE_URL, &API_KEY, self).await?;
        info!("deepseek llm response: {}", raw);

        Ok(serde_json::from_str(&raw)?)
    }
}
