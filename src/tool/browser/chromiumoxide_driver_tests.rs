use super::chromiumoxide_runtime::{
    combine_cleanup_results, shutdown_handler_task, take_first_or_stale,
};
use super::{
    ClickTool, CloseTool, FillTool, GetHtmlTool, GetTextTool, NavigateTool, ScreenshotTool,
    ScrollTool, SnapshotTool, WaitTextTool, run_with_default_browser_scope,
};
use crate::llm::base_llm::{FunctionCall, ToolCall};
use crate::tool::base_tool::LlmTool;
use anyhow::anyhow;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

const TEST_PAGE_HTML: &str = r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Browser Smoke</title><style>body{font-family:sans-serif;margin:0;padding:24px}.spacer{height:1200px}#hidden-panel{display:none;margin-top:24px}</style></head><body><h1>Browser Smoke</h1><label for="name-input">Name</label><input id="name-input" name="name-input"/><button id="apply-button" type="button" onclick="document.getElementById('status').textContent='Hello, '+document.getElementById('name-input').value;document.getElementById('hidden-panel').style.display='block';">Apply</button><p id="status">Waiting</p><div class="spacer"></div><section id="hidden-panel"><button id="finish-button" type="button">Finish</button></section></body></html>"#;

#[test]
fn take_first_or_stale_returns_stale_error_for_empty_lookup() {
    let error = take_first_or_stale::<i32>(vec![]).unwrap_err();

    assert_eq!(error.code, "element_id_stale");
    assert_eq!(error.message, "call browser_snapshot again");
}

#[test]
fn combine_cleanup_results_preserves_primary_error_and_mentions_cleanup_failure() {
    let error = combine_cleanup_results(
        Err(anyhow!("new page failed")),
        Err(anyhow!("handler cleanup failed")),
        "browser launch cleanup",
    )
    .unwrap_err();

    let rendered = error.to_string();
    assert!(rendered.contains("new page failed"));
    assert!(rendered.contains("browser launch cleanup also failed"));
    assert!(rendered.contains("handler cleanup failed"));
}

#[tokio::test]
async fn shutdown_handler_task_cancels_pending_task() {
    let handle = tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok::<(), anyhow::Error>(())
    });

    shutdown_handler_task(handle).await.unwrap();
}

#[tokio::test]
async fn chromiumoxide_smoke_flow() {
    let server = TestPageServer::spawn(TEST_PAGE_HTML).await;
    run_with_default_browser_scope(async {
        let first = parse(&NavigateTool::new(), json!({ "url": server.url() })).await;
        assert_eq!(first["ok"], true);

        let first_snapshot = parse(&SnapshotTool::new(), json!({})).await;
        assert_eq!(first_snapshot["result"]["url"], server.url());
        assert_eq!(first_snapshot["result"]["title"], "Browser Smoke");
        assert_eq!(first_snapshot["result"]["scroll_y"], 0);
        assert!(
            first_snapshot["result"]["document_height"]
                .as_i64()
                .unwrap()
                > 0
        );
        let input_id = find_id(&first_snapshot["result"]["elements"], "name-input");
        let apply_id = find_id(&first_snapshot["result"]["elements"], "Apply");

        let fill = parse(
            &FillTool::new(),
            json!({ "element_id": input_id, "value": "Codex" }),
        )
        .await;
        assert_eq!(fill["ok"], true);

        let click = parse(&ClickTool::new(), json!({ "element_id": apply_id })).await;
        assert_eq!(click["ok"], true);

        let waited = parse(
            &WaitTextTool::new(),
            json!({ "text": "Hello, Codex", "timeout_ms": 3_000 }),
        )
        .await;
        assert_eq!(waited["ok"], true);

        let second_snapshot = parse(&SnapshotTool::new(), json!({})).await;
        let finish_id = find_id(&second_snapshot["result"]["elements"], "Finish");

        let scroll = parse(&ScrollTool::new(), json!({ "element_id": finish_id })).await;
        assert_eq!(scroll["ok"], true);

        let text = parse(&GetTextTool::new(), json!({})).await;
        assert!(
            text["result"]["text"]
                .as_str()
                .unwrap()
                .contains("Hello, Codex")
        );

        let html = parse(&GetHtmlTool::new(), json!({ "element_id": finish_id })).await;
        assert!(html["result"]["html"].as_str().unwrap().contains("Finish"));

        let shot = parse(&ScreenshotTool::new(), json!({})).await;
        let shot_path = PathBuf::from(shot["result"]["path"].as_str().unwrap());
        assert!(shot_path.is_absolute());
        assert!(shot_path.exists());

        let close = parse(&CloseTool::new(), json!({})).await;
        assert_eq!(close["ok"], true);
        Ok::<(), anyhow::Error>(())
    })
    .await
    .unwrap();
}

async fn parse<T: LlmTool>(tool: &T, arguments: Value) -> Value {
    let call = ToolCall {
        id: format!("call_{}", tool.name()),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: tool.name().to_string(),
            arguments: arguments.to_string(),
        },
    };
    serde_json::from_str(&tool.deep_seek_call(&call).await).unwrap()
}

fn find_id(elements: &Value, needle: &str) -> String {
    elements
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element.to_string().contains(needle))
        .and_then(|element| element["id"].as_str())
        .unwrap()
        .to_string()
}

struct TestPageServer {
    url: String,
    task: JoinHandle<()>,
}

impl TestPageServer {
    async fn spawn(html: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 2048];
                let _ = socket.read(&mut request).await;
                let wants_favicon = String::from_utf8_lossy(&request).contains("GET /favicon.ico");
                let body = if wants_favicon { "" } else { html };
                let status = if wants_favicon {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        Self {
            url: format!("http://{address}/"),
            task,
        }
    }

    fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for TestPageServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
