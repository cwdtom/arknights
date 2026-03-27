# Arknights

Arknights is a Rust 2024 agent service that runs behind a Feishu/Lark bot.
It receives text messages from Lark, expands and plans work with DeepSeek,
executes subtasks through a Plan -> ReAct -> Replan pipeline, can also execute
scheduled timer tasks through the same pipeline, and sends the final answer
back through Lark.

## Features

- Plan -> ReAct -> Replan workflow driven by DeepSeek chat completions
- Feishu/Lark websocket integration for inbound messages and outbound replies
- Pluggable async tool system for system, internet, process-control, memory, and timer tools
- SQLite-backed chat history persistence plus KV-backed user profile storage
- Built-in timer scheduler that replays saved prompts through the agent pipeline
- Optional RAG indexing and retrieval with `sqlite-vec` and `fastembed`

## Prerequisites

- A Rust toolchain with edition 2024 support
- A Feishu/Lark app with websocket message delivery enabled
- A DeepSeek API key
- A Bocha API key if you want the internet search tools to work

## Environment Variables

Copy `.env.example` to `.env` and fill in the values you need.

| Variable | Required | Purpose |
| --- | --- | --- |
| `DEEPSEEK_API_KEY` | Yes | Used by `src/llm/deep_seek.rs` for planner and ReAct model calls. |
| `LARK_APP_ID` | Yes | Feishu/Lark application ID used by `src/im/lark.rs`. |
| `LARK_APP_SECRET` | Yes | Feishu/Lark application secret used to fetch tenant access tokens. |
| `LARK_USER_OPEN_ID` | Yes | Target Feishu/Lark user for outgoing replies. |
| `BOCHA_API_KEY` | Recommended | Required when `internet_search` is invoked. |
| `ARKNIGHTS_DB_PATH` | No | SQLite database path. Defaults to `arknights.db`. |
| `ARKNIGHTS_RAG_MODEL` | No | Enables async embedding/indexing for saved chat history. Leave empty to disable. |

Supported `ARKNIGHTS_RAG_MODEL` values:

- `BAAI/bge-small-en-v1.5`
- `BAAI/bge-small-zh-v1.5`

When RAG is enabled, embeddings are cached under `models/fastembed`. If no local
model bundle exists there, `fastembed` may download model files on first use.

## Getting Started

```bash
# Clone the project
git clone <repo-url>
cd arknights

# Configure environment variables
cp .env.example .env

# Build
cargo build

# Run the Lark bot service
cargo run
```

After the service starts, the process opens the Lark websocket client and the
background timer loop. Send a text message to the configured Feishu/Lark app.

## Usage Guide

You can drive the agent entirely through chat. One practical workflow is to ask
the bot to create a scheduled task that refreshes the user profile for you.

### Initialize a daily user profile refresh task

Send a message like this to the configured Feishu/Lark bot:

```text
Please initialize a scheduled task for refreshing my user profile.
Requirements:
1. Run once every day at 4:00 AM with cron expression `0 0 4 * * *`.
2. Read the current user profile before making any changes.
3. Use recent chat history, memory search results, and any other available tools to decide whether the profile should be updated.
4. If the profile needs to be changed, overwrite it directly. If not, leave it unchanged.
5. Use `daily_user_profile_refresh` as the fixed task ID and keep it running long-term.
```

What happens next:

- The planner can select `timer`, `memory`, and any other relevant tool groups
  for this request.
- The task is persisted through `timer_insert` with a six-field cron expression,
  where daily 4:00 AM is `0 0 4 * * *`.
- Every due run executes the stored prompt through `Plan::new(...).execute()`,
  so the timer uses the same planning and tool-calling pipeline as an ordinary
  chat request.
- ReAct always injects `system`, `process_control`, and `memory` tools, so the
  scheduled run can inspect recent chat history and RAG-backed memory results.
- User profile reads and overwrites map to `memory_get_user_profile` and
  `memory_rewrite_user_profile`.

## Common Commands

```bash
cargo build
cargo run
cargo test
cargo clippy
```

## Architecture

### Runtime Flow

1. `src/main.rs` loads `.env`, initializes tracing, starts the background timer
   service, and opens the Lark websocket client.
2. `src/im/lark.rs` receives text messages, sends status emoji replies, and serializes plan
   execution with `PLAN_LOCK` while keeping the websocket receive loop responsive.
3. `src/agent/plan.rs` expands the user goal, prepends recent chat history from SQLite, and
   either answers directly or emits ordered subtasks with tool groups.
4. `src/agent/re_act.rs` executes each subtask with the requested tool groups plus default
   `system`, `process_control`, and `memory` tools.
5. `process_control_ask_user` can pause execution for a Lark reply, while `done` and `replan`
   let the ReAct loop either finish a subtask or request a new plan.
6. `src/timer/timer_service.rs` polls due timer tasks every 10 seconds, executes each saved
   prompt through `Plan::new(...).execute()`, and records the latest result for future runs.
7. When the planner reaches a final answer, the response is sent back through Lark and the
   user/assistant pair is written to chat history.
8. If `ARKNIGHTS_RAG_MODEL` is configured, chat history is indexed asynchronously into
   `chat_history_vec` using `sqlite-vec` and `fastembed`.

### Iteration Limits

- Planner loop: up to 20 turns
- ReAct loop: up to 20 turns per subtask

## Project Structure

```text
src/
├── main.rs                    # Service entry point
├── agent/
│   ├── plan.rs                # Plan -> ReAct -> Replan orchestration
│   ├── re_act.rs              # ReAct execution loop
│   └── mod.rs
├── dao/
│   ├── base_dao.rs            # Shared SQLite connection management
│   ├── chat_history_dao.rs    # Chat history table access
│   ├── chat_history_vec_dao.rs# sqlite-vec table access
│   └── mod.rs
├── im/
│   ├── lark.rs                # Feishu/Lark websocket and messaging
│   └── mod.rs
├── kv/
│   ├── kv_service.rs          # User profile and personalization KV access
│   └── mod.rs
├── llm/
│   ├── base_llm.rs            # Shared request/response types
│   ├── deep_seek.rs           # DeepSeek Chat Completions client
│   └── mod.rs
├── memory/
│   ├── chat_history_service.rs# History persistence and retrieval
│   ├── rag_embedder.rs        # Optional embedding generation
│   └── mod.rs
├── timer/
│   ├── timer_service.rs       # Background scheduler and task execution
│   └── mod.rs
├── tool/
│   ├── base_tool.rs           # `LlmTool` trait
│   ├── internet.rs            # `internet_search`, `internet_curl`
│   ├── memory.rs              # Memory search, history, and user profile tools
│   ├── process_control.rs     # `ask_user`, `done`, `replan`
│   ├── system.rs              # `system_date`
│   ├── timer/                 # `timer_get`, `timer_list`, CRUD timer tools
│   └── mod.rs                 # Tool registry
└── util/
    ├── http_utils.rs          # Shared HTTP client helpers
    └── mod.rs
```

## Built-in Tools

- `system_date`
- `internet_search`
- `internet_curl`
- `memory_search_tool`
- `memory_list_tool`
- `memory_get_user_profile`
- `memory_rewrite_user_profile`
- `process_control_ask_user`
- `process_control_done`
- `process_control_replan`
- `timer_get`
- `timer_list`
- `timer_insert`
- `timer_update`
- `timer_remove`

## Extending Tools

1. Add a new file under `src/tool/`.
2. Implement the `LlmTool` trait:

```rust
#[async_trait::async_trait]
impl LlmTool for MyTool {
    fn group_name(&self) -> &str {
        "my_group"
    }

    fn deep_seek_schema(&self) -> Function {
        // Return the tool schema exposed to the model.
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        // Execute the tool and return the result string.
    }
}
```

3. Register the tool in `src/tool/mod.rs`.
4. If the tool belongs to a new group, make sure planner or caller code includes that group.

## Logging

- Console logs are enabled by default.
- File logs are written to `logs/arknights.log`.

## License

MIT
