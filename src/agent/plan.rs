use crate::agent::{ReAct};
use crate::llm;
use crate::llm::{LlmProvider, Message, Role};
use anyhow::anyhow;
use serde::Deserialize;
use tracing::info;

const MAX_TURNS: u8 = 20;
const PLAN_PROMPT: &str = "You are the \"PLAN\" node in the Plan-ReAct-Replan process, \
formulate an appropriate execution plan to answer the user's question. \
When it is confirmed that the question has been fully answered, set is_done to true. \
When unable to provide an answer, reformulate the plan. \
You have these tools(no need to actually call it, just reflect it in the plan) available for use: \
- system: System-related tools \
Response format MUST follow this JSON format: {\"plans\": [\"subTaskA\",\"subTaskB\"], \"content\": \"final answer\",\"is_done\": true}";

/// agent plan module
pub struct Plan {
    plans: Vec<String>,
    llm: Box<dyn LlmProvider>,
}

#[derive(Deserialize, Debug)]
pub struct PlanResp {
    plans: Vec<String>,
    #[serde(default)]
    is_done: bool,
    content: String,
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
                let sub_message = Message::new(Role::User, plan.to_string());
                re_act_history.push(sub_message);

                // init reAct
                let mut re_act = ReAct::new(re_act_history.clone());
                let re_act_resp = re_act.execute().await?;
                // set sub answer
                let answer = Message::new(Role::User, re_act_resp.content);
                self.llm.push_message(answer.clone());
                re_act_history.push(answer);

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
                        // finish
                        info!("final answer: {}", plan_resp.content);
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
