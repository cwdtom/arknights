# Arknights

A ReAct (Reasoning + Acting) agent framework powered by DeepSeek LLM, built with Rust.

## Features

- **ReAct Loop** — Alternates between reasoning and acting; the LLM autonomously decides when to call tools and when to return a final answer
- **Pluggable Tool System** — Extend tools via the `LlmTool` trait with async execution support
- **DeepSeek API Integration** — Uses Chat Completions API with function calling

## Getting Started

### Prerequisites

- Rust 1.80+ (edition 2024)
- DeepSeek API Key

### Installation & Run

```bash
# Clone the project
git clone <repo-url>
cd arknights

# Configure environment variables
cp .env.example .env
# Edit .env and fill in your DEEPSEEK_API_KEY

# Run
cargo run
```

## Project Structure

```
src/
├── main.rs              # Entry point, initializes runtime and agent
├── agent/
│   └── re_act.rs        # ReAct loop implementation
├── llm/
│   └── deep_seek.rs     # DeepSeek API client
└── tool/
    ├── base_tool.rs     # LlmTool trait definition
    ├── mod.rs           # Tool registry (TOOL_REGISTRY)
    └── system.rs        # Built-in tool: DateTool
```

## Extending Tools

1. Create a new file under `src/tool/`, define a struct and implement the `LlmTool` trait:

```rust
#[async_trait::async_trait]
impl LlmTool for MyTool {
    fn deep_seek_schema(&self) -> Function {
        // Return the tool's JSON Schema for LLM to recognize and invoke
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        // Execute tool logic and return the result as a string
    }
}
```

2. Register the tool in `TOOL_REGISTRY` in `src/tool/mod.rs`.

## How It Works

```
User Input → ReAct Agent → DeepSeek LLM
                 ↑              ↓
          Tool Result ← Tool Call (function call)
                 ↓
          LLM sets is_done → Return Final Answer
```

The agent runs up to 100 iterations. In each turn, the LLM can either call a tool to gather information or set `is_done: true` to return the final result.

## License

MIT
