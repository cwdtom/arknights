use crate::agent::ReAct;
use crate::llm::{LlmProvider, Message, Role};
use crate::{im, llm};
use anyhow::anyhow;
use serde::Deserialize;

const MAX_TURNS: u8 = 20;
const PLAN_PROMPT: &str = r#"
You are the PLAN or REPLAN node in a Plan-ReAct-Replan pipeline.

## Role
Given the user's question and any previous execution results, produce a structured plan that guides downstream ReAct nodes to find the answer.

## Available Tools
- system: System-related operations
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

        // TODO set history chat

        // make user message
        let user = Message::new(Role::User, user_message);
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
                    plan_resp.plans
                        .iter()
                        .map(|p| p.task.clone())
                        .collect::<Vec<String>>()
                        .join("\n"),
                );

                Ok(Plan {
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
                        // send final answer
                        im::lark::async_send(plan_resp.content);
                        return Ok(());
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
