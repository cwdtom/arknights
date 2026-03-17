use crate::llm;
use crate::llm::base_llm::{Choice, ToolCall};
use crate::llm::{ChatResponse, LlmProvider, Message, Role, Tool};
use crate::tool;
use anyhow::anyhow;
use serde::Deserialize;

const MAX_TURNS: u8 = 20;
const THINK_PROMPT: &str = "You are the \"think\" node in the ReAct process, \
using appropriate tools to solve problems. \
When it is confirmed that the question has been fully answered, set is_done to true. \
If it is determined that the task needs to be re-planned, set needs_replan to true. \
Response format MUST follow this JSON format: {\"content\":\"response\", \"is_done\": true, \"needs_replan\": true}";

/// reAct resp format
#[derive(Deserialize, Debug)]
pub struct ReActResp {
    pub content: String,
    pub is_done: bool,
    pub needs_replan: bool,
}

/// agent reAct module
pub struct ReAct {
    pub llm: Box<dyn LlmProvider>,
}

impl ReAct {
    pub fn new(mut messages: Vec<Message>) -> Self {
        // system message
        let system: Message = Message::new(Role::System, THINK_PROMPT.to_string());
        messages.insert(0, system);

        let tools = tool::all_tools()
            .iter()
            .map(|t| Tool::new(t.deep_seek_schema()))
            .collect();

        let llm = llm::deep_seek::DeepSeek::new(messages, tools);
        ReAct { llm: Box::new(llm) }
    }

    pub async fn execute(&mut self) -> anyhow::Result<ReActResp> {
        for _ in 1..=MAX_TURNS {
            // THINK
            let choice = self.think().await?;

            let Message {
                content,
                tool_calls,
                ..
            } = choice.message;

            match tool_calls {
                // ACT
                Some(calls) => {
                    // build assistant message
                    let assistant_message = Message {
                        role: Role::Assistant,
                        tool_call_id: None,
                        content,
                        tool_calls: Some(calls.clone()),
                    };
                    // set tool call
                    self.llm.push_message(assistant_message);

                    // OBSERVE
                    let tools = self.act(calls).await?;
                    self.llm.extend_messages(tools);
                }
                None => {
                    // set text resp to messages
                    let re_act_resp: ReActResp = serde_json::from_str(&content)?;

                    // reAct done
                    if re_act_resp.is_done || re_act_resp.needs_replan {
                        return Ok(re_act_resp);
                    }

                    // OBSERVE
                    let assistant = Message::new(Role::Assistant, re_act_resp.content);
                    self.llm.push_message(assistant);
                }
            }
        }

        Err(anyhow!("reAct exceeded max turns ({MAX_TURNS})"))
    }

    async fn think(&mut self) -> anyhow::Result<Choice> {
        // THINK
        let chat_resp: ChatResponse = self.llm.call().await?;
        match chat_resp.choices.first() {
            Some(choice) => Ok(choice.clone()),
            None => Err(anyhow!("llm response is empty")),
        }
    }

    async fn act(&mut self, calls: Vec<ToolCall>) -> anyhow::Result<Vec<Message>> {
        let futures: Vec<_> = calls
            .into_iter()
            .map(|call| async move {
                let content = match tool::get_tool(&call.function.name) {
                    Some(t) => t.deep_seek_call(&call).await.to_string(),
                    None => "tool not found".to_string(),
                };
                Message {
                    role: Role::Tool,
                    tool_call_id: Some(call.id),
                    content,
                    tool_calls: None,
                }
            })
            .collect();

        Ok(futures::future::join_all(futures).await)
    }
}
