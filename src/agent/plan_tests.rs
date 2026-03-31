use super::*;
use crate::im::base_im::{self, Im};
use crate::llm::ChatResponse;
use crate::llm::base_llm::{Llm, LlmProvider};
use crate::test_support;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[test]
fn plan_resp_defaults_is_done_to_false() {
    let json = r#"{
        "expand_goal": "collect context with explicit scope",
        "plans": ["collect context"],
        "tools": ["internet"]
    }"#;

    let resp: PlanResp = serde_json::from_str(json).unwrap();
    assert!(!resp.is_done);
    assert!(resp.content.is_none());
    assert_eq!(resp.expand_goal, "collect context with explicit scope");
    assert_eq!(resp.plans.len(), 1);
    assert_eq!(resp.plans[0], "collect context");
    let mut tools = HashSet::new();
    tools.insert(String::from("internet"));
    assert_eq!(resp.tools, tools);
}

#[test]
fn plan_resp_accepts_done_payload() {
    let json = r#"{
        "expand_goal": "final answer with explicit scope",
        "plans": [],
        "tools": [],
        "content": {
            "text": "final answer",
            "files": []
        },
        "is_done": true
    }"#;

    let resp: PlanResp = serde_json::from_str(json).unwrap();
    assert!(resp.is_done);
    let content = resp.content.expect("done payload should include content");
    assert_eq!(content.text, "final answer");
    assert!(content.files.is_empty());
    assert_eq!(resp.expand_goal, "final answer with explicit scope");
    assert!(resp.plans.is_empty());
}

#[test]
fn build_system_prompt_includes_user_profile_section() {
    let prompt = build_system_prompt("prefers concise answers");

    assert!(prompt.contains("## User profile"));
    assert!(prompt.contains("prefers concise answers"));
}

#[test]
fn plan_prompt_mentions_browser_tool_group() {
    assert!(PLAN_PROMPT.contains("browser"));
}

#[tokio::test]
async fn execute_persists_latest_expand_goal_after_replan() {
    let _guard = test_support::app_test_guard().await;
    disable_rag_and_set_lark_env();
    let token = test_support::unique_test_token("plan-tests", "replan");
    let initial_question = format!("initial-question-{token}");
    let final_question = format!("final-question-{token}");
    let final_answer = format!("final-answer-{token}");
    install_fake_im(Arc::new(Mutex::new(vec![]))).await;
    let response = plan_chat_response(&format!(
        r#"{{
            "expand_goal": "{final_question}",
            "plans": [],
            "tools": [],
            "content": {{
                "text": "{final_answer}",
                "files": []
            }},
            "is_done": true
        }}"#
    ));
    let mut plan = Plan {
        question: initial_question,
        plans: vec![],
        tools: HashSet::new(),
        llm: Llm {
            llm_provider: Box::new(TestLlm::new(vec![response])),
        },
        answer: None,
    };

    plan.execute().await.unwrap();
    tokio::task::yield_now().await;

    let messages = memory::chat_history_service::build_chat_history_messages(100)
        .await
        .unwrap();
    let matched_messages: Vec<_> = messages
        .into_iter()
        .filter(|message| message.content.contains(&token))
        .collect();

    assert_eq!(matched_messages.len(), 2);
    assert!(matches!(matched_messages[0].role, Role::User));
    test_support::assert_timestamped_message(&matched_messages[0].content, &final_question);
    assert!(matches!(matched_messages[1].role, Role::Assistant));
    test_support::assert_timestamped_message(&matched_messages[1].content, &final_answer);
}

#[tokio::test]
async fn execute_sends_final_answer_via_im_without_background_panic() {
    let _guard = test_support::app_test_guard().await;
    disable_rag_and_set_lark_env();
    let token = test_support::unique_test_token("plan-tests", "send");
    let final_question = format!("final-question-{token}");
    let final_answer = format!("final-answer-{token}");
    let response = plan_chat_response(&format!(
        r#"{{
            "expand_goal": "{final_question}",
            "plans": [],
            "tools": [],
            "content": {{
                "text": "{final_answer}",
                "files": []
            }},
            "is_done": true
        }}"#
    ));
    let sent_messages = Arc::new(Mutex::new(vec![]));
    install_fake_im(sent_messages.clone()).await;
    let mut plan = Plan {
        question: format!("initial-question-{token}"),
        plans: vec![],
        tools: HashSet::new(),
        llm: Llm {
            llm_provider: Box::new(TestLlm::new(vec![response])),
        },
        answer: None,
    };

    plan.execute().await.unwrap();
    tokio::task::yield_now().await;

    assert_eq!(take_sent_messages(&sent_messages), vec![final_answer]);
}

#[tokio::test]
async fn execute_with_prefilled_answer_sends_final_message() {
    let _guard = test_support::app_test_guard().await;
    disable_rag_and_set_lark_env();
    let final_answer = format!(
        "prefilled-answer-{}",
        test_support::unique_test_token("plan-tests", "prefilled")
    );
    let sent_messages = Arc::new(Mutex::new(vec![]));
    install_fake_im(sent_messages.clone()).await;
    let mut plan = Plan {
        question: format!(
            "prefilled-question-{}",
            test_support::unique_test_token("plan-tests", "prefilled-question")
        ),
        plans: vec![],
        tools: HashSet::new(),
        llm: Llm {
            llm_provider: Box::new(TestLlm::new(vec![])),
        },
        answer: Some(Content {
            text: final_answer.clone(),
            files: vec![],
        }),
    };

    let result = plan.execute().await.unwrap();
    tokio::task::yield_now().await;

    assert_eq!(result, final_answer);
    assert_eq!(take_sent_messages(&sent_messages), vec![final_answer]);
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
    test_support::configure_app_test_env();
    test_support::disable_rag_for_test();
}

async fn install_fake_im(sent_messages: Arc<Mutex<Vec<String>>>) {
    base_im::install_test_im(Box::new(FakeIm { sent_messages })).await;
}

fn take_sent_messages(sent_messages: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    let mut guard = sent_messages.lock().unwrap();
    std::mem::take(&mut *guard)
}

struct FakeIm {
    sent_messages: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl Im for FakeIm {
    async fn send(&mut self, content: String) -> anyhow::Result<()> {
        self.sent_messages.lock().unwrap().push(content);
        Ok(())
    }

    async fn ask_user(&mut self, _question: String) -> anyhow::Result<String> {
        anyhow::bail!("ask_user should not be called in this test")
    }

    async fn reply_emoji(&mut self, _message_id: String, _emoji: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn send_file(&mut self, _file: File) -> anyhow::Result<()> {
        Ok(())
    }
}
