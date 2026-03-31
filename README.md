# Arknights

Arknights is a Rust 2024 agent service that runs behind a Feishu/Lark bot.
It receives text messages from Lark, routes slash commands and ordinary chat
separately, expands and plans work with DeepSeek, executes subtasks through a
Plan -> ReAct -> Replan pipeline, can also execute scheduled timer prompts
through the same pipeline, and sends final text or files back through Lark.

Chat history is stored in SQLite, user profile state and personal rewrite style
are stored in the local KV store, and chat history can optionally be indexed
and searched with `sqlite-vec` and `fastembed`.

## Features

- Plan -> ReAct -> Replan workflow driven by DeepSeek chat completions
- Feishu/Lark websocket integration for inbound text, outbound replies, and
  status emoji updates
- Slash command support for `/set_personal`
- Pluggable async tool system for `system`, `internet`, `memory`,
  `process_control`, `timer`, and `schedule`
- SQLite-backed chat history, timer tasks, and schedule events
- KV-backed user profile and personal rewrite style storage
- Optional RAG indexing and retrieval with `sqlite-vec` and `fastembed`
- Background timer scheduler that replays saved prompts through the same agent
  pipeline and can suppress redundant reminder pushes

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
| `ARKNIGHTS_RAG_MODEL` | No | Enables async embedding and vector search for saved chat history. Leave empty to disable. |
| `BASH_TOOL_ENABLE` | No | High-risk switch. Enables `system_bash` and gives the agent read/write access to all files the service process can access. Defaults to `false`. |

Supported `ARKNIGHTS_RAG_MODEL` values:

- `BAAI/bge-small-en-v1.5`
- `BAAI/bge-small-zh-v1.5`

When RAG is enabled, embeddings are cached under `models/fastembed`. If no
local model bundle exists there, `fastembed` may download model files on first
use.

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

After the service starts, the process initializes the global IM client, starts
the background timer loop, and opens the Lark websocket client. Logs are also
written to `logs/arknights.log`.

## Chat Usage

Regular text messages go through the agent pipeline. Slash commands are handled
separately before the planner runs.

### Set the personal rewrite style

Send a slash command like this:

```text
/set_personal Speak like Amiya, but keep every factual detail unchanged.
```

The stored style is later consumed by `src/agent/personal.rs` to rewrite final
assistant messages before they are sent back through Lark.

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

- The planner can select `timer`, `memory`, and any other relevant tool groups.
- The task is persisted through `timer_insert` with a six-field cron
  expression, where daily 4:00 AM is `0 0 4 * * *`.
- Every due run executes the stored prompt through `Plan::new(...).execute()`,
  so the timer uses the same planning and tool-calling pipeline as an ordinary
  chat request.
- ReAct always injects `system`, `process_control`, and `memory` tools, so the
  scheduled run can inspect recent chat history and RAG-backed memory results.
- User profile reads and overwrites map to `memory_get_user_profile` and
  `memory_rewrite_user_profile`.

The planner can also choose the `schedule` tool group to create, list, search,
update, and remove user schedule events stored in SQLite.

## Common Commands

```bash
cargo build
cargo run
cargo test
cargo clippy
```

`cargo test` uses `src/test_support.rs` to inject temporary defaults for the
required Lark variables and database path, so most tests do not require a real
DeepSeek or Lark credential set.

## Architecture

### Runtime Flow

1. `src/main.rs` loads `.env`, initializes tracing, initializes the global IM
   client, starts the background timer service, and reconnects the Lark
   websocket client on exit.
2. `src/im/lark.rs` receives text messages, sends status emoji replies, routes
   `ask_user` replies, handles slash commands, and starts planner tasks behind
   `PLAN_LOCK` so only one plan pipeline runs at a time.
3. `src/command/command.rs` handles slash commands such as `/set_personal`,
   which updates the personal rewrite role stored in KV.
4. `src/agent/plan.rs` expands the user goal, prepends recent chat history from
   SQLite plus the stored user profile, and either answers directly or emits
   ordered subtasks with tool groups.
5. `src/agent/re_act.rs` executes each subtask with the requested tool groups
   plus default `system`, `process_control`, and `memory` tools.
6. `src/timer/timer_service.rs` polls due timer tasks every 10 seconds,
   executes each saved prompt through `Plan::new(...).execute()`, and records
   the latest result plus the next trigger time.
7. Final answers can be filtered by `src/agent/notify_check.rs` for timer runs,
   rewritten by `src/agent/personal.rs`, and then sent back through Lark
   together with any generated files.
8. If `ARKNIGHTS_RAG_MODEL` is configured, chat history is indexed
   asynchronously into `chat_history_vec` using `sqlite-vec` and `fastembed`.

### Iteration Limits

- Planner loop: up to 20 turns
- ReAct loop: up to 20 turns per subtask

### Module Structure

- `src/main.rs` — Service entry point. Initializes tracing, IM, timers, and the
  reconnecting Lark websocket loop.
- `src/agent/` — Agent orchestration and response post-processing.
  - `plan.rs` — Plan -> ReAct -> Replan orchestration.
  - `re_act.rs` — ReAct execution loop.
  - `notify_check.rs` — Timer reminder suppression decisions.
  - `personal.rs` — Final-message style rewriting using stored role text.
- `src/command/` — Slash command entrypoints such as `/set_personal`.
- `src/im/` — Feishu/Lark websocket intake, outbound messaging, emoji replies,
  and `ask_user` coordination.
- `src/kv/` — KV-backed personalization and user-profile storage.
- `src/llm/` — Shared LLM request types and the DeepSeek client.
- `src/memory/` — Chat-history persistence plus optional embedding and vector
  search via `fastembed`.
- `src/schedule/` — Schedule-event application service built on top of SQLite.
- `src/timer/` — Background scheduler that persists timer tasks and executes
  them through the same planning pipeline.
- `src/dao/` — SQLite DAOs for chat history, vectors, KV, timers, and
  schedules.
  - `timer/` — Timer task persistence.
  - `schedule/` — Schedule event persistence.
- `src/tool/` — Pluggable tool system.
  - `base_tool.rs` — `LlmTool` async trait.
  - `mod.rs` — Static `TOOL_REGISTRY` mapping tool names to implementations and
    filtering by tool group.
  - `internet.rs` — `internet_search` and `internet_curl`.
  - `memory.rs` — Memory search, recent history listing, and user profile
    tools.
  - `process_control.rs` — `process_control_ask_user`,
    `process_control_done`, and `process_control_replan`.
  - `system/` — `system_date`, `system_bash`, and bash runtime helpers.
  - `timer/` — `timer_get`, `timer_list`, `timer_insert`, `timer_update`, and
    `timer_remove`.
  - `schedule/` — `schedule_insert`, `schedule_get`, `schedule_list`,
    `schedule_search`, `schedule_list_by_tag`, `schedule_update`, and
    `schedule_remove`.
- `src/util/` — Shared HTTP utilities with 60 second timeouts and explicit HTTP
  error propagation.

## Built-in Tools

- `system_date`
- `system_bash`
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
- `schedule_insert`
- `schedule_get`
- `schedule_list`
- `schedule_search`
- `schedule_list_by_tag`
- `schedule_update`
- `schedule_remove`

## Extending Tools

1. Add a new implementation under `src/tool/`.
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
4. If the tool belongs to a new group, make sure planner or caller code
   includes that group.

## Logging

- Console logs are enabled by default.
- File logs are written to `logs/arknights.log`.

## License

MIT
