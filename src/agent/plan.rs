use crate::agent::ReAct;
use crate::llm::{LlmProvider, Message, Role};
use crate::{im, llm, memory};
use anyhow::anyhow;
use serde::Deserialize;
use std::collections::HashSet;
use chrono::Local;
use tracing::error;

const MAX_TURNS: u8 = 20;
const PLAN_PROMPT: &str = r#"
You are the PLAN or REPLAN node in a Plan-ReAct-Replan pipeline.

## Role
First, Expand user's colloquial question into a complete and unambiguous one.
Then given the expanded question and any previous execution results, produce a structured plan that guides downstream ReAct nodes to find the answer.

## Available Tools
- system: System-related info or operations
- internet: Internet-related operations
- memory: Memory-related search

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
4. For questions involving relative time, be sure to check the current time first.
5. Each subtask MUST contain the necessary information and be able to be completed independently.

## Language Rule
`content`, `expand_goal` and every `task` field must be written in the same language as the user's message.

## Output Format Json
{
  "expand_goal": "<expanded question>",
  "plans": [
    {"task": "<subtask description>", "tools": ["internet"]},
    {"task": "<subtask description>", "tools": []}
  ],
  "content": "<final answer>",
  "is_done": false
}
"#;

/// agent plan module
pub struct Plan {
    question: String,
    plans: Vec<Task>,
    llm: Box<dyn LlmProvider>,
    // if first plan can answer
    answer: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct PlanResp {
    plans: Vec<Task>,
    #[serde(default)]
    is_done: bool,
    content: String,
    expand_goal: String,
}

#[derive(Deserialize, Debug)]
pub struct Task {
    task: String,
    tools: HashSet<String>,
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
        let now = Local::now().to_rfc3339();
        let user = Message::new(Role::User, format!("[{now}] {user_message}"));
        messages.push(user);

        // make plan
        let mut llm = llm::deep_seek::DeepSeek::new(messages, vec![]);
        let chat_resp = llm.call().await?;
        match chat_resp.choices.first() {
            Some(choice) => {
                let plan_resp: PlanResp = serde_json::from_str(&choice.message.content)?;
                llm.push_message(choice.message.clone());

                // plan already answer
                if plan_resp.is_done {
                    return Ok(Plan {
                        question: plan_resp.expand_goal,
                        plans: vec![],
                        llm: Box::new(llm),
                        answer: Some(plan_resp.content),
                    });
                }

                // send replans
                im::lark::async_send(plan_resp.expand_goal.clone());

                Ok(Plan {
                    question: plan_resp.expand_goal,
                    plans: plan_resp.plans,
                    llm: Box::new(llm),
                    answer: None,
                })
            }
            None => Err(anyhow!("llm response is empty")),
        }
    }

    pub async fn execute(&mut self) -> anyhow::Result<()> {
        if let Some(answer) = self.answer.take() {
            match memory::chat_history_service::save_chat_history(self.question.as_str(), &answer)
                .await
            {
                Ok(_) => {}
                Err(err) => error!("Failed to save chat history: {}", err),
            }

            im::lark::async_send(answer);
            return Ok(());
        }

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
                    self.question = plan_resp.expand_goal.clone();

                    if plan_resp.is_done {
                        // save chat history
                        match memory::chat_history_service::save_chat_history(
                            self.question.as_str(),
                            plan_resp.content.as_str(),
                        )
                        .await
                        {
                            Ok(_) => {}
                            Err(err) => error!("Failed to save chat history: {}", err),
                        }

                        // send final answer
                        im::lark::async_send(plan_resp.content);

                        return Ok(());
                    } else {
                        // update plans
                        self.llm.push_message(choice.message.clone());
                        self.plans = plan_resp.plans;
                        self.question = plan_resp.expand_goal;

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
    use crate::llm::ChatResponse;
    use crate::memory::rag_embedder;
    use std::collections::VecDeque;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn plan_resp_defaults_is_done_to_false() {
        let json = r#"{
            "expand_goal": "collect context with explicit scope",
            "plans": [{"task":"collect context","tools":["internet"]}],
            "content": ""
        }"#;

        let resp: PlanResp = serde_json::from_str(json).unwrap();
        assert!(!resp.is_done);
        assert_eq!(resp.content, "");
        assert_eq!(resp.expand_goal, "collect context with explicit scope");
        assert_eq!(resp.plans.len(), 1);
        assert_eq!(resp.plans[0].task, "collect context");
        let mut tools = HashSet::new();
        tools.insert(String::from("internet"));
        assert_eq!(resp.plans[0].tools, tools);
    }

    #[test]
    fn plan_resp_accepts_done_payload() {
        let json = r#"{
            "expand_goal": "final answer with explicit scope",
            "plans": [],
            "content": "final answer",
            "is_done": true
        }"#;

        let resp: PlanResp = serde_json::from_str(json).unwrap();
        assert!(resp.is_done);
        assert_eq!(resp.content, "final answer");
        assert_eq!(resp.expand_goal, "final answer with explicit scope");
        assert!(resp.plans.is_empty());
    }

    #[tokio::test]
    async fn execute_persists_latest_expand_goal_after_replan() {
        let _guard = rag_embedder::TEST_ENV_LOCK.lock().unwrap();
        disable_rag_and_set_lark_env();
        let token = unique_token("replan");
        let initial_question = format!("initial-question-{token}");
        let final_question = format!("final-question-{token}");
        let final_answer = format!("final-answer-{token}");
        let response = plan_chat_response(&format!(
            r#"{{
                "expand_goal": "{final_question}",
                "plans": [],
                "content": "{final_answer}",
                "is_done": true
            }}"#
        ));
        let mut plan = Plan {
            question: initial_question,
            plans: vec![],
            llm: Box::new(TestLlm::new(vec![response])),
            answer: None,
        };

        plan.execute().await.unwrap();

        let messages = memory::chat_history_service::build_chat_history_messages(100)
            .await
            .unwrap();
        let matched_messages: Vec<_> = messages
            .into_iter()
            .filter(|message| message.content.contains(&token))
            .collect();

        assert_eq!(matched_messages.len(), 2);
        assert!(matches!(matched_messages[0].role, Role::User));
        assert_timestamped_message(&matched_messages[0].content, &final_question);
        assert!(matches!(matched_messages[1].role, Role::Assistant));
        assert_timestamped_message(&matched_messages[1].content, &final_answer);
    }

    struct TestLlm {
        responses: VecDeque<ChatResponse>,
    }

    impl TestLlm {
        fn new(responses: Vec<ChatResponse>) -> Self {
            Self {
                responses: responses.into(),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for TestLlm {
        async fn call(&mut self) -> anyhow::Result<ChatResponse> {
            self.responses
                .pop_front()
                .ok_or_else(|| anyhow!("test llm response queue is empty"))
        }

        fn push_message(&mut self, _message: Message) {}

        fn extend_messages(&mut self, _messages: Vec<Message>) {}
    }

    fn plan_chat_response(content: &str) -> ChatResponse {
        serde_json::from_value(serde_json::json!({
            "id": "test-chat-response",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": content
                }
            }]
        }))
        .unwrap()
    }

    fn disable_rag_and_set_lark_env() {
        rag_embedder::clear_test_embedding_mode();
        unsafe {
            std::env::remove_var("ARKNIGHTS_RAG_MODEL");
            std::env::set_var("LARK_APP_ID", "test-app-id");
            std::env::set_var("LARK_APP_SECRET", "test-app-secret");
            std::env::set_var("LARK_USER_OPEN_ID", "test-open-id");
        }
    }

    fn unique_token(label: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("plan-tests-{label}-{nanos}")
    }

    fn assert_timestamped_message(actual: &str, expected_suffix: &str) {
        let (prefix, suffix) = actual
            .split_once("] ")
            .expect("message should contain RFC3339 prefix");
        assert!(prefix.starts_with('['));
        chrono::DateTime::parse_from_rfc3339(&prefix[1..]).unwrap();
        assert_eq!(suffix, expected_suffix);
    }
}
