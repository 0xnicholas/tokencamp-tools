# Codex 兼容性与 tool loop 可行性（ticket #5）

- 状态：research 完成，2026-11-01
- 被测版本：Codex CLI `0.132.0`（npm `@openai/codex`，macOS x86_64 / Intel）
- tokencamp 基线：`tokencamp-pro` `25db44b`（2026-10-31）；客户文档 `tokencamp-www` `640d204`
- 范围：`tcamp` v1 是否把 Codex 纳入；纯客户端，不改 `tokencamp-pro`
- 结论：**blocked（v1 不纳入）**，不是配置少一个字段

## 0. 结论（Verdict）

**blocked。** Codex `0.132.0` 只支持 Responses；`wire_api = "chat"` 在配置解析阶段直接
报错（§2）。Responses 路径又与 tokencamp `/v1/responses` 的 stateless 子集有两处硬冲突：

1. **第一轮请求就失败。** Codex 每个 Responses 请求都序列化 `include`：没有 reasoning
   时是 `[]`，有 reasoning 时是 `["reasoning.encrypted_content"]`。`include` 在
   `ResponsesApiRequest` 里是无 `skip_serializing_if` 的 `Vec<String>`，空数组也会上线。
   tokencamp 的 `validate()` 对 `include.is_some()` 一律拒绝，`param = "include"`，所以
   请求在鉴权前就是 400 `invalid_request`。
2. **tool loop 第二轮失败。** Codex 在下一轮 `input` 里回放 `function_call` 和
   `function_call_output` 两种 item；tokencamp 的 `validate_input()` 只接受 message
   item，遇到第一个 `function_call` 就 400 `param = "input"`。只放行
   `function_call_output` 也不够，必须同时支持 `function_call`。
3. **reasoning 是条件性第三冲突。** 对 tokencamp 的 `auto` / catalog id，Codex 把模型
   当作非 reasoning 模型，发 `reasoning: null`（不触发）；但如果模型名命中 Codex 自带
   catalog（例如 `gpt-5.5`）且用户设了 `model_reasoning_effort`，Codex 会发
   `reasoning: {"effort": "high"}` + `include: ["reasoning.encrypted_content"]`。
   tokencamp 对非空 `reasoning` 也拒绝，但 `include` 先检查，实际先报 `include`。

这是协议子集边界，不是 `tcamp` 写错配置。v1 应明确不支持 Codex：`tcamp codex` 不做，
或在 `doctor`/帮助里说明“Codex CLI 0.132.x 与 tokencamp Responses 子集不兼容”。最小
pro 改动见 §6，仅记录，不在 v1 实施。

## 1. `model_providers.<id>` 的形状

### 1.1 文件与合并

| 项目 | 事实 | 依据 |
| --- | --- | --- |
| 用户配置 | `$CODEX_HOME/config.toml`；未设 `CODEX_HOME` 时默认 `~/.codex/config.toml` | 【实测】`codex --help` 的 `-c` 说明；`codex doctor --json` 显示 `CODEX_HOME` 与 `config.toml` 路径 |
| 合并层级 | MDM > system managed > session flags（`-c`）> user config；provider map 按 key 合并 | 【源码】[`config/src/loader/README.md`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/config/src/loader/README.md) |
| CLI 覆盖 | `-c key=value` 用 dotted path 覆盖，value 按 TOML 解析 | 【实测】`codex --help`；本次用 `-c model_providers.tokencamp.wire_api="chat"` 复现错误 |
| 命名 profile | `[profiles.<name>]` 可设 `model` / `model_provider`；`codex -p <name>` 选择 | 【源码】[`config/src/profile_toml.rs:24-32`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/config/src/profile_toml.rs#L24-L32)；【实测】`codex exec -p tokencamp` 成功 |
| 严格校验 | `--strict-config` 让未识别字段报错；`--ignore-user-config` 不读用户 config | 【实测】`codex --help` / `codex exec --help` |

`model_providers` 是 `HashMap<String, ModelProviderInfo>`，ID 不能覆盖内置
`openai`、`ollama`、`lmstudio`、`amazon-bedrock`、`ollama-chat` 等保留名
（[`config_toml.rs:273-274`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/config/src/config_toml.rs#L273-L274)、
[`config_toml.rs:933-953`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/config/src/config_toml.rs#L933-L953)）。
`tokencamp` 不是保留名，可用。

### 1.2 `ModelProviderInfo` 字段

源码：[`model-provider-info/src/lib.rs:84-136`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/model-provider-info/src/lib.rs#L84-L136)。

| TOML key | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `name` | string | 是（非空） | 显示名；空字符串会被 `validate_model_providers` 拒绝 |
| `base_url` | string | 否 | provider 的 OpenAI-compatible API root；Codex 在其后追加 `responses`，所以必须含 `/v1` |
| `env_key` | string | 否 | 环境变量名；Codex 读取后发 `Authorization: Bearer <value>`。与 `auth` / `aws` 不能组合；推荐只用它 |
| `env_key_instructions` | string | 否 | key 缺失时的提示文案 |
| `experimental_bearer_token` | string | 否 | 直接把 token 写在 config；与 `auth` / `aws` 不能组合，和 `env_key` 同时存在时的优先级未测，不推荐同时写 |
| `auth` | table | 否 | command-backed bearer token；与 `env_key` 互斥 |
| `aws` | table | 否 | AWS SigV4；仅 `amazon-bedrock` 可用 |
| `wire_api` | string | 否 | 只有 `"responses"`；默认就是 `responses`。`"chat"` 直接报错（§2） |
| `query_params` | table | 否 | 追加到 base URL 的 query |
| `http_headers` | table | 否 | 固定 header |
| `env_http_headers` | table | 否 | header 值来自环境变量 |
| `request_max_retries` | integer | 否 | HTTP 重试次数 |
| `stream_max_retries` | integer | 否 | 流断线重连次数 |
| `stream_idle_timeout_ms` | integer | 否 | 流空闲超时 |
| `websocket_connect_timeout_ms` | integer | 否 | WebSocket 连接超时 |
| `requires_openai_auth` | bool | 否 | 默认 `false`；`false` 时跳过 ChatGPT 登录，API key 来自 `env_key` |
| `supports_websockets` | bool | 否 | 默认 `false`；tokencamp 没有 Responses WebSocket，保持 false |

### 1.3 最小配置（可成功产生 Codex 请求，但会被 tokencamp 拒绝）

```toml
# $CODEX_HOME/config.toml（默认 ~/.codex/config.toml）
model = "auto"                    # 或 catalog id，如 deepseek/deepseek-v4-pro
model_provider = "tokencamp"

[model_providers.tokencamp]
name = "tokencamp"
base_url = "https://api.tokencamp.com/v1"
env_key = "TOKENCAMP_API_KEY"
wire_api = "responses"
requires_openai_auth = false
```

要点：

- `base_url` 必须含 `/v1`。实测把 base 改成 `http://127.0.0.1:8899`（无 `/v1`）后，Codex
  请求的是 `POST /responses`，不是 tokencamp 的 `/v1/responses`。
- `requires_openai_auth = false` 是默认值；显式写出来便于审阅。它让 Codex 不要求
  ChatGPT 登录，直接用 `env_key` 的环境变量发 Bearer。实测 `codex doctor --json` 显示
  `model provider requires OpenAI auth: false`、`provider auth env var: TOKENCAMP_API_KEY (present)`。
- `name` 不能省；源码的 `validate_model_providers` 对空 `name` 报
  `model_providers.tokencamp: provider name must not be empty`。
- 不要设 `supports_websockets = true`；tokencamp 没有 Responses WebSocket 端点。
- 若不想覆盖用户顶层 `model`，可改用 profile，启动时加 `-p tokencamp`：

```toml
[profiles.tokencamp]
model = "auto"
model_provider = "tokencamp"
```

实测 `codex exec -p tokencamp` 的 header 显示 `model: auto`、`provider: tokencamp`，请求
正常发出。

## 2. Codex 是否支持 `wire_api = "chat"`？

**不支持（0.132.0）。** `WireApi` 枚举只剩 `Responses`；`"chat"` 的反序列化分支直接返回
错误：

- 【源码】[`model-provider-info/src/lib.rs:49-78`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/model-provider-info/src/lib.rs#L49-L78)
- 【源码】同一个错误的单元测试：
  [`model_provider_info_tests.rs:110-122`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/model-provider-info/src/model_provider_info_tests.rs#L110-L122)
- 【实测】把配置改成 `wire_api = "chat"`：

```text
Error loading config.toml: `wire_api = "chat"` is no longer supported.
How to fix: set `wire_api = "responses"` in your provider config.
More info: https://github.com/openai/codex/discussions/7782
in `model_providers.tokencamp.wire_api`
EXIT: 1
```

因此 tokencamp `/v1/chat/completions` 这条最自然的兼容路径对 Codex 0.132.x 不可用。
tokencamp 的 chat 端点本身是 OpenAI-compatible，支持 tool calls、流式和 reasoning
参数透传；如果未来 Codex 恢复 chat wire（或 `tcamp` 面向旧版 Codex），它是更合适的
兼容面，但当前版本没有配置开关能走到它。`--oss` 也走 Responses，不是逃生口。

## 3. 真实 Codex 会话观察到的 wire body

### 3.1 实验方式

- `CODEX_HOME` 指向临时目录，未读取/修改用户真实 `~/.codex`。
- 本地 Python mock 绑定 `127.0.0.1:8899`，记录 `POST /v1/responses` 的 header/body，返回
  OpenAI Responses 形状的 SSE；实测使用纯文本和 `function_call` 两种响应。`bare error`
  事件只做源码对照（§3.5），未完成端到端实验。
- 配置就是 §1.3 的版本，`TOKENCAMP_API_KEY=dummy-key`。
- 先跑 `codex exec --skip-git-repo-check --ephemeral -s read-only "Say hi."`，再跑工具
  触发 prompt，观察第二轮请求。

### 3.2 第一轮请求（关键字段）

实际 body（省略 20 KB 的 `instructions` 和 `input` 里的 skill 文本）：

```json
{
  "model": "auto",
  "instructions": "<Codex system prompt, ~20 KB>",
  "input": [
    {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "..."}]},
    {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "<environment_context>..."}]},
    {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Say hi."}]}
  ],
  "tools": [
    {"type": "function", "name": "exec_command", "description": "...", "strict": false, "parameters": {"...": "..."}}
  ],
  "tool_choice": "auto",
  "parallel_tool_calls": false,
  "reasoning": null,
  "store": false,
  "stream": true,
  "include": [],
  "prompt_cache_key": "<session-uuid>",
  "client_metadata": {"x-codex-installation-id": "<uuid>"}
}
```

Headers 关键项：`Authorization: Bearer <redacted>`、`Content-Type: application/json`、
`Accept: text/event-stream`、`originator: codex_exec`、`user-agent: codex_exec/0.132.0 ...`。

Codex 的请求构造源码：
[`core/src/client.rs:712-764`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/core/src/client.rs#L712-L764)；
`ResponsesApiRequest` 的字段：
[`codex-api/src/common.rs:169-190`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/codex-api/src/common.rs#L169-L190)。

### 3.3 第二轮请求（tool loop）

mock 第一轮返回一个 `function_call`（`name = "exec_command"`, `arguments = {"cmd":"echo hello-from-mock"}`），
Codex 执行命令后，第二轮 `input` 末尾变成：

```json
[
  {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Run echo hello-from-mock..."}]},
  {"type": "function_call", "name": "exec_command", "arguments": "{\"cmd\":\"echo hello-from-mock\"}", "call_id": "call_mock_1"},
  {"type": "function_call_output", "call_id": "call_mock_1", "output": "Chunk ID: ...\nWall time: ...\nProcess exited with code 0\n...\nOutput:\nhello-from-mock\n"}
]
```

这证明 Codex 的工具结果回放**正好**使用 `function_call_output`，并且同一轮还会回放
`function_call` 自身。`ResponseItem` 的 serde tag 是 `type` + `snake_case`，所以这两个
wire 名称是稳定的：
[`protocol/src/models.rs:750-847`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/protocol/src/models.rs#L750-L847)。

### 3.4 逐字段对照 tokencamp `/v1/responses`

tokencamp 的请求类型：
[`crates/core/src/responses.rs:61-84`](tokencamp-pro:crates/core/src/responses.rs:61-84)；
validate：
[`crates/gateway/src/routes/responses.rs:187-232`](tokencamp-pro:crates/gateway/src/routes/responses.rs:187-232)；
input item 校验：
[`crates/gateway/src/routes/responses.rs:280-327`](tokencamp-pro:crates/gateway/src/routes/responses.rs:280-327)。

| Codex 发送 | 示例 | tokencamp 行为 | 判定 |
| --- | --- | --- | --- |
| `model` | `auto` / `deepseek/deepseek-v4-pro` | 接受；catalog id / auto / preset / family alias | 可用 |
| `instructions` + message items | 20 KB system prompt；`developer` / `user` 文本 | 接受；`instructions` 映射为 system，role 允许 developer，text parts 允许 | 可用 |
| `store` | `false` | `Some(false)` 接受；只有 `true` 拒绝 | 可用 |
| `stream` | `true` | 接受；按模型 streaming capability 校验 | 可用 |
| `include` | `[]` | `Option<Vec<String>>` 反序列化为 `Some([])`；`include.is_some()` 一律 400，`param = "include"` | **blocked（第一轮）** |
| `reasoning` | `null` | `None`，接受 | 可用 |
| `reasoning` | `{"effort":"high"}` | `Some(...)`，400，`param = "reasoning"` | 条件性冲突 |
| `tools` | 仅 `type: "function"` | 接受；非 function 工具才拒绝 | 可用 |
| `tool_choice` | `"auto"` | 接受 | 可用 |
| `parallel_tool_calls` | `false` | 接受，并作为 chat 参数透传 | 可用 |
| `prompt_cache_key` | session UUID | 未建模，落 `extra`，上游透传 | 未验证上游是否接受 |
| `client_metadata` | `{x-codex-installation-id: ...}` | 未建模，落 `extra`，上游透传 | 未验证上游是否接受 |
| `previous_response_id` | 不发送 | 不触发 | n/a |
| input item `function_call` | 第二轮回放 | `validate_input` 只允许 message；400，`param = "input"` | **blocked（第二轮）** |
| input item `function_call_output` | 第二轮回放 | 同上；400，`param = "input"` | **blocked（第二轮）** |

关键点：

- `include` 是 `Option<Vec<String>>`，空数组不是 `None`。我用 `tokencamp-core` 的
  `serde_json::from_str::<ResponsesRequest>` 做运行时确认，输出 `include=Some([])`。
  所以不是“只有非空 include 才拒绝”，空数组同样拒绝。
- `validate()` 在 `direct::handle` 之前执行，注释明确写着 “before any lifecycle work —
  unauthenticated and unlogged”（[`responses.rs:121-126`](tokencamp-pro:crates/gateway/src/routes/responses.rs:121-126)）。
  因此即使 key 有效，也会先 400，不是鉴权问题。
- `validate_input()` 对 `item.kind` 为 `None` 或 `"message"` 放行；`"item_reference"`
  单独拒绝；其余一律 `unsupported input item type ...: only message items are served`。
  第二轮的第一个非 message item 是 `function_call`，所以错误会先报 `function_call`，
  而不是 `function_call_output`。
- `function_call_output` 在客户文档里也被明确列为当前拒绝：
  [`guides/migration.mdx:169-178`](tokencamp-www:docs-site/guides/migration.mdx:169-178)；
  OpenAPI 的 `/v1/responses` 描述同样写明 stateless 子集和拒绝字段
  （[`docs/openapi.yaml:495-523`](tokencamp-pro:docs/openapi.yaml:495-523)）。

### 3.5 流式成功路径与错误路径

成功路径：Codex 接受 OpenAI Responses 的 `response.created` /
`response.output_item.added` / `response.output_text.delta` / `response.completed`
SSE 形状；mock 用这套事件返回文本，`codex exec` 正常结束并输出文本。tokencamp 的
文档描述的也是同一族事件，终端 `response.completed` 携带完整 response 对象和 usage
（[`guides/migration.mdx:180-184`](tokencamp-www:docs-site/guides/migration.mdx:180-184)）。
**成功路径的 SSE 形状看起来兼容，但没有在真实 tokencamp 网关上端到端验证。**

错误路径有一个次级不兼容：tokencamp 文档说 mid-stream failure 是单个 `error` 事件，
而 Codex 的 SSE 解析器在错误事件上只处理 `response.failed`；`error` 事件会落到
`_ => trace!("unhandled responses event")`，随后如果流关闭而没有
`response.completed`，Codex 报 `stream closed before response.completed`
（[`codex-api/src/sse/responses.rs:312-396`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/codex-api/src/sse/responses.rs#L312-L396)）。
这不是第一轮阻塞点，但如果 pro 将来支持 Codex，错误语义也要对齐：要么发
`response.failed`，要么同时发 `error` 和 `response.failed`。

## 4. 模型命名与 `model_reasoning_effort`

### 4.1 模型字段

- Codex 的 `model` 是字符串，原样放进请求；不会按 tokencamp catalog 校验。
  实测 `-m auto` 发送 `"model": "auto"`；`-m deepseek/deepseek-v4-pro` 发送
  `"model": "deepseek/deepseek-v4-pro"`。两者都被 mock 接受，tokencamp 文档说
  `auto` / catalog id / `@preset/<slug>` / `~author/family-latest` 都可用
  （[`docs/openapi.yaml:1939-1960`](tokencamp-pro:docs/openapi.yaml:1939-1960)）。
- Codex 可能主动 `GET /v1/models`（本次 mock 在一次请求后收到一次 `GET /v1/models`）。
  tokencamp 的 `/v1/models` 是公开只读，返回 OpenAI 形状；mock 返回空列表时 Codex
  仍继续执行，所以这不是阻塞点。
- 模型名命中 Codex 自带 catalog 时，Codex 的 `ModelInfo` 会影响 reasoning 和
  verbosity；tokencamp catalog id 不在 Codex catalog 里，因此按“非 reasoning 模型”
  处理，默认 `reasoning: null`。

### 4.2 `model_reasoning_effort`

- `model_reasoning_effort` 是 Codex 顶层配置（也可放 profile）；Codex 把它翻译成
  Responses 请求的 `reasoning` 对象，而不是原样发送 `model_reasoning_effort`。
- 实测 `-m auto -c model_reasoning_effort="high"` 仍发送 `"reasoning": null`：
  因为 `auto` 不是 Codex catalog 模型，`build_reasoning` 返回 None（源码
  [`core/src/client.rs:721-726`](https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/core/src/client.rs#L721-L726)）。
- 实测 `-m gpt-5.5 -c model_reasoning_effort="high"` 发送
  `"reasoning": {"effort": "high"}` 且 `"include": ["reasoning.encrypted_content"]`。
  这会被 tokencamp 先以 `include`、后以 `reasoning` 拒绝。
- 结论：对 tokencamp catalog id / `auto`，`model_reasoning_effort` 不构成第一轮阻塞
  （Codex 发 null）；但它也没有实际效果。若 tcamp 支持 Codex，不能承诺 reasoning
  可用。

## 5. 启动、检测与配置合并

| 项目 | 事实 | 依据 |
| --- | --- | --- |
| 可执行文件 | `codex`（npm 全局包 `@openai/codex` 的 `bin/codex.js` 包装 native binary） | 【实测】`which codex`、`codex --version` |
| 版本 | `codex-cli 0.132.0` | 【实测】`codex --version` |
| 非交互 | `codex exec [PROMPT]`（别名 `codex e`）；`--json` 输出 JSONL 事件 | 【实测】`codex exec --help` |
| 常用 flag | `-m/--model`、`-c/--config`、`-p/--profile`、`-C/--cd`、`-s/--sandbox`、`--skip-git-repo-check`、`--ephemeral` | 【实测】`codex exec --help` |
| 交互模式 | 无子命令或直接 `codex [PROMPT]` 进入 TUI | 【实测】`codex --help` |
| 配置校验 | `codex doctor --json`；`codex --strict-config` 对未知字段报错 | 【实测】`codex doctor --help` / `codex --help` |
| 配置目录 | `CODEX_HOME`；默认 `~/.codex`；本次全部用临时 `CODEX_HOME` | 【实测】`codex doctor --json` |
| 退出码 | 成功 0；配置解析错误 1；CLI 参数错误 2 | 【实测】`wire_api=chat` → 1；`-a` 误用 → 2 |

启动/集成注意事项：

1. **stdin**：`codex exec` 在 stdin 非 TTY 时会先打印 `Reading additional input from stdin...`；
   如果 tcamp 用子进程启动，必须 `stdin` 设为 null/`/dev/null` 或显式关闭，否则可能等待
   输入。本次脚本用 `< /dev/null` 后正常。
2. **`-a` 不是 `codex exec` 的参数**：审批策略走配置或交互 TUI；`codex exec` 默认
   `approval: never`。不要照搬 `codex` 顶层 help 的 `-a`。
3. **配置合并**：Codex 的 loader 按层合并（MDM / system / session flags / user），
   `-c` 是 session flags；`[profiles.*]` 是用户 config 内的命名层。`tcamp` 若写
   `config.toml`，必须用 format-preserving 的 TOML 编辑器（如 `toml_edit`），只
   upsert 自己的 provider/profile，不能整文件重写；顶层 `model` / `model_provider`
   属于用户选择，优先用 profile + `-p tokencamp` 避免覆盖。
4. **检测**：`command -v codex` 优先；再跑 `codex --version` 解析版本。`codex doctor
   --json` 可检查 config/auth/runtime，但本次只用于研究，不写用户 `~/.codex`。
5. **不要用 `requires_openai_auth = true`**：那会要求 ChatGPT 登录并把 token 放
   `auth.json`，tokencamp 的 Bearer key 路径应保持 `false` + `env_key`。

## 6. 最小 pro 改动（仅记录，不在 v1 实施）

若 tokencamp 想让 Codex 0.132.x 可用，按优先级至少需要：

1. **接受空 `include`。** `include: []` 是 Codex 无 reasoning 时的固定形状；应视为
   no-op，而不是 400。非空 `include`（`["reasoning.encrypted_content"]`）可以选择
   忽略（tokencamp 没有 reasoning 状态）或继续拒绝；若继续拒绝，Codex 开启 reasoning
   时仍不可用。
2. **接受并翻译 tool transcript 回放。** 至少支持：
   - `function_call` item → 一条 assistant message，带 `tool_calls: [{id: call_id, type:
     "function", function: {name, arguments}}]`；
   - `function_call_output` item → 一条 `role: "tool"`、`tool_call_id: call_id` 的
     message，content 为 `output`（字符串或 content items 展开为文本）；
   - 保持原顺序，并处理同一 assistant turn 的文本 + 多个 tool calls。
   这需要新的翻译逻辑；不是 validate 放行一行。`tokencamp` 的 canonical `ChatRequest`
   已经建模 `tool_calls` / `tool_call_id`（[`crates/core/src/chat.rs:59-68`](tokencamp-pro:crates/core/src/chat.rs:59-68)），
   所以上游 chat/completions 能表达，但 Responses skin 的 `to_chat_request` 目前只把
   message item 映射成 chat message（[`responses.rs:594-655`](tokencamp-pro:crates/gateway/src/routes/responses.rs:594-655)），
   需要扩展。
3. **处理 `reasoning` / `reasoning` item。** 若 Codex 发非空 `reasoning` 或回放
   `reasoning` item（含 `encrypted_content`），tokencamp 无法解密；建议接受并忽略，
   或在文档里明确“reasoning 不生效”。当前是 400。
4. **错误事件对齐。** mid-stream failure 发 `response.failed`（Codex 能识别），或
   同时发 `error` + `response.failed`；只发 `error` 会让 Codex 报
   “stream closed before response.completed”。
5. **处理 `client_metadata` / `prompt_cache_key`。** 两者目前落 `extra`，openai
   upstream 适配器原样透传（[`crates/gateway/src/upstream/openai.rs:155-165`](tokencamp-pro:crates/gateway/src/upstream/openai.rs:155-165)）。
   `prompt_cache_key` 是 OpenAI chat 参数，风险较低；`client_metadata` 不是，建议在
   Responses skin 里剥掉，或确认上游会忽略。

**结论：最小改动不是“一行 validate”。** 空 `include` 很小，但 tool replay 翻译是
新功能，且需要端到端测试 tool loop、流式、usage/计费。若 v1 目标是三个 harness（Claude
Code、OpenCode、Kimi Code），Codex 应保持 out of scope；如果产品上必须支持 Codex，
应另立一个 pro 兼容性 ticket，而不是在 `tcamp` 里做代理/适配层。

## 7. 实验记录与复现

### 7.1 环境

```text
$ codex --version
codex-cli 0.132.0

$ which codex
/Users/<user>/.npm-global/bin/codex
```

临时目录：

- `CODEX_HOME=/tmp/tcamp-codex-home`（配置见 §1.3）
- `CODEX_HOME=/tmp/tcamp-codex-profile`（profile 变体）
- mock：`/tmp/codex-mock/server.py`，仅监听 `127.0.0.1:8899`
- 未读取/修改用户真实 `~/.codex`；未使用真实 tokencamp key；未做付费请求。

### 7.2 关键命令

```bash
# 1. 基本请求（文本 mock）
CODEX_HOME=/tmp/tcamp-codex-home TOKENCAMP_API_KEY=dummy-key \
  codex exec --skip-git-repo-check --ephemeral -s read-only "Say hi." < /dev/null

# 2. 触发 tool loop（mock 返回 function_call）
CODEX_HOME=/tmp/tcamp-codex-home TOKENCAMP_API_KEY=dummy-key \
  codex exec --skip-git-repo-check --ephemeral -s read-only \
  "Run echo hello-from-mock and report the output." < /dev/null

# 3. 复现 chat wire 移除
CODEX_HOME=/tmp/tcamp-codex-home TOKENCAMP_API_KEY=dummy-key \
  codex exec --skip-git-repo-check --ephemeral -s read-only \
  -c 'model_providers.tokencamp.wire_api="chat"' "Say hi." < /dev/null

# 4. profile 变体
CODEX_HOME=/tmp/tcamp-codex-profile TOKENCAMP_API_KEY=dummy-key \
  codex exec --skip-git-repo-check --ephemeral -s read-only -p tokencamp "Say hi." < /dev/null
```

观察结果：

- 文本 mock：`codex exec` 退出 0，输出 `hello from the mock server`。
- tool mock：第二轮请求出现 `function_call` + `function_call_output`；因 mock 继续返回
  tool call，会话会循环，实验时手动终止（只取前两轮样本）。
- `wire_api=chat`：退出 1，错误见 §2。
- profile：退出 0，header 显示 `model: auto`、`provider: tokencamp`。
- 把 `base_url` 改成无 `/v1`：mock 收到 `POST /responses`，证明 Codex 不会自动补
  `/v1`。

### 7.3 tokencamp 侧的验证边界

本次没有在运行中的 tokencamp gateway 上复现 400：

- 本机 `127.0.0.1:3005` 没有启动服务；
- `api.tokencamp.com` 从本环境请求超时（`curl` 15s timeout，HTTP 000）；
- 因此 tokencamp 的拒绝行为来自路由源码 + 对 `ResponsesRequest` 的运行时反序列化
  检查（`include=Some([])`、function_call items 可反序列化为 `ResponseInputItem`
  但 `kind` 非 message）。这与 `guides/migration.mdx` 和 `docs/openapi.yaml` 的
  描述一致。

**真实网关端到端仍是 open question**（§8）；但源码路径足够确定：`validate()` 在
鉴权前执行，`include` / `input` 的拒绝分支没有配置开关。

## 8. 未决问题

1. **真实 tokencamp 网关端到端**：如果 pro 修了 `include` + tool replay，用真实 key
   跑一次 `codex exec`，确认 `/v1/responses` 接受 Codex 的完整 body，并确认流式
   usage/计费。
2. **Codex 版本漂移**：0.132.0 移除了 chat wire；未来版本是否重新引入、或改变
   `include` / tool item 形状，需要重新验证。当前结论只对 0.132.x 有效。
3. **pro 是否值得支持 Codex**：tool replay 翻译是新功能，影响 stateless 子集的边界
   和测试矩阵；如果 v1 只做三个 harness，建议 Codex 继续 out of scope。
4. **`client_metadata` / `prompt_cache_key` 上游容忍度**：tokencamp 会作为 `extra`
   透传给 openai-compatible 上游；是否所有上游都忽略未知字段未验证。
5. **reasoning 语义**：Codex 对已知模型会发 `reasoning` + encrypted content；
   tokencamp 不能解密/保持 reasoning 状态，只能忽略或拒绝。若忽略，需在文档里说清
   “reasoning 不生效”。
6. **错误流**：tokencamp 的 `error` 事件与 Codex 的 `response.failed` 期望不一致；
   是否需要 pro 改，或 Codex 未来版本兼容 `error`，未定。
7. **代理方案**：在 `tcamp` 里做 Responses→Chat 代理可以让 Codex 工作，但违反
   “tcamp 只写配置、不转发流量”的 v1 边界，不在本 ticket 范围。

## 9. 引用

### Codex CLI 源码（tag `rust-v0.132.0`）

- `WireApi` 只保留 Responses、`chat` 报错：
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/model-provider-info/src/lib.rs#L49-L78
- `ModelProviderInfo` 字段（`name` / `base_url` / `env_key` / `wire_api` /
  `requires_openai_auth` / `supports_websockets`）：
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/model-provider-info/src/lib.rs#L84-L136
- `chat` wire 的单元测试：
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/model-provider-info/src/model_provider_info_tests.rs#L110-L122
- Responses 请求构造（`include` 总是 `Vec`、`store`、`stream`、`reasoning`）：
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/core/src/client.rs#L712-L764
- `ResponsesApiRequest` 字段：
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/codex-api/src/common.rs#L169-L190
- `ResponseItem` 的 `function_call` / `function_call_output` / `reasoning`：
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/protocol/src/models.rs#L750-L847
- Codex SSE 解析（只处理 `response.failed`；`error` 事件落到 `_`）：
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/codex-api/src/sse/responses.rs#L312-L396
- 配置层合并与 profile：
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/config/src/loader/README.md
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/config/src/profile_toml.rs#L24-L32
- `model_providers` 保留 ID 与空 name 校验：
  https://github.com/openai/codex/blob/rust-v0.132.0/codex-rs/config/src/config_toml.rs#L933-L978
- chat wire 移除说明：
  https://github.com/openai/codex/discussions/7782
- 官方配置参考（本环境返回 403，未能抓取正文；以源码为准）：
  https://developers.openai.com/codex/config-reference

### tokencamp-pro 源码（commit `25db44b`）

- `crates/gateway/src/routes/responses.rs:121-126` — validate 在 lifecycle/鉴权之前
- `crates/gateway/src/routes/responses.rs:187-232` — `store` / `include` / `reasoning` 等拒绝面
- `crates/gateway/src/routes/responses.rs:280-327` — input item 只允许 message
- `crates/gateway/src/routes/responses.rs:594-655` — Responses → ChatRequest 的 message-only 翻译
- `crates/core/src/responses.rs:61-84` — `include: Option<Vec<String>>`、`reasoning: Option<Value>`
- `crates/core/src/responses.rs:96-113` — `ResponseInputItem` 的 kind/role/content/extra
- `crates/core/src/chat.rs:59-68` — canonical `Message` 已有 `tool_calls` / `tool_call_id`
- `crates/gateway/src/upstream/openai.rs:155-165` — openai upstream 对 `extra` 原样透传
- `docs/openapi.yaml:495-523` — `/v1/responses` 的 stateless 子集描述
- `docs/openapi.yaml:1939-1960` — `ResponsesRequest` 的 model 说明

### 客户文档（tokencamp-www commit `640d204`）

- `docs-site/guides/migration.mdx:169-178` — stateless-only；`include` / `reasoning` /
  `function_call_output` 当前拒绝
- `docs-site/guides/migration.mdx:180-184` — `response.*` SSE 与 `error` 事件

### 本报告实验

所有 Codex 实验都在临时 `CODEX_HOME` 下进行；mock 只绑定 `127.0.0.1`；未读取/修改
用户真实 `~/.codex`；未使用真实 tokencamp key；未做付费调用。实验后已停止 mock。
