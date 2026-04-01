# Arknights

Arknights 是一个基于 Rust 2024 的智能体服务，运行在飞书/Lark 机器人之后。
它接收来自 Lark 的文本消息，将斜杠命令与普通聊天分别路由，使用
DeepSeek 扩展并规划任务，通过 Plan -> ReAct -> Replan 流水线执行子任务，
也可以通过同一套流水线执行定时任务提示，并将最终文本或文件发送回
Lark。

聊天历史存储在 SQLite 中，用户画像状态和个人化改写风格存储在本地 KV
中，聊天历史还可以通过 `sqlite-vec` 和 `fastembed` 进行可选的索引与搜索。

## 功能特性

- 基于 DeepSeek chat completions 驱动的 Plan -> ReAct -> Replan 工作流
- 飞书/Lark websocket 集成，用于接收文本消息、发送回复与状态表情更新
- 支持 `/set_personal` 斜杠命令
- 可插拔的异步工具系统，覆盖 `system`、`internet`、`memory`、
  `process_control`、`timer`、`schedule` 和 `browser`
- 基于 SQLite 的聊天历史、定时任务与日程事件存储
- 基于 KV 的用户画像与个人化改写风格存储
- 通过 `sqlite-vec` 和 `fastembed` 提供可选的 RAG 建索引与检索能力
- 后台定时调度器会通过同一套智能体流水线重放已保存的提示词，并可抑制
  冗余的提醒推送，同时维护 `remaining_runs` 和最近一次执行结果
- 基于 `chromiumoxide` 的有状态浏览器自动化；当规划器选择 `browser`
  工具组时，每次 ReAct 执行会共享同一个页面会话

## 前置要求

- 支持 Rust 2024 edition 的 Rust 工具链
- 已启用 websocket 消息投递的飞书/Lark 应用
- DeepSeek API Key
- 如果要使用互联网搜索工具，需要 Bocha API Key
- 如果要运行 `browser_*` 工具，宿主环境必须能够启动 Chromium 内核浏览器

## 环境变量

将 `.env.example` 复制为 `.env`，并填写所需的配置值。

| 变量 | 是否必需 | 说明 |
| --- | --- | --- |
| `DEEPSEEK_API_KEY` | 是 | 由 `src/llm/deep_seek.rs` 用于规划器与 ReAct 模型调用。 |
| `LARK_APP_ID` | 是 | `src/im/lark.rs` 使用的飞书/Lark 应用 ID。 |
| `LARK_APP_SECRET` | 是 | 用于获取 tenant access token 的飞书/Lark 应用密钥。 |
| `LARK_USER_OPEN_ID` | 是 | 接收外发回复的目标飞书/Lark 用户。 |
| `BOCHA_API_KEY` | 推荐 | 调用 `internet_search` 时必需。 |
| `ARKNIGHTS_DB_PATH` | 否 | SQLite 数据库路径，默认值为 `arknights.db`。 |
| `ARKNIGHTS_RAG_MODEL` | 否 | 为已保存聊天历史启用异步 embedding 与向量搜索；留空则禁用。 |
| `BASH_TOOL_ENABLE` | 否 | 高风险开关。启用 `system_bash`，并赋予智能体访问服务进程可访问全部文件的读写权限。默认值为 `false`。 |

支持的 `ARKNIGHTS_RAG_MODEL` 取值：

- `BAAI/bge-small-en-v1.5`
- `BAAI/bge-small-zh-v1.5`

启用 RAG 后，embedding 会缓存在 `models/fastembed` 下。如果该目录下没有
本地模型包，`fastembed` 可能会在首次使用时下载模型文件。

## 快速开始

```bash
# 克隆项目
git clone <repo-url>
cd arknights

# 配置环境变量
cp .env.example .env

# 构建
cargo build

# 运行 Lark 机器人服务
cargo run
```

服务启动后，进程会初始化全局 IM 客户端、启动后台定时循环，并打开 Lark
websocket 客户端。按天滚动的日志也会写入 `logs/` 目录，基础文件名为
`arknights.log`。

## 聊天使用方式

普通文本消息会进入智能体流水线。斜杠命令会在规划器运行前单独处理。

### 设置个人化改写风格

发送如下斜杠命令：

```text
/set_personal 像阿米娅一样说话，但保留所有事实细节不变。
```

保存后的风格随后会被 `src/agent/personal.rs` 用来改写最终的助手消息，
然后再发送回飞书/Lark。

### 通过追问基本信息初始化用户画像

向配置好的飞书/Lark 机器人发送如下消息：

```text
通过询问我有关的基本信息，初始化我的画像，例如：称呼，职业，地理位置等。
```

后续会发生的事情：

- 规划器会把它当作普通聊天任务处理，并可通过 `process_control_ask_user`
  在 Lark 中继续追问补充信息。
- 在收集到足够事实后，智能体可以通过 `memory_rewrite_user_profile`
  写入初始用户画像。
- `memory_rewrite_user_profile` 当前会拒绝超过 1000 个字符的画像
  Markdown，因此最终保存的画像需要控制在这个限制内。

### 初始化一个每日用户画像刷新任务

向配置好的飞书/Lark 机器人发送如下消息：

```text
请帮我初始化一个用于刷新用户画像的定时任务。
要求：
1. 每天凌晨 4:00 执行一次，cron 表达式为 `0 0 4 * * *`。
2. 在做任何修改前先读取当前用户画像。
3. 使用最近聊天历史、记忆搜索结果以及其他可用工具来判断是否需要更新画像。
4. 如果画像需要变更，直接覆盖写入；如果不需要，则保持不变。
5. 通过 `memory_rewrite_user_profile` 写回时，最终画像 Markdown 需要
   保持在当前 1000 个字符的限制内。
6. 使用 `daily_user_profile_refresh` 作为固定任务 ID，并长期保持运行。
```

后续会发生的事情：

- 规划器可以选择 `timer`、`memory` 以及其他相关工具组。
- 该任务会通过 `timer_insert` 持久化为六段式 cron 表达式，其中每天凌晨
  4:00 对应 `0 0 4 * * *`。
- 每次触发时，都会通过 `Plan::new(...).execute()` 执行已保存的提示词，
  因此定时任务与普通聊天请求使用的是同一套规划与工具调用流水线。
- ReAct 总会注入 `system`、`process_control` 和 `memory` 工具，因此
  定时执行可以检查最近聊天历史以及基于 RAG 的记忆检索结果。
- 用户画像的读取与覆盖写入分别映射到 `memory_get_user_profile` 和
  `memory_rewrite_user_profile`。
- `memory_rewrite_user_profile` 会拒绝超过 1000 个字符的写入。

规划器还可以选择 `schedule` 工具组，用于创建、获取、列出、搜索、
按标签列出、更新和删除保存在 SQLite 中的用户日程事件。

通过 `timer_insert` 或 `timer_update` 创建的定时任务需要提供 `id`、
`prompt`、`cron_expr` 和 `remaining_runs`。只有 `remaining_runs > 0`
时任务才会被视为活跃；把它设为 `0` 会暂停任务，但不会删除已保存的
提示词或执行历史。

`schedule` 工具使用 RFC3339 时间字符串。服务层会把时间规范化为本地时区、
毫秒精度的时间戳，并拒绝 `end_time` 早于 `start_time` 的输入。

## 浏览器工具

当任务需要真实页面交互而不是简单 HTTP 抓取时，规划器可以选择 `browser`
工具组。

- 单次 ReAct 执行中的所有 `browser_*` 调用会共享同一个浏览器会话和页面。
- `browser_snapshot` 会返回当前页面结构和 `element_id` 值。发生导航或
  `element_id_stale` 错误后，智能体必须重新获取快照，才能继续执行依赖
  元素 ID 的操作。
- `browser_screenshot` 会把 PNG 文件写入仓库根目录下的
  `.cache/browser/<scope-id>/`，并返回实际保存路径。

返回给用户的文件会通过 Lark 文件上传接口发送。当前实现会在上传前拒绝
大于 20MB 的文件。

## 常用命令

```bash
cargo build
cargo run
cargo test
cargo clippy
```

`cargo test` 会使用 `src/test_support.rs` 注入所需 Lark 变量和数据库路径的
临时默认值，因此大多数测试不需要真实的 DeepSeek 或 Lark 凭据。

## 架构说明

### 运行时流程

1. `src/main.rs` 加载 `.env`，初始化 tracing、全局 IM 客户端，启动后台
   定时服务，并在退出时重新连接 Lark websocket 客户端。
2. `src/im/lark.rs` 接收文本消息，发送状态表情回复，路由 `ask_user` 回复，
   处理斜杠命令，并在 `PLAN_LOCK` 后启动规划任务，因此同一时间只会有
   一个 plan 流水线运行。
3. `src/command/command.rs` 处理 `/set_personal` 等斜杠命令，它会更新保存在
   KV 中的个人化改写角色文本。
4. `src/agent/plan.rs` 扩展用户目标，拼接来自 SQLite 的最近聊天历史和已
   存储的用户画像，然后要么直接回答，要么输出带工具组的有序子任务。
5. `src/agent/re_act.rs` 用请求的工具组加上默认的 `system`、
   `process_control` 和 `memory` 工具执行每个子任务。当启用 `browser`
   工具组时，整个 ReAct 执行会共享一个浏览器会话与页面。
6. `process_control_ask_user` 可以在普通聊天执行中暂停流程以等待用户通过
   Lark 回复，超时时间为 5 分钟；定时任务触发的执行不允许向用户追问。
7. `src/timer/timer_service.rs` 每 10 秒轮询一次到期的定时任务，通过
   `Plan::new(...).execute()` 执行每条已保存的提示词，递减
   `remaining_runs`，并记录最新结果及下次触发时间。
8. 最终答案对于定时任务可由 `src/agent/notify_check.rs` 进行过滤，再由
   `src/agent/personal.rs` 改写，然后连同生成的文件一起发回 Lark。
9. 如果配置了 `ARKNIGHTS_RAG_MODEL`，聊天历史会通过 `sqlite-vec` 和
   `fastembed` 异步索引到 `chat_history_vec` 中。

### 迭代限制

- Planner 循环：最多 20 轮
- ReAct 循环：每个子任务最多 20 轮

### 模块结构

- `src/main.rs` — 服务入口。初始化 tracing、IM、timer，以及可重连的
  Lark websocket 循环。
- `src/agent/` — 智能体编排与响应后处理。
  - `plan.rs` — Plan -> ReAct -> Replan 编排。
  - `re_act.rs` — ReAct 执行循环。
  - `notify_check.rs` — 定时提醒抑制判断。
  - `personal.rs` — 使用已保存角色文本改写最终消息的逻辑。
- `src/command/` — `/set_personal` 等斜杠命令入口。
- `src/im/` — 飞书/Lark websocket 接入、消息发送、表情回复与 `ask_user`
  协调逻辑。
- `src/kv/` — 基于 KV 的个性化与用户画像存储。
- `src/llm/` — 共享的 LLM 请求类型和 DeepSeek 客户端。
- `src/memory/` — 聊天历史持久化，以及基于 `fastembed` 的可选 embedding 与
  向量搜索。
- `src/schedule/` — 基于 SQLite 的日程事件应用服务。
- `src/timer/` — 持久化定时任务并通过同一套计划流水线执行它们的后台调度器。
- `src/dao/` — 聊天历史、向量、KV、timer 与 schedule 的 SQLite DAO。
  - `timer/` — 定时任务持久化。
  - `schedule/` — 日程事件持久化。
- `src/tool/` — 可插拔工具系统。
  - `base_tool.rs` — `LlmTool` 异步 trait。
  - `mod.rs` — 静态 `TOOL_REGISTRY`，负责把工具名映射到实现并按工具组过滤。
  - `browser/` — 基于 `chromiumoxide` 的有状态浏览器工具，包含页面导航、DOM
    快照、元素操作、文本提取、等待、滚动和截图。
  - `internet.rs` — `internet_search` 和 `internet_curl`。
  - `memory.rs` — 记忆搜索、最近历史列表和用户画像工具。
  - `process_control.rs` — `process_control_ask_user`、
    `process_control_done` 和 `process_control_replan`。
  - `system/` — `system_date`、`system_bash` 和 bash 运行时辅助工具。
  - `timer/` — `timer_get`、`timer_list`、`timer_insert`、`timer_update` 和
    `timer_remove`。
  - `schedule/` — `schedule_insert`、`schedule_get`、`schedule_list`、
    `schedule_search`、`schedule_list_by_tag`、`schedule_update` 和
    `schedule_remove`。
- `src/util/` — 共享 HTTP 工具，统一使用 60 秒超时并显式传播 HTTP 错误。

## 内置工具

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
- `browser_navigate`
- `browser_snapshot`
- `browser_screenshot`
- `browser_click`
- `browser_fill`
- `browser_get_text`
- `browser_scroll`
- `browser_wait_text`
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

## 扩展工具

1. 在 `src/tool/` 下添加新的实现。
2. 实现 `LlmTool` trait：

```rust
#[async_trait::async_trait]
impl LlmTool for MyTool {
    fn group_name(&self) -> &str {
        "my_group"
    }

    fn deep_seek_schema(&self) -> Function {
        // 返回暴露给模型的工具 schema。
    }

    async fn deep_seek_call(&self, tool_call: &ToolCall) -> String {
        // 执行工具并返回结果字符串。
    }
}
```

3. 在 `src/tool/mod.rs` 中注册该工具。
4. 如果该工具属于新的工具组，确保规划器或调用方代码会包含这个组。

## 日志

- 默认启用控制台日志。
- 按天滚动的文件日志会写入 `logs/` 目录，基础文件名为 `arknights.log`。
- `chromiumoxide` 那条噪声较大的 invalid websocket-message warning 会被
  有意抑制；其他浏览器 warning 和 error 仍会出现在日志中。

## 许可证

MIT
