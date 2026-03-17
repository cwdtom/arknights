use crate::{llm, tool};
use anyhow::anyhow;
use serde::Deserialize;
use crate::llm::base_llm::{ChatResponse, Choice, Message, Role, Tool, ToolCall};

const MAX_TURNS: u8 = 100;
const THINK_PROMPT: &str = "You are the \"think\" node in the ReAct process, \
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
    pub ds: llm::base_llm::Llm,
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

        let ds = llm::deep_seek::Llm::new(messages, tools);
        ReAct { ds }
    }

    pub async fn execute(&mut self) -> anyhow::Result<Message> {
        for _ in 1..=MAX_TURNS {
            // THINK
            let choice = self.think().await?;

            let Message { content, tool_calls, .. } = choice.message;

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
                    self.ds.messages.push(assistant_message);

                    // OBSERVE
                    let tools = self.act(calls).await?;
                    self.ds.messages.extend(tools);
                },
                None => {
                    // set text resp to messages
                    let re_act_resp: ReActResp = serde_json::from_str(&content)?;

                    // reAct done
                    if re_act_resp.is_done {
                        return Ok(Message::new(
                            Role::Assistant,
                            re_act_resp.content,
                        ));
                    }

                    // OBSERVE
                    let assistant = Message::new(Role::Assistant, re_act_resp.content);
                    self.ds.messages.push(assistant);
                }
            }
        }

        Err(anyhow!("reAct exceeded max turns ({MAX_TURNS})"))
    }

    async fn think(&mut self) -> anyhow::Result<Choice> {
        // THINK
        let chat_resp: ChatResponse = self.ds.call().await?;
        match chat_resp.choices.first() {
            Some(choice) => Ok(choice.clone()),
            None => Err(anyhow!("llm response is empty")),
        }
    }

    async fn act(&mut self, calls: Vec<ToolCall>) -> anyhow::Result<Vec<Message>> {
        // tool call
        let mut tools: Vec<Message> = vec![];
        for call in calls {
            let tool_message = match tool::get_tool(&call.function.name) {
                Some(tool) => {
                    let res = tool.deep_seek_call(&call).await;

                    Message {
                        role: Role::Tool,
                        tool_call_id: Some(call.id),
                        content: res.to_string(),
                        tool_calls: None,
                    }
                }
                None => {
                    Message {
                        role: Role::Tool,
                        tool_call_id: Some(call.id),
                        content: "tool not found".to_string(),
                        tool_calls: None,
                    }
                }
            };

            // set tool resp
            tools.push(tool_message);
        }

        Ok(tools)
    }
}
