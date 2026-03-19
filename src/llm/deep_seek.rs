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
        info!(
            "deepseek llm request: {}",
            serde_json::to_string(&self.llm)?
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::base_llm::{Function, Parameters, Role, Tool};

    #[test]
    fn deep_seek_without_tools_uses_json_output() {
        let message = Message::new(Role::User, "hello".to_string());
        let deep_seek = DeepSeek::new(vec![message], vec![]);

        assert_eq!(deep_seek.llm.model, "deepseek-chat");
        assert_eq!(deep_seek.llm.tool_choice, "none");
        assert_eq!(deep_seek.llm.response_format.r#type, "json_object");
        assert!(deep_seek.llm.tools.is_empty());
    }

    #[test]
    fn deep_seek_with_tools_requires_tool_calls() {
        let message = Message::new(Role::User, "hello".to_string());
        let tool = Tool::new(Function {
            name: "system_date".to_string(),
            description: "Get current date".to_string(),
            parameters: Parameters::new(serde_json::json!({}), vec![]),
        });
        let deep_seek = DeepSeek::new(vec![message], vec![tool]);

        assert_eq!(deep_seek.llm.tool_choice, "required");
        assert_eq!(deep_seek.llm.response_format.r#type, "text");
        assert_eq!(deep_seek.llm.tools.len(), 1);
    }

    #[test]
    fn deep_seek_push_and_extend_messages_append_history() {
        let mut deep_seek = DeepSeek::new(vec![], vec![]);

        deep_seek.push_message(Message::new(Role::User, "first".to_string()));
        deep_seek.extend_messages(vec![
            Message::new(Role::Assistant, "second".to_string()),
            Message::new(Role::Tool, "third".to_string()),
        ]);

        assert_eq!(deep_seek.llm.messages.len(), 3);
        assert_eq!(deep_seek.llm.messages[0].content, "first");
        assert_eq!(deep_seek.llm.messages[1].content, "second");
        assert_eq!(deep_seek.llm.messages[2].content, "third");
    }
}
