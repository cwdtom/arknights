use serde::Deserialize;
use tracing::error;
use crate::llm::deep_seek::{Message, Tool};
use crate::{llm, tool};

const MAX_TURNS: i8 = 100;
const THINK_PROMPT: &'static str = "You are the \"think\" node in the ReAct process, \
using appropriate tools to solve problems. \
When it is confirmed that the question has been fully answered, set is_done to true. \
Response format MUST follow this JSON format: {\"content\":\"response\", \"is_done\": true}";

/// reAct resp format
#[derive(Deserialize, Debug)]
pub struct ReActResp {
    pub content: String,
    pub is_done: bool,
}

/// agent reAct module
pub struct ReAct {
    pub ds: llm::deep_seek::DeepSeek,
}

impl ReAct {
    pub fn new(mut messages: Vec<Message>) -> Self {
        // system message
        let system: Message = Message::new(llm::deep_seek::Role::System, THINK_PROMPT.to_string());
        messages.insert(0, system);

        // tools
        let tools = tool::all_tools()
            .iter()
            .map(|t| Tool::new(t.deep_seek_schema()))
            .collect();

        let ds = llm::deep_seek::DeepSeek::new(messages, tools);
        ReAct { ds }
    }

    pub async fn execute(&mut self) -> Result<Message, String> {
        let mut turns = 1;

        while turns < MAX_TURNS {
            turns += 1;

            match self.ds.call().await {
                Ok(re_act_resp) => {
                    if re_act_resp.is_done {
                        return Ok(Message::new(llm::deep_seek::Role::Assistant, re_act_resp.content.to_string()));
                    }

                    if turns >= MAX_TURNS {
                        error!("reAct turns exceeded");
                        return Err("reAct turn exceeded".to_string());
                    }
                },
                Err(e) => {
                    error!("reAct execute error: {}", e);
                    return Err("reAct execute error".to_string());
                }
            }
        }

        Err("compiler need".to_string())
    }
}
