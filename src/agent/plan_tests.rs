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
