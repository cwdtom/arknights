use crate::agent::ReAct;
use crate::llm::{LlmProvider, Message, Role};
use crate::{im, llm, memory};
use anyhow::anyhow;
use serde::Deserialize;

const MAX_TURNS: u8 = 20;
const PLAN_PROMPT: &str = r#"
You are the PLAN or REPLAN node in a Plan-ReAct-Replan pipeline.

## Role
Given the user's question and any previous execution results, produce a structured plan that guides downstream ReAct nodes to find the answer.

## Available Tools
- internet: Internet-related operations

## Decision Rules
1. If the question has NOT been fully answered yet:
   - Decompose it into ordered subtasks.
   - Assign relevant tools to each subtask.
   - Set `is_done` to false and leave `content` empty.

2. If the question HAS been fully answered:
   - Set `is_done` to true.
   - Write the final answer in `content`.

3. If the current plan failed or is insufficient:
   - Reformulate a new plan with alternative subtasks and tools.
   - Set `is_done` to false and leave `content` empty.

4. Each subtask MUST contain the necessary information and be able to be completed independently.

## Language Rule
`content` and every `task` field must be written in the same language as the user's message.

## Output Format Json
{
  "plans": [
    {"task": "<subtask description>", "tools": ["system"]},
    {"task": "<subtask description>", "tools": []}
  ],
  "content": "",
  "is_done": false
}
"#;

/// agent plan module
pub struct Plan {
    content: String,
    plans: Vec<Task>,
    llm: Box<dyn LlmProvider>,
}

#[derive(Deserialize, Debug)]
pub struct PlanResp {
    plans: Vec<Task>,
    #[serde(default)]
    is_done: bool,
    content: String,
}

#[derive(Deserialize, Debug)]
pub struct Task {
    task: String,
    tools: Vec<String>,
}

impl Plan {
    pub async fn new(user_message: String) -> anyhow::Result<Self> {
        // set system prompt
        let system = Message::new(Role::System, PLAN_PROMPT.to_string());
        let mut messages = vec![system];

        // set history chat
        let history = memory::chat_history_service::build_chat_history_messages(20).await?;
        messages.extend(history);

        // make user message
        let user = Message::new(Role::User, user_message.clone());
        messages.push(user);

        // make plan
        let mut llm = llm::deep_seek::DeepSeek::new(messages, vec![]);
        let chat_resp = llm.call().await?;
        match chat_resp.choices.first() {
            Some(choice) => {
                let plan_resp: PlanResp = serde_json::from_str(&choice.message.content)?;
                llm.push_message(choice.message.clone());

                // send replans
                im::lark::async_send(
                    plan_resp
                        .plans
                        .iter()
                        .map(|p| p.task.clone())
                        .collect::<Vec<String>>()
                        .join("\n"),
                );

                Ok(Plan {
                    content: user_message,
                    plans: plan_resp.plans,
                    llm: Box::new(llm),
                })
            }
            None => Err(anyhow!("llm response is empty")),
        }
    }

    pub async fn execute(&mut self) -> anyhow::Result<()> {
        let mut re_act_history: Vec<Message> = vec![];

        for _ in 1..=MAX_TURNS {
            for plan in &self.plans {
                // set sub task
                let sub_message = Message::new(Role::User, plan.task.clone());
                re_act_history.push(sub_message);

                // init reAct
                let mut re_act = ReAct::new(re_act_history.clone(), plan.tools.clone())?;
                let re_act_resp = re_act.execute().await?;
                // set sub answer
                let answer = Message::new(Role::User, re_act_resp.content);
                self.llm.push_message(answer.clone());
                re_act_history.push(answer.clone());

                // send reAct answer
                im::lark::async_send(plan.task.clone() + " Done");

                if re_act_resp.needs_replan {
                    break;
                }
            }

            // REPLAN
            let chat_resp = self.llm.call().await?;
            match chat_resp.choices.first() {
                Some(choice) => {
                    let plan_resp: PlanResp = serde_json::from_str(&choice.message.content)?;

                    if plan_resp.is_done {
                        // save chat history
                        return match memory::chat_history_service::save_chat_history(self.content.as_str(), plan_resp.content.as_str()).await {
                            Ok(_) => {
                                // send final answer
                                im::lark::async_send(plan_resp.content);

                                Ok(())
                            },
                            Err(err) => Err(anyhow!("save chat history error: {err:#}")),
                        }
                    } else {
                        // update plans
                        self.llm.push_message(choice.message.clone());
                        self.plans = plan_resp.plans;

                        continue;
                    }
                }
                None => return Err(anyhow!("llm response is empty")),
            }
        }

        Err(anyhow!("plan exceeded max turns ({MAX_TURNS})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_resp_defaults_is_done_to_false() {
        let json = r#"{
            "plans": [{"task":"collect context","tools":["system","internet"]}],
            "content": ""
        }"#;

        let resp: PlanResp = serde_json::from_str(json).unwrap();
        assert!(!resp.is_done);
        assert_eq!(resp.content, "");
        assert_eq!(resp.plans.len(), 1);
        assert_eq!(resp.plans[0].task, "collect context");
        assert_eq!(
            resp.plans[0].tools,
            vec!["system".to_string(), "internet".to_string()]
        );
    }

    #[test]
    fn plan_resp_accepts_done_payload() {
        let json = r#"{
            "plans": [],
            "content": "final answer",
            "is_done": true
        }"#;

        let resp: PlanResp = serde_json::from_str(json).unwrap();
        assert!(resp.is_done);
        assert_eq!(resp.content, "final answer");
        assert!(resp.plans.is_empty());
    }
}
