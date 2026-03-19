use crate::llm;
use crate::llm::base_llm::{Choice, ToolCall, ToolResult};
use crate::llm::{ChatResponse, LlmProvider, Message, Role, Tool};
use crate::tool;
use anyhow::anyhow;
use serde::Deserialize;

const MAX_TURNS: u8 = 20;
const THINK_PROMPT: &str = r#"
You are the "think" node in a ReAct loop.
Primary rule:
- Using appropriate tools to solve problems.

Output contract:
- Keep the tool args language as same as the user's message.
"#;

/// reAct resp format
#[derive(Deserialize, Debug)]
pub struct ReActResp {
    pub content: String,
    #[serde(default)]
    pub is_done: bool,
    #[serde(default)]
    pub needs_replan: bool,
}

/// agent reAct module
pub struct ReAct {
    pub llm: Box<dyn LlmProvider>,
}

impl ReAct {
    pub fn new(mut messages: Vec<Message>, mut tools_group: Vec<String>) -> anyhow::Result<Self> {
        // system message
        let system: Message = Message::new(Role::System, THINK_PROMPT.to_string());
        messages.insert(0, system);

        // default add process control
        tools_group.push("process_control".to_string());

        let tools: Vec<Tool> = tools_group.iter()
            .flat_map(|t| tool::get_tool_by_group(t))
            .map(|t| Tool::new(t.deep_seek_schema()))
            .collect();

        let llm = llm::deep_seek::DeepSeek::new(messages, tools);
        Ok(ReAct { llm: Box::new(llm) })
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

                    let tool_results = self.act(calls).await?;

                    // check last tool call done or replan
                    match tool_results.last() {
                        Some(r) => {
                            // check done or replan
                            if r.replan || r.done {
                                return Ok(ReActResp {
                                    content: r.content.to_string(),
                                    is_done: r.done,
                                    needs_replan: r.replan,
                                });
                            }
                        }
                        None => {
                            return Ok(ReActResp {
                                content: "".to_string(),
                                is_done: false,
                                needs_replan: true,
                            });
                        }
                    };

                    let messages = tool_results.iter()
                        .map(|r| Message {
                            role: Role::Tool,
                            tool_call_id: r.tool_call_id.clone(),
                            content: r.content.clone(),
                            tool_calls: None,
                        }).collect::<Vec<Message>>();
                    // OBSERVE
                    self.llm.extend_messages(messages);
                }
                None => return Err(anyhow!("call tool error")),
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

    async fn act(&mut self, calls: Vec<ToolCall>) -> anyhow::Result<Vec<ToolResult>> {
        let futures: Vec<_> = calls
            .into_iter()
            .map(|call| async move {
                let content = match tool::get_tool(&call.function.name) {
                    Some(t) => t.deep_seek_call(&call).await.to_string(),
                    None => "tool not found".to_string(),
                };
                ToolResult {
                    tool_call_id: Some(call.id),
                    content,
                    replan: call.function.name == "process_control_replan",
                    done: call.function.name == "process_control_done",
                }
            })
            .collect();

        Ok(futures::future::join_all(futures).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::base_llm::FunctionCall;

    #[test]
    fn re_act_resp_full_json() {
        let json = r#"{"content":"answer","is_done":true,"needs_replan":false}"#;
        let resp: ReActResp = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content, "answer");
        assert!(resp.is_done);
        assert!(!resp.needs_replan);
    }

    #[test]
    fn re_act_resp_defaults_to_false() {
        let json = r#"{"content":"thinking"}"#;
        let resp: ReActResp = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content, "thinking");
        assert!(!resp.is_done);
        assert!(!resp.needs_replan);
    }

    #[test]
    fn re_act_resp_needs_replan() {
        let json = r#"{"content":"need to replan","is_done":false,"needs_replan":true}"#;
        let resp: ReActResp = serde_json::from_str(json).unwrap();
        assert!(resp.needs_replan);
        assert!(!resp.is_done);
    }

    #[tokio::test]
    async fn act_marks_done_for_done_tool() {
        let mut react =
            ReAct::new(vec![Message::new(Role::User, "task".to_string())], vec![]).unwrap();
        let tool_call = ToolCall {
            id: "call_done".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "process_control_done".to_string(),
                arguments: r#"{"answer":"task finished"}"#.to_string(),
            },
        };

        let results = react.act(vec![tool_call]).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "task finished");
        assert!(results[0].done);
        assert!(!results[0].replan);
    }

    #[tokio::test]
    async fn act_marks_replan_for_replan_tool() {
        let mut react =
            ReAct::new(vec![Message::new(Role::User, "task".to_string())], vec![]).unwrap();
        let tool_call = ToolCall {
            id: "call_replan".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "process_control_replan".to_string(),
                arguments: r#"{"reason":"need another path"}"#.to_string(),
            },
        };

        let results = react.act(vec![tool_call]).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "need another path");
        assert!(!results[0].done);
        assert!(results[0].replan);
    }

    #[tokio::test]
    async fn act_returns_tool_not_found_for_unknown_tool() {
        let mut react =
            ReAct::new(vec![Message::new(Role::User, "task".to_string())], vec![]).unwrap();
        let tool_call = ToolCall {
            id: "call_unknown".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "missing_tool".to_string(),
                arguments: "{}".to_string(),
            },
        };

        let results = react.act(vec![tool_call]).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "tool not found");
        assert!(!results[0].done);
        assert!(!results[0].replan);
    }
}
