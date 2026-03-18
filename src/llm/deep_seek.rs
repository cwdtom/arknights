use crate::llm::base_llm::{ChatResponse, Llm, LlmProvider, Message, ResponseFormat, Tool};
use crate::util;
use std::sync::LazyLock;
use tracing::info;

const BASE_URL: &str = "https://api.deepseek.com/chat/completions";
static API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY not set"));

#[derive(Debug)]
pub struct DeepSeek {
    llm: Llm,
}

impl DeepSeek {
    pub fn new(messages: Vec<Message>, tools: Vec<Tool>) -> Self {
        // needs call tool
        let mut tool_choice = "required";
        let mut resp_format = "text";
        if tools.is_empty() {
            tool_choice = "none";
            resp_format = "json_object";
        }

        let llm = Llm {
            model: "deepseek-chat".to_string(),
            messages,
            stream: false,
            max_tokens: 8192,
            response_format: ResponseFormat {
                r#type: resp_format.to_string(),
            },
            temperature: 1.0,
            tools,
            tool_choice: tool_choice.to_string(),
        };

        Self { llm }
    }
}

#[async_trait::async_trait]
impl LlmProvider for DeepSeek {
    async fn call(&mut self) -> anyhow::Result<ChatResponse> {
        info!("deepseek llm request: {}", serde_json::to_string(&self.llm)?);
        let raw = util::http_utils::post(BASE_URL, &API_KEY, &self.llm).await?;
        info!("deepseek llm response: {}", raw);

        Ok(serde_json::from_str(&raw)?)
    }

    fn push_message(&mut self, message: Message) {
        self.llm.messages.push(message);
    }

    fn extend_messages(&mut self, messages: Vec<Message>) {
        self.llm.messages.extend(messages);
    }
}
