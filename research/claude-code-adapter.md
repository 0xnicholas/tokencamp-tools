# Claude Code 适配器契约（ticket #2）

- 状态：research 完成，2026-11-01
- 被测版本：Claude Code `2.1.316`（macOS x86_64，npm `@anthropic-ai/claude-code-darwin-x64`，2026-10-31 发布）
- 平台侧：`tokencamp-pro@25db44b`（2026-10-31）；客户文档 `tokencamp-www@640d204`（2026-10-22）
- 范围：`tcamp` v1 的 Claude Code adapter；纯客户端，不改 tokencamp-pro
- 结论日期：2026-11-01（Claude Code 迭代很快，见 §10）

## 0. 结论（Verdict）

**有条件 GO：客户端契约清楚、可实施；但当前默认平台配置下不可用，必须先满足平台前提。**

`tcamp` 应写 `~/.claude/settings.json` 的 `env` 块，指向 tokencamp 的 **origin**（`https://api.tokencamp.com`，不带 `/v1`），用 `ANTHROPIC_AUTH_TOKEN` 发 `Authorization: Bearer`，并把 `ANTHROPIC_API_KEY` 显式置空。模型必须显式钉到 tokencamp catalog id；Claude Code 默认会发 `claude-opus-5-5`，tokencamp 必 404。

三个必须写进 v1 契约的限制：

1. **`/v1/messages` 只服务有 `provider_type: anthropic` deployment 的模型**，而 `config/default.yaml` 当前 seed 了 **0 个** Anthropic deployment（7 个全是 `openai_compatible`）。默认/dev 栈上，任何模型在 `/v1/messages` 都会 404 `model_not_found`（文案带 `no anthropic deployment for /v1/messages`）。这是 v1 的 blocker，不是 tcamp 能修的。
2. **thinking / tool-search / document 等 content block 未建模**。tokencamp 的 `ContentBlock` 只有 `text` / `image` / `tool_use` / `tool_result`；Claude Code 在未知模型上默认发 `thinking: {"type":"adaptive"}`，并且 tool search 默认开启。v1 必须设 `MAX_THINKING_TOKENS=0` 与 `ENABLE_TOOL_SEARCH=false`，否则非流式响应或下一轮请求会 400。或者平台先补 content block 类型。
3. **`cache_control` 只在 `system` / `tools` 上保留，在 message content block 上被丢弃**（`ContentBlock` 没有 `flatten extra`）。Claude Code 会把 `cache_control` 打在 user/assistant 消息块上；tokencamp 反序列化后重序列化会把它删掉。Prompt caching 只能缓存 system/tools，多轮对话前缀缓存退化。

不建议在平台前提未确认前让 `tcamp` 对外宣称“Claude Code 已支持”。研究结论是：**adapter 可以写，但要带条件上线**。

## 1. 方法与证据等级

- Claude Code 侧：在临时 `HOME=/tmp/tcamp-home` 下运行官方 native binary `2.1.316`，配一个本地 mock Anthropic gateway（只绑 `127.0.0.1:8765`），记录 request line、关键 header、完整 body；未读取/修改用户真实 `~/.claude`。
- 平台侧：读 `tokencamp-pro@25db44b` 源码与 `docs/openapi.yaml`；没有 tokencamp API key，且研究环境访问 `api.tokencamp.com` 超时，**未对真实网关做端到端请求**。平台侧结论是源码/契约级，不是线上实测。
- content block 行为：用 `/tmp/tcamp-typecheck` 临时 Rust crate 直接依赖 `tokencamp-core`，对真实 wire 类型做 `serde_json` round-trip；输出见 §12.3。
- 引用标记：`【文档】` 官方文档；`【源码】` tokencamp-pro 源码；`【实测】` 本报告实验；`【推断】` 由类型/代码直接推出但未跑真实网关。

## 2. 配置位置与优先级

### 2.1 文件与 scope

| 路径 | scope | 对 tcamp 的用途 | 依据 |
| --- | --- | --- | --- |
| `~/.claude/settings.json` | user，全机器全项目 | **推荐默认写入目标** | 【文档】[Settings](https://code.claude.com/docs/en/settings) |
| `.claude/settings.json` | shared project，会进 git | 不写凭据；只放团队共享的非敏感项 | 同上 |
| `.claude/settings.local.json` | project local，个人 | 可选 per-project profile；交互式要 trust 后才应用 | 同上 |
| `/Library/Application Support/ClaudeCode/managed-settings.json`（macOS）<br>`/etc/claude-code/managed-settings.json`（Linux/WSL） | managed，管理员 | tcamp 只读检测，不写 | 【文档】[Managed settings](https://code.claude.com/docs/en/managed-settings) |
| `~/.claude.json` | global config（账号、MCP、trust、/config 全局键） | **不要写**；不是 settings.json | 【文档】[Settings](https://code.claude.com/docs/en/settings) |

优先级（高到低）：managed > `--settings`（命令行 JSON/文件）> project local > shared project > user。`env` 块是一个普通 key，按同一套文件优先级合并。【文档】[Settings § Settings precedence](https://code.claude.com/docs/en/settings#settings-precedence)

### 2.2 `env` 块是否生效

**生效。** `env` 是 settings.json 的正式 key，类型是 `object<string,string>`；Claude Code 会把值原样放进进程环境，不经过 shell，所以 `~`、`$HOME` 不会被展开，必须写绝对路径或字面值。【文档】[All settings § env](https://code.claude.com/docs/en/settings-reference#env)

实测：

- `~/.claude/settings.json` 的 `env` 里放 `ANTHROPIC_BASE_URL` / `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_MODEL`，`claude -p` 直接生效。
- settings 文件的 `env` **覆盖** shell 里同名 export。shell 里故意设 `ANTHROPIC_BASE_URL=http://127.0.0.1:9999`、`ANTHROPIC_AUTH_TOKEN=sk-shell`，实际请求仍走 settings 的 `8765` 和 `sk-tc-envblock`。【实测】
- `.claude/settings.local.json` 的 `env` 在 `-p`（非交互）下也生效；交互式下 project/local 的 `env` 要等 workspace trust 之后才应用。文档明确：project/local 的 `env` 在 trust 后或 `-p` 启动时应用。【文档】[env 何时应用](https://code.claude.com/docs/en/settings-reference#when-claude-code-applies-env-values)；【实测】`-p` 下 project local 生效。
- `ANTHROPIC_MODEL` 在 `env` 块里会覆盖 `"model": "sonnet"` 这类 settings key；`--model` 又覆盖 `ANTHROPIC_MODEL`。【文档】[Settings § precedence](https://code.claude.com/docs/en/settings#settings-precedence)；【实测】

### 2.3 写文件行为

Claude Code 会监听 settings 文件并在保存时重载；`/config`、`/model` 等会话内操作会写 `~/.claude/settings.json`。【文档】[Settings § when edits take effect](https://code.claude.com/docs/en/settings#when-edits-take-effect)

实测写入行为：

- 原文件含未知 key `"tcampUnknownKey": {"keep": true}`；在交互式会话执行 `/config verbose=true` 后，Claude Code 重写了整个文件：
  - 新增 `"verbose": true`；
  - **保留了 `tcampUnknownKey`**；
  - 整个文件被重新格式化（2 空格缩进、key 顺序变化）。
- settings JSON schema 的根 `additionalProperties: true`，允许未知 key。【文档】[JSON schema](https://json.schemastore.org/claude-code-settings.json)
- 因此 tcamp **不能做文本级 patch**（会破坏格式/丢 key），但可以安全地做 JSON read-modify-write；Claude Code 本身不会删除未知 key【实测】，tcamp 也不要依赖这一点做持久状态。

## 3. 鉴权与 base URL

### 3.1 base URL：origin，不带 `/v1`

`ANTHROPIC_BASE_URL` 是 Claude Code 的 **API origin**；Claude Code 自己拼 `/v1/messages`（还会带 `?beta=true`）和 `/v1/models`。【文档】[Connect § set the base URL](https://code.claude.com/docs/en/llm-gateway-connect#set-the-base-url-and-credential)；【文档】[Gateway protocol § API formats](https://code.claude.com/docs/en/llm-gateway-protocol#api-formats)

实测（2.1.316）：

| `ANTHROPIC_BASE_URL` | 实际请求 | 结果 |
| --- | --- | --- |
| `http://127.0.0.1:8765` | `POST /v1/messages?beta=true` | 成功 |
| `http://127.0.0.1:8765/` | `POST /v1/messages?beta=true` | 成功（尾斜杠被归一） |
| `http://127.0.0.1:8765/v1` | `POST /v1/v1/messages?beta=true` | 404，`claude -p` exit 1，报 “There's an issue with the selected model …” |

**结论：`ANTHROPIC_BASE_URL=https://api.tokencamp.com`，不要写 `https://api.tokencamp.com/v1`。** 这与 tokencamp 客户文档里 Anthropic/OpenAI SDK 示例的 `base_url="…/v1"` 不同：SDK 的 `base_url` 语义是“连 `/v1` 一起”，Claude Code 的 `ANTHROPIC_BASE_URL` 不是。`guides/migration.mdx` 说“Claude Code-style clients take the same two settings”会误导，tcamp 不要照抄。【源码】[migration.mdx L74-L81](https://github.com/0xnicholas/tokencamp-www/blob/640d204/docs-site/guides/migration.mdx#L74-L81)

> 客户文档的 base URL 目前还是 placeholder：`snippets/base-url.mdx` 明确写“provisional，launch 前确认”。【源码】[base-url.mdx](https://github.com/0xnicholas/tokencamp-www/blob/640d204/docs-site/snippets/base-url.mdx#L1-L4)

### 3.2 header 语义

| Claude Code 变量 | 实际 header | 对 tokencamp |
| --- | --- | --- |
| `ANTHROPIC_AUTH_TOKEN` | `Authorization: Bearer <value>` | ✅ 唯一可用 |
| `ANTHROPIC_API_KEY` | `x-api-key: <value>` | ❌ tokencamp `/v1` 不读 |
| `apiKeyHelper` | **两个都发**：`Authorization: Bearer` + `x-api-key` | ✅ tokencamp 读 Bearer；可作 key 轮换方案 |
| 两者都设 | 两个 header 都发 | ⚠️ 能工作，但 `/status` 会显示 API key，且启动可能有 auth-conflict 警告 |
| `ANTHROPIC_API_KEY=""` | 不发 `x-api-key`（空值视为 unset） | ✅ 用来压掉 shell 里遗留的 key |

依据：【文档】[Connect § credential variable](https://code.claude.com/docs/en/llm-gateway-connect#set-the-credential-variable)、[Connect § header mapping](https://code.claude.com/docs/en/llm-gateway-connect#how-the-credential-variable-maps-to-a-header)、[env-vars § ANTHROPIC_API_KEY](https://code.claude.com/docs/en/env-vars)；【实测】。

实测细节：

- 只设 `ANTHROPIC_AUTH_TOKEN`：`authorization: Bearer sk-tc-envblock`，无 `x-api-key`。
- 只设 `ANTHROPIC_API_KEY`：无 `authorization`，`x-api-key: sk-api-only`；tokencamp 会 401。
- 两者都设：`authorization: Bearer …` + `x-api-key: …` 同时出现；Claude Code 的 `--output-format json` 里 `apiKeySource` 是 `ANTHROPIC_API_KEY`，尽管实际 Bearer 生效。
- `env` 块里写 `"ANTHROPIC_API_KEY": ""`：即使 shell 里 `ANTHROPIC_API_KEY=sk-shell`，请求也只有 Bearer，`apiKeySource: "none"`。**这是 tcamp 保证 Bearer-only 的关键写法。**

tokencamp 侧只读 `Authorization`：

- `direct.rs` 只从 `headers.get(AUTHORIZATION)` 取，`auth::authenticate` 只 `strip_prefix("Bearer ")`；没有 `x-api-key` fallback。【源码】[direct.rs L399-L402](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/direct.rs#L399-L402)、[auth.rs L54-L60](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/pipeline/auth.rs#L54-L60)

### 3.3 登录态与 OAuth

- 只设 `ANTHROPIC_BASE_URL` 而不设凭据时，Claude Code 会继续用保存的 claude.ai 登录/OAuth token；tokencamp 的 `/v1` 不接受 OAuth identity token，会 401。必须设 `ANTHROPIC_AUTH_TOKEN`（或用 `apiKeyHelper`）。【文档】[Other LLM gateways § subscriptions](https://code.claude.com/docs/en/llm-gateway#subscriptions-and-gateways)；平台约束。
- `ANTHROPIC_AUTH_TOKEN` 立即覆盖已保存登录；不会弹“批准 key”提示。`ANTHROPIC_API_KEY` 在交互式下需要一次批准。【文档】[Connect § conflicts](https://code.claude.com/docs/en/llm-gateway-connect#conflicts-with-an-existing-login)
- `apiKeyHelper` 的 stdout 必须是纯 key；2.1.227+ 有 banner/log 行会让 helper 失败；默认缓存 5 分钟。【文档】[Connect § apiKeyHelper](https://code.claude.com/docs/en/llm-gateway-connect#rotate-credentials-with-apikeyhelper)

## 4. 模型槽位与模型 ID

### 4.1 槽位表

| 变量/key | 用途 | tcamp 建议 |
| --- | --- | --- |
| `ANTHROPIC_MODEL` | 主会话模型；覆盖 `model` key；`--model` 又覆盖它 | **必设**，钉到 catalog id |
| `model`（settings key） | 默认模型；`ANTHROPIC_MODEL` / `--model` 可覆盖 | 可写，但 tcamp 以 `ANTHROPIC_MODEL` 为准 |
| `ANTHROPIC_DEFAULT_MODEL` | 新会话默认模型；仅当没有任何文件设置 `model` 时生效；v2.1.236+ | 不用；不如 `ANTHROPIC_MODEL` 确定 |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | `opus` alias；`opusplan` 的 Plan 阶段 | 映射到强模型 |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | `sonnet` alias；`opusplan` 的执行阶段 | 映射到日常编码模型 |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | `haiku` alias，**以及后台功能**（标题、摘要、classifier 等） | **必设**；不设会去请求 `claude-haiku-5-5` 并 404 |
| `ANTHROPIC_DEFAULT_FABLE_MODEL` | `fable` / `best` alias；ticket 没列，但 Claude Code 在用 | 建议映射到强模型 |
| `ANTHROPIC_SMALL_FAST_MODEL` | 已废弃，旧版 Haiku 类后台模型 | **不要设**；实测只设它时 `--model haiku` 仍发 `claude-haiku-5-5` |
| `CLAUDE_CODE_SUBAGENT_MODEL` | subagent / teammate / workflow 的默认模型 | 可选；建议设为主模型，避免子代理走 Claude ID |
| `ANTHROPIC_DEFAULT_*_MODEL_NAME` / `_DESCRIPTION` | picker 显示名/描述；gateway 下也生效 | 可选，改善 UX |
| `ANTHROPIC_DEFAULT_*_MODEL_SUPPORTED_CAPABILITIES` | 声明 effort/thinking 等能力 | **不要指望**：文档明确说在 `ANTHROPIC_BASE_URL` gateway 后无效 |

依据：【文档】[Model config § environment variables](https://code.claude.com/docs/en/model-config#environment-variables)、[Model config § customize pinned model](https://code.claude.com/docs/en/model-config#customize-pinned-model-display-and-capabilities)、[env-vars](https://code.claude.com/docs/en/env-vars)；`_SUPPORTED_CAPABILITIES` 的 gateway 例外见【文档】[Gateway protocol § feature pass-through](https://code.claude.com/docs/en/llm-gateway-protocol#feature-pass-through)。

### 4.2 实测映射

配好 `ANTHROPIC_DEFAULT_OPUS_MODEL=deepseek/deepseek-v4-pro`、`..._SONNET_MODEL=qwen/qwen3-coder-480b`、`..._HAIKU_MODEL=zhipu/glm-5.3`、`..._FABLE_MODEL=deepseek/deepseek-v4-pro` 后：

| 启动方式 | 实际 wire `model` |
| --- | --- |
| 默认（`ANTHROPIC_MODEL=deepseek/deepseek-v4-pro`） | `deepseek/deepseek-v4-pro` |
| `--model sonnet` | `qwen/qwen3-coder-480b` |
| `--model opus` | `deepseek/deepseek-v4-pro` |
| `--model haiku` | `zhipu/glm-5.3` |
| `--model fable` | `deepseek/deepseek-v4-pro` |
| 只设 `ANTHROPIC_SMALL_FAST_MODEL=zhipu/glm-5.3` + `--model haiku` | `claude-haiku-5-5`（SMALL_FAST 被忽略） |

【实测】2.1.316。

### 4.3 `/v1/messages` 接受哪些 model id

tokencamp 的 `/v1/messages` 解析是 `direct.rs::resolve_chain`：`~` 开头走 family alias，其他走 `find_available_model`（catalog 名或 catalog alias）。它**没有** chat 管道的 `auto` 展开和 `@preset/<slug>` 解析。【源码】[direct.rs L698-L719](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/direct.rs#L698-L719)、[catalog.rs L74-L101](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/catalog.rs#L74-L101)

OpenAPI 对 `/v1/messages` 的 `model` 描述只有：**“Catalog model id or a family alias `~author/family-latest`”**；而 chat completions 的 `model` 描述包含 `auto`、`@preset/<slug>`、family alias。【源码】[docs/openapi.yaml L1645-L1648](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/docs/openapi.yaml#L1645-L1648)、[docs/openapi.yaml L1459-L1461](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/docs/openapi.yaml#L1459-L1461)

| Claude Code 发什么 | tokencamp `/v1/messages` | 备注 |
| --- | --- | --- |
| `deepseek/deepseek-v4-pro` | ✅ | catalog id |
| `deepseek-pro`（catalog alias） | ✅（`find_available_model` 走 index） | 但文档只承诺 catalog id / family alias |
| `~deepseek/deepseek-v4-latest` | ✅ | family alias；解析到最新未过期成员 |
| `auto` | ❌ 404 `model_not_found` | 只有 chat pipeline 展开 auto |
| `@preset/cheap-chat` | ❌ 404 `model_not_found` | 只有 chat pipeline 解析 preset |
| `qwen/qwen3-coder-480b[1m]` | Claude Code 先剥掉 `[1m]`，实际发 `qwen/qwen3-coder-480b` | 【实测】`[1m]` 是客户端后缀，不发给网关 |

Claude Code 本身对 `ANTHROPIC_MODEL` / `--model` 的字符串几乎不校验；`auto`、`@preset/...`、`~...` 都原样发出去【实测】。所以 **tcamp 必须在写入前校验**：只允许 `/v1/messages` 能解析的 catalog id / alias / `~family-latest`，否则用户会看到 404。

另外两个 `/v1/messages` 的硬门：模型 `kind` 必须是 chat（embedding 模型会被 400 拒绝），且 `capabilities.streaming` 必须为 true（Claude Code 总是 `stream:true`，不流式的模型会被 400 拒绝）。【源码】[messages.rs L199-L223](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L199-L223) tcamp 选模型时要同时查 `kind` 与 `capabilities.streaming`。

**不要让用户给 tokencamp id 加 `[1m]`。** Claude Code 会把它当客户端后缀剥掉再发，但在此之前会按 `[1m]` 假设 1M 窗口，导致 `CLAUDE_CODE_MAX_CONTEXT_TOKENS` 不生效（文档说带 `[1m]` 的未知 ID 需要同时 `CLAUDE_CODE_DISABLE_1M_CONTEXT=1` 才能让 `MAX_CONTEXT_TOKENS` 生效）。tokencamp 的窗口应通过 `CLAUDE_CODE_MAX_CONTEXT_TOKENS` 表达，不要用 `[1m]`。【文档】[Model config § which IDs](https://code.claude.com/docs/en/model-config#which-ids-claude_code_max_context_tokens-applies-to)；【实测】`[1m]` 被剥离

### 4.4 模型发现（`/v1/models`）不适用

Claude Code 的 gateway model discovery 请求 `GET /v1/models?limit=1000`，只保留 `id` 里包含 `claude` 或 `anthropic`（大小写不敏感）的条目。【文档】[Gateway protocol § model discovery](https://code.claude.com/docs/en/llm-gateway-protocol#model-discovery)

实测：mock `/v1/models` 返回 4 个模型（`deepseek/…`、`zhipu/…`、`qwen/…`、`my-gateway-claude-sonnet-5`），开启 `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1` 后，`~/.claude/cache/gateway-models.json` 只缓存了 `my-gateway-claude-sonnet-5` 一个。【实测】2.1.316

**结论：tokencamp 的 catalog id 不含 `claude`/`anthropic`，开 discovery 不会把 tokencamp 模型加进 `/model` picker。不要设 `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1`。** 需要 picker 时用 `modelPicker`（v2.1.242+，user/managed）或 `ANTHROPIC_CUSTOM_MODEL_OPTION`（单个），它们接受任意字符串 ID。【文档】[Model config § add a custom model option](https://code.claude.com/docs/en/model-config#add-a-custom-model-option)、[settings-reference § modelPicker](https://code.claude.com/docs/en/settings-reference#modelpicker)

`modelPicker` 的 `behavesAs`（v2.1.257+）可以把 tokencamp 模型映射到一个 Claude Code 已知的模型 ID，从而套用它的 capabilities/effort 默认值。**v1 不要用**：它会让 Claude Code 以为这是 Claude 模型，可能改变 thinking/context 行为，反而掩盖 tokencamp 的真实能力。只把 `modelPicker` 当显示层（`model` + `label` + `description`）。【文档】[settings-reference § modelPicker](https://code.claude.com/docs/en/settings-reference#modelpicker)

### 4.5 context window 与 max output 元数据

- `/v1/models` 的 `ModelObject` 只有 `id` / `object` / `created` / `owned_by` / `kind` / `expiration_date`，**没有 context window、max output、display_name、description**。【源码】[routes/mod.rs L510-L548](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/mod.rs#L510-L548)
- Claude Code 对不认识的 model id 默认假设 **200K context**；对 `claude-` 开头或带 `[1m]` 的 ID 另有规则。tokencamp id 不满足这些规则，所以 `CLAUDE_CODE_MAX_CONTEXT_TOKENS` 会直接生效。【文档】[Model config § correct the window](https://code.claude.com/docs/en/model-config#correct-the-window-for-a-gateway-or-custom-model-id)
- tokencamp 的 `/feed/models`（公开、非 v1）带 `context_window`，可以拿来填 `CLAUDE_CODE_MAX_CONTEXT_TOKENS`；它没有 max output 字段。【源码】[feed.rs L40-L60](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/feed.rs#L40-L60)
- `max output` 在 catalog/config 里也没有权威字段；`ModelConfig` 只有 `context_window`，没有 `max_output`。【源码】[config.rs L319-L347](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/config.rs#L319-L347)、[config/default.yaml L75-L100](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/config/default.yaml#L75-L100)
- Claude Code 对未知模型默认 `max_tokens=32000`、上限 128000；`CLAUDE_CODE_MAX_OUTPUT_TOKENS` 可覆盖，实测请求里生效。【文档】[env-vars § CLAUDE_CODE_MAX_OUTPUT_TOKENS](https://code.claude.com/docs/en/env-vars)；【实测】设 8192 后 body `max_tokens=8192`。

**约束：`CLAUDE_CODE_MAX_CONTEXT_TOKENS` 是全局单值，不是 per-model。** 如果 tcamp 同时映射 opus/sonnet/haiku 三个不同窗口的模型，只能取最小值（避免大窗口模型被过早压缩、小窗口模型溢出）或默认只推一个主模型。`modelSettings.autoCompactWindow` 是 per-model 的，但它不能扩大 Claude Code 假设的 context window；不要用它替代 `MAX_CONTEXT_TOKENS`。

## 5. 最小可用配置

下面这份配置已在 2.1.316 + 本地 mock 上端到端跑通（`is_error:false`、`result:"OK"`）。真实 tokencamp 上还缺“至少一个 Anthropic deployment”这个平台前提。

```json
{
  "$schema": "https://json.schemastore.org/claude-code-settings.json",
  "env": {
    "ANTHROPIC_BASE_URL": "https://api.tokencamp.com",
    "ANTHROPIC_AUTH_TOKEN": "sk-REPLACE_WITH_TOKENCAMP_KEY",
    "ANTHROPIC_API_KEY": "",

    "ANTHROPIC_MODEL": "deepseek/deepseek-v4-pro",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "deepseek/deepseek-v4-pro",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "qwen/qwen3-coder-480b",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "zhipu/glm-5.3",
    "ANTHROPIC_DEFAULT_FABLE_MODEL": "deepseek/deepseek-v4-pro",

    "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "1048576",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "8192",

    "ENABLE_TOOL_SEARCH": "false",
    "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS": "1",
    "MAX_THINKING_TOKENS": "0"
  }
}
```

逐项说明：

- `ANTHROPIC_BASE_URL`：origin，**不带 `/v1`**。
- `ANTHROPIC_AUTH_TOKEN`：tokencamp `sk-…` key；Claude Code 发 Bearer。
- `ANTHROPIC_API_KEY": ""`：压掉 shell/其他配置里可能存在的 API key，保证不发 `x-api-key`。
- 四个 `ANTHROPIC_DEFAULT_*_MODEL`：把 Claude Code 的内置 alias 全部钉到 tokencamp 模型；否则 `sonnet`/`opus`/`haiku`/`fable` 会发 `claude-*` 并 404。`HAIKU` 还负责后台任务。
- `CLAUDE_CODE_MAX_CONTEXT_TOKENS`：按实际模型填；示例 1,048,576 对应 `deepseek/deepseek-v4-pro`。多个槽位窗口不同时取最小值。
- `CLAUDE_CODE_MAX_OUTPUT_TOKENS`：按模型实际上限填；不确定时先用 8192，再按上游 400 `max_tokens_exceeded` 调整。Claude Code 未知模型默认 32000，可能超过部分模型上限。
- `ENABLE_TOOL_SEARCH=false`：关掉 MCP tool search；tokencamp 不透传 `anthropic-beta`，且不建模 `tool_reference` 块。
- `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS=1`：去掉 `context_management`、beta tool schema 字段（`strict`/`defer_loading`）等；tokencamp 不转发 beta header，留着 body 字段会被上游 400。
- `MAX_THINKING_TOKENS=0`：让 Claude Code 不发 `thinking`；tokencamp 不建模 thinking/redacted_thinking 块。**这会关闭 reasoning**；如果所选模型不需要 thinking，这是 v1 的兼容档。平台补类型后可移除。

推荐再加（非最小集）：

```json
{
  "env": {
    "CLAUDE_CODE_ATTRIBUTION_HEADER": "0",
    "CLAUDE_CODE_SUBAGENT_MODEL": "deepseek/deepseek-v4-pro"
  }
}
```

- `CLAUDE_CODE_ATTRIBUTION_HEADER=0`：Claude Code 默认在 system prompt 前加一段 attribution block；gateway 后面是非 Anthropic 上游，这段会进 prompt、增加 token，且对 prompt cache 不友好。实测 system block 从 3 个降到 2 个。【文档】[Gateway protocol § system prompt attribution](https://code.claude.com/docs/en/llm-gateway-protocol#system-prompt-attribution-block)；【实测】
- `CLAUDE_CODE_SUBAGENT_MODEL`：避免子代理用内置 Claude 模型；可选。

如果网络只允许访问 tokencamp（不允许直连 Anthropic），再加：

```json
{
  "env": {
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1"
  }
}
```

它关闭版本检查、遥测、release notes 等非必要流量，但也**关闭自动更新**；不是所有用户都需要。【文档】[Connect § turn off traffic outside the gateway](https://code.claude.com/docs/en/llm-gateway-connect#turn-off-traffic-outside-the-gateway-path)

## 6. 可选开关矩阵

| 变量 | tokencamp 下 | 原因 |
| --- | --- | --- |
| `ENABLE_TOOL_SEARCH` | **设 `false`** | 2.1.304+ 在自定义 `ANTHROPIC_BASE_URL` 后默认开启 tool search；tokencamp 不转发 beta header，且 `tool_reference` 未建模。不关会 400/多一轮重试。【文档】[env-vars](https://code.claude.com/docs/en/env-vars)、[changelog 2.1.304](https://code.claude.com/docs/en/changelog)；【实测】 |
| `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS` | **设 `1`** | 去掉 `context_management`、beta tool fields、structured outputs；tokencamp 只注入固定 `anthropic-version`，不转发客户端 `anthropic-beta`。【文档】[Gateway protocol § disable pre-release](https://code.claude.com/docs/en/llm-gateway-protocol#disable-pre-release-capabilities)；【源码】[upstream/anthropic.rs L124-L130](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/upstream/anthropic.rs#L124-L130) |
| `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY` | **不设** | discovery 只保留 id 含 `claude`/`anthropic` 的模型；tokencamp catalog id 全部被过滤。【文档】[Gateway protocol § model discovery](https://code.claude.com/docs/en/llm-gateway-protocol#model-discovery)；【实测】 |
| `CLAUDE_CODE_SIMPLE_SYSTEM_PROMPT` | **默认不设**；小 context 模型可设 `1` | 只影响 system prompt/工具描述长度；不是 gateway 兼容开关。`1` 可省 token，但可能降低工具使用质量。`0` 强制完整 prompt。【文档】[env-vars](https://code.claude.com/docs/en/env-vars) |
| `CLAUDE_CODE_SKIP_FAST_MODE_ORG_CHECK` | **不设** | Fast mode 是 Anthropic 专属，tokencamp 不提供；设了只让 `/fast` 的提示消失，不会让 fast mode 可用，反而可能让用户以为可用。【文档】[Connect § troubleshooting](https://code.claude.com/docs/en/llm-gateway-connect#troubleshoot-gateway-errors) |
| `ANTHROPIC_SMALL_FAST_MODEL` | **不设** | 已废弃；实测对 `haiku` alias 不生效，必须用 `ANTHROPIC_DEFAULT_HAIKU_MODEL`。【文档】[model-config § note](https://code.claude.com/docs/en/model-config#environment-variables)；【实测】 |
| `ANTHROPIC_DEFAULT_*_MODEL_SUPPORTED_CAPABILITIES` | **不设/不要依赖** | 只在 Bedrock/Vertex/Foundry 等 provider 配置生效；`ANTHROPIC_BASE_URL` gateway 后无效。因此 Claude Code 会把 tokencamp id 当“未知新模型”，发 adaptive thinking / mid-conversation system messages。【文档】[Gateway protocol § feature pass-through](https://code.claude.com/docs/en/llm-gateway-protocol#feature-pass-through) |
| `MAX_THINKING_TOKENS` | **v1 设 `0`** | 去掉 `thinking` 字段；tokencamp 不建模 thinking/redacted_thinking 块。关闭 reasoning 是 v1 的兼容代价。【文档】[env-vars](https://code.claude.com/docs/en/env-vars)；【实测】 |
| `CLAUDE_CODE_ATTRIBUTION_HEADER` | 建议 `0` | 非 Anthropic 上游不需要 attribution block；省 token，避免污染 prompt cache。【文档】[Gateway protocol](https://code.claude.com/docs/en/llm-gateway-protocol#system-prompt-attribution-block)；【实测】 |
| `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` | 可选 `1` | 只在 egress 受限时需要；关闭自动更新。【文档】[Connect](https://code.claude.com/docs/en/llm-gateway-connect#turn-off-traffic-outside-the-gateway-path) |
| `CLAUDE_CODE_ENABLE_FINE_GRAINED_TOOL_STREAMING` | 默认不设 | 自定义 base URL 下默认关闭；只有上游明确支持 fine-grained tool streaming 时才设 `1`。【文档】[Gateway protocol § feature pass-through](https://code.claude.com/docs/en/llm-gateway-protocol#feature-pass-through) |
| `apiKeyHelper` | 可选 | key 轮换/保险库场景；helper 输出会同时进 Authorization 和 x-api-key，tokencamp 读前者。固定 key 用 `ANTHROPIC_AUTH_TOKEN` 更简单。【文档】[Connect § apiKeyHelper](https://code.claude.com/docs/en/llm-gateway-connect#rotate-credentials-with-apikeyhelper) |

## 7. 协议行为与已知坑

### 7.1 流式：可用

tokencamp `/v1/messages` 是 near-passthrough：上游 SSE 事件按原顺序 relay，`message_start` 只改 `message.model`，`message_delta` 的 `usage` 加 `cost`，其他事件原样；下游每 15 秒发 `: keep-alive` comment，idle 120 秒超时；首字节后不重试，中流失败发 Anthropic `event: error`。【源码】[messages.rs L1-L19](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L1-L19)、[messages.rs L385-L427](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L385-L427)、[upstream/anthropic.rs L596-L646](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/upstream/anthropic.rs#L596-L646)、[config.rs L175-L180](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/config.rs#L175-L180)

实测 Claude Code 默认 `stream: true`；mock 返回完整 `message_start → content_block_start → content_block_delta → content_block_stop → message_delta → message_stop`，`claude -p` 成功。tokencamp 的 keep-alive 是 SSE comment，Claude Code 的 byte watchdog 能收到字节；不需要额外 `ping` event。【实测】

注意：**流式成功不代表 thinking 可用**。SSE relay 不解析 content block，所以 thinking 块能流过去；但 Claude Code 下一轮会把 thinking 块放回 `messages`，tokencamp 请求反序列化会失败（§7.4）。【推断】由 `messages_event_stream` 原样 relay + §12.3 的请求类型校验推出；未跑真实 thinking 上游。

### 7.2 tool use：基础可用，tool search 不可用

- `tools` / `tool_choice` 在 `MessagesRequest` 里是 `serde_json::Value`，原样转发；`tool_use` / `tool_result` content block 已建模，可转发。【源码】[core/anthropic.rs L17-L55](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L17-L55)、[L92-L118](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L92-L118)
- Claude Code 的 tool search 会发 `defer_loading` 工具字段和 `tool_reference` content block；tokencamp 不建模 `tool_reference`，且不转发 `anthropic-beta`。必须 `ENABLE_TOOL_SEARCH=false` + `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS=1`。
- 即便不关，Claude Code 会在 400/422 后自动重试一次不带 tool search；但首轮多一次失败请求，且 `tool_reference` 在历史里仍可能触发 tokencamp 400。不要依赖重试。【文档】[Gateway protocol § retry without tool search](https://code.claude.com/docs/en/llm-gateway-protocol#retry-without-tool-search)；【源码】[core/anthropic.rs L92-L118](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L92-L118)

### 7.3 `cache_control`：system/tools 保留，message 块丢弃

- Claude Code 会把 `cache_control` 打在 `system` blocks 和 `messages` entries 上。【文档】[Gateway protocol § feature pass-through](https://code.claude.com/docs/en/llm-gateway-protocol#feature-pass-through)
- tokencamp 的 `system` 是 `Vec<serde_json::Value>`，`tools` 是 `Vec<Value>`，所以 cache_control 保留。【源码】[core/anthropic.rs L66-L74](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L66-L74)
- `messages[].content[]` 是 `ContentBlock` 枚举，只有 `text{text}` / `image{source}` / `tool_use{...}` / `tool_result{...}`，**没有 `flatten extra`**；serde 会静默丢弃未知的 `cache_control` 字段，再序列化时不再出现。【源码】[core/anthropic.rs L92-L118](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L92-L118)
- 实测 round-trip：message 里的 `{"type":"text","text":"hi","cache_control":…}` 变成 `{"type":"text","text":"hi"}`；system/tools 里的 cache_control 保留。【实测/typecheck】

后果：Claude Code 的多轮对话前缀缓存只能在 system/tools 上命中；user/assistant 消息的 cache 断点被删，`cache_read_input_tokens` 会偏低，成本/延迟上升。Anthropic 兼容上游可能仍按自己的前缀缓存工作，但 tokencamp 的 wire 层丢了显式断点。

### 7.4 thinking / effort / mid-conversation system

这是 v1 最大的 wire 兼容坑。

**(a) thinking block 未建模。** `ContentBlock` 没有 `Thinking` / `RedactedThinking` 变体。实测：

- 非流式响应含 `{"type":"thinking",...}` → `serde` 报 `unknown variant 'thinking', expected one of 'text', 'image', 'tool_use', 'tool_result'`，tokencamp 只能 502/400。
- 请求 `messages[].content` 含 thinking block → `MessagesContent` untagged 反序列化失败，tokencamp 400。
- `redacted_thinking` / `tool_reference` / `document` / `server_tool_use` 同样报 unknown variant。【实测/typecheck】

Claude Code 对不认识的 model id 默认发 `thinking: {"type":"adaptive"}`，并把上游返回的 thinking block 在后续轮次回传。因此：**只要上游返回 thinking，下一轮就会坏。** v1 必须 `MAX_THINKING_TOKENS=0`（实测请求里 `thinking` 字段消失）；或者 tokencamp 先补 `thinking`/`redacted_thinking` 类型。

**(b) `output_config.effort` 仍会发。** `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS=1` 不删除 `output_config.effort`；实测未知模型发 `{"effort":"high"}`。如果上游拒绝，Claude Code 2.1.310+ 会识别 4xx 文案并重试不带 effort；tokencamp 的 4xx 上游 message 会透传（§7.5），所以通常能自愈，但首轮多一次 400。没有环境变量能完全去掉 `output_config`；这是 Claude Code 当前行为。【文档】[Gateway protocol § automatic retry](https://code.claude.com/docs/en/llm-gateway-protocol#automatic-retry-and-error-forwarding)；【实测】

**(c) mid-conversation `role:"system"`。** Claude Code 对未知/新模型会在 `messages` 里发 `role:"system"` 的条目；实测第一轮 body 就有 `{"role":"system","content":"# Environment…"}`。tokencamp 的 `MessagesMessage.role` 是 `String`，会转发；Anthropic-native 上游通常只接受 user/assistant，会 400 `Unexpected role "system"`；Claude Code 会重试一次去掉这些条目。tokencamp 的 4xx message 透传让这个重试能匹配；首轮多一次失败请求。【文档】[Gateway protocol § feature pass-through](https://code.claude.com/docs/en/llm-gateway-protocol#feature-pass-through)；【源码】[core/anthropic.rs L76-L82](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L76-L82)；【实测】

### 7.5 错误信封与重试

- 同步错误是 Anthropic-native 信封：`{"type":"error","error":{"type":..., "message":..., "code":...}}`；`code` 是 tokencamp 权威字段，`type` 是 best-effort 映射。Claude Code 会忽略未知 `code` 字段，按 `type`/status/message 处理。【源码】[messages.rs L653-L682](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L653-L682)、[routes/mod.rs L550-L590](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/mod.rs#L550-L590)
- **4xx 的上游 message 会透传**（这是 Claude Code 能力拒绝重试的关键）；5xx 被替换成 generic 文案（provider 身份不泄露）。【源码】[normalize.rs L170-L205](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/upstream/normalize.rs#L170-L205)
- 不返回 `x-should-retry`，也不返回 `anthropic-ratelimit-unified-*`；Claude Code 的重试主要看 status/Retry-After，usage-limit UI 不可用。tokencamp 只在 429 设 `Retry-After`。【源码】[routes/mod.rs L550-L590](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/mod.rs#L550-L590)
- 中流错误是 `event: error`，形状同上；Claude Code 会中断本轮并显示错误，不会自动续传；tokencamp 首字节后不重试。【源码】[messages.rs L1-L19](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L1-L19)

### 7.6 token counting

tokencamp 没有 `/v1/messages/count_tokens` 路由（router 里只有 `/v1/messages`、`/v1/models` 等）。【源码】[routes/mod.rs L52-L65](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/mod.rs#L52-L65) Claude Code 会退回字符估算，`/context` 显示近似值；不影响主流程，但 context 管理精度下降。【文档】[Gateway protocol § feature pass-through](https://code.claude.com/docs/en/llm-gateway-protocol#feature-pass-through)

### 7.7 其他 content 类型

- `image` 已建模（source 透传），Claude Code 的图片输入可过；但 tokencamp guardrails 的输入扫描不覆盖图片/二进制（平台已知 v1 gap），不是 Claude Code 适配器问题。【源码】[core/anthropic.rs L92-L104](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L92-L104)、[messages.rs L51-L58](https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L51-L58)
- `document`（PDF）未建模；Claude Code 读 PDF 时会发 document content block，tokencamp 400。v1 应告知用户不要在 Claude Code 里读 PDF，或平台补类型。
- `server_tool_use` / `web_search_tool_result` 未建模；Claude Code 的 server-side WebSearch 结果可能触发。tokencamp 不提供 Anthropic server tools，建议用户不要启用依赖这些的 workflow。

## 8. 非破坏性写入策略

tcamp 的写文件契约建议：

1. **默认写 user scope**：`~/.claude/settings.json`。它覆盖所有项目、不需要 trust、后台 agent 也能读到。项目级配置只在用户显式选择 per-project 时写 `.claude/settings.local.json`；**绝不把 key 写进 `.claude/settings.json`**（会进 git）。【文档】[Connect § settings file](https://code.claude.com/docs/en/llm-gateway-connect#set-in-a-settings-file)
2. **JSON read-modify-write**：解析现有 JSON（strict JSON，不允许注释/尾逗号），只合并 `env` 下自己的 key；保留其他所有 key（包括未知 key）。Claude Code 自己也会保留未知 key，但会重排/重格式化整个文件【实测】；tcamp 不能做文本 patch。
3. **原子写**：同目录临时文件 + `rename`，最终 mode `0600`；写前备份（如 `settings.json.tcamp-backup-<ts>`）；写后重新解析验证。文件不存在时创建目录 `~/.claude`（0700）。
4. **并发**：Claude Code 可能在 `/config`、`/model` 时写同一文件；tcamp 写入前重新读取，写入后比较/重试，避免覆盖会话期间的用户改动。不要持有长期锁。
5. **不要写 `~/.claude.json`**：它保存账号、MCP、trust、global config；写坏会导致配置错误/重置提示。【文档】[Settings](https://code.claude.com/docs/en/settings)
6. **managed settings 检测**：读 macOS/Linux 的 managed-settings.json，检测 `env.ANTHROPIC_BASE_URL`、`allowedProviders`、`forceLoginMethod`、`forceLoginOrgUUID`。managed 优先级最高，tcamp 写 user settings 不能覆盖；若 managed 把 base URL pin 到别的值，或 `forceLoginMethod` 与 gateway 凭据冲突，直接报错并让用户找管理员。【文档】[Managed settings](https://code.claude.com/docs/en/managed-settings)、[Connect § troubleshooting](https://code.claude.com/docs/en/llm-gateway-connect#troubleshoot-gateway-errors)
7. **key 存储**：`ANTHROPIC_AUTH_TOKEN` 是明文写进 settings；至少 mode 0600，并在 `tcamp` 输出里打码。更安全的是 `apiKeyHelper` 指向 `tcamp` 的 credential 命令/系统 keychain；helper 输出会同时进两个 header，tokencamp 只读 Bearer，兼容。
8. **可撤销**：记录 tcamp 写入的 key 集合（sidecar manifest 或 `tcamp` 自己的配置），`tcamp uninstall` 只删自己写的 key，不整文件删除。

## 9. 启动与版本检测

### 9.1 可执行文件路径

| 平台/安装 | 典型路径 | 依据 |
| --- | --- | --- |
| macOS/Linux native（推荐） | `~/.local/bin/claude` → `~/.local/share/claude/versions/…` | 【文档】[Setup § auto-updates](https://code.claude.com/docs/en/setup#auto-updates) |
| Homebrew cask | `/opt/homebrew/bin/claude`（Apple Silicon）或 `/usr/local/bin/claude`（Intel） | 安装方式推断；`command -v claude` 优先 |
| npm global | `$(npm prefix -g)/bin/claude` | 【文档】[Setup § install with npm](https://code.claude.com/docs/en/setup#install-with-npm) |
| Windows native | `%USERPROFILE%\.local\bin\claude.exe` | 【文档】[Setup](https://code.claude.com/docs/en/setup)（v1 若只做 macOS/Linux 可忽略） |

检测顺序：`command -v claude` → `~/.local/bin/claude` → `/opt/homebrew/bin/claude` → `/usr/local/bin/claude` → npm prefix。不要 `pkill`/改 PATH；只读检测。

### 9.2 版本

- `claude --version` / `claude -v` 输出 `2.1.316 (Claude Code)`；解析第一段 semver。【文档】[CLI reference](https://code.claude.com/docs/en/cli-reference)；【实测】
- **建议 supported floor：`>=2.1.304`**。这是 tool search 在自定义 base URL 后默认开启的版本，也是本报告实测行为的分界；低于它的版本可能能跑，但 tool search/retry 行为不同，未测。
- 单特性 floor：`ANTHROPIC_CUSTOM_MODEL_OPTION` 2.1.78；`ANTHROPIC_DEFAULT_MODEL` 2.1.236；`modelPicker` 2.1.242；discovery 同时发两个凭据 header 2.1.248；discovery `description` 2.1.257；`CLAUDE_CODE_GATEWAY_MODEL_DISCOVERY_TIMEOUT_MS` 2.1.269；`claude auth status --json` 的 `configDirectory` 2.1.268；`CLAUDE_CODE_DISABLE_STRUCTURED_OUTPUTS` 2.1.288；`modelPicker` 的 `behavesAs` 2.1.257；tool search 默认开启 2.1.304。【文档】[Changelog](https://code.claude.com/docs/en/changelog)
- 若 tcamp 只用 §5 的 env 变量（不用 `modelPicker`/`ANTHROPIC_DEFAULT_MODEL`），硬性下限可以更低；但建议产品上统一要求 2.1.304+，减少分支。

### 9.3 非交互 smoke test

`-p` 是非交互模式；退出码 0 成功、1 失败（实测坏 base URL 时 exit 1）。`--output-format json` 的 stdout 是纯 JSON；`[claude-code:unrecognized_model]` 警告走 **stderr**（不要把 stderr 合并进 JSON 解析）。【文档】[CLI reference](https://code.claude.com/docs/en/cli-reference)；【实测】

推荐 `tcamp doctor` 用 `--settings` 内联 JSON 做一次性验证，不写用户文件、不改配置：

```bash
claude --settings '{
  "env": {
    "ANTHROPIC_BASE_URL": "https://api.tokencamp.com",
    "ANTHROPIC_AUTH_TOKEN": "sk-…",
    "ANTHROPIC_API_KEY": "",
    "ANTHROPIC_MODEL": "deepseek/deepseek-v4-pro",
    "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "1048576",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "8192",
    "ENABLE_TOOL_SEARCH": "false",
    "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS": "1",
    "MAX_THINKING_TOKENS": "0"
  }
}' -p "Reply with OK" --output-format json
```

检查 stdout JSON 的 `"is_error": false` 和 `"result"`；实际会发完整 system/tools，不是 1-token 请求，成本很小但非零。若要零成本验证网关 auth/路径，先用 `curl` 打 `/v1/messages`；但 curl 不能验证 Claude Code 的 header/body 行为。

其他检测：

- `claude auth status --json`（v2.1.268+）：`authMethod` ∈ `none|claude.ai|oauth_token|api_key|api_key_helper|third_party`，`configDirectory`；exit 0 已登录、1 未登录。可用于识别“用户其实还在用 claude.ai 登录”。【文档】[CLI reference](https://code.claude.com/docs/en/cli-reference)
- `claude doctor`：只读安装/设置诊断，打印 `Managed settings (remote)`、`Auto-updates` 等；可用来提示 managed 策略/安装问题。实测可用，不写文件。【文档】[Setup § verify](https://code.claude.com/docs/en/setup#verify-your-installation)
- `claude -p` 不需要 trust 对话；交互式启动时 project/local 的 `env` 要 trust 后才应用。tcamp 默认写 user settings 可以绕过这个问题。【文档】[settings-reference § when applies](https://code.claude.com/docs/en/settings-reference#when-claude-code-applies-env-values)

## 10. 版本与日期 caveat

- 本报告所有 Claude Code 实测都在 **2.1.316**（2026-10-31 发布）上；Claude Code 的 model alias、tool search 默认值、beta header 集合、重试文案都可能随版本变。每次升级重跑 §9.3 的 smoke test。
- `ENABLE_TOOL_SEARCH` 在 2.1.304 改为自定义 base URL 后默认开启；在此之前默认可能是关闭/自动。tokencamp 的写法（显式 `false`）在两者上都安全。
- `ANTHROPIC_DEFAULT_MODEL` 2.1.236 才有；tcamp 不用它，避免版本依赖。
- `modelPicker` 2.1.242 才有；如果 tcamp 用它做 picker UX，需检查版本，否则退回 `ANTHROPIC_CUSTOM_MODEL_OPTION`（2.1.78+）。
- `CLAUDE_CODE_MAX_CONTEXT_TOKENS` 对未知 ID 的生效规则在 2.1.193/2.1.259 有修正；tokencamp id 不带 `@YYYYMMDD`，一般不受影响，但仍建议 2.1.304+。
- 客户文档的 base URL 是 provisional；上线前必须确认 `api.tokencamp.com` 是最终域名，否则 tcamp 写进 settings 的 URL 会过期。
- tokencamp-pro `config/default.yaml` 的零 Anthropic deployment 是 2026-10-31 的 HEAD 状态；生产 DB 可能通过 Control 加过 deployment，本报告无法验证。

## 11. 未决问题 / blockers

1. **生产/预发到底有哪些模型有 `provider_type: anthropic` deployment？** 这是 v1 的硬 blocker。默认 seed 0 个；请 ops 给出可用 catalog id 列表，并在 `/v1/models` 或文档中标注“支持 Anthropic Messages”。否则 tcamp 只能写配置，无法端到端工作。
2. **tokencamp 是否计划建模 thinking / redacted_thinking / tool_reference / document / server_tool_use？** 如果不做，v1 必须永久 `MAX_THINKING_TOKENS=0` + `ENABLE_TOOL_SEARCH=false`，并明确“Claude Code 不能用 reasoning / tool search / PDF”。
3. **tokencamp 是否计划转发 `anthropic-beta`？** 当前 `upstream/anthropic.rs` 只注入 `x-api-key` + 固定 `anthropic-version`，不转发客户端 beta。扩展上下文、interleaved thinking、tool search 都依赖 beta header；不转发就只能降级。
4. **`max output` 的权威来源**：catalog/config 没有字段；tcamp 需要用户输入/静态表，或平台在 `/feed/models`/`/v1/models` 补 `max_output_tokens`。
5. **`/v1/messages` 是否未来支持 `auto` / `@preset`？** 当前 OpenAPI 明确只支持 catalog id / family alias；如果支持，tcamp 可以少做模型映射，但也会让“auto”路由质量不可控。
6. **managed settings 策略**：客户环境可能用 `allowedProviders: ["customEndpoint"]` 把 base URL pin 死，或用 `forceLoginMethod` 禁止 gateway 凭据；tcamp 需要检测并给出明确错误，而不是写一个被覆盖的配置。
7. **`/v1/messages/count_tokens`**：不做的话 `/context` 只是估算；是否值得为 Claude Code 补一个精确计数端点？
8. **`apiKeyHelper` vs `ANTHROPIC_AUTH_TOKEN`**：tcamp 的 key 存哪里？明文 settings（简单）还是 helper + keychain（安全）？v1 建议先 AUTH_TOKEN，后续加 helper。
9. **Windows**：v1 是否支持？路径/安装方式不同；本报告只实测 macOS x86_64。
10. **base URL 最终域名**：`api.tokencamp.com` 目前是 placeholder；确认后再写死。

## 12. 引用与实验记录

### 12.1 Claude Code 官方文档

- Settings files and precedence — https://code.claude.com/docs/en/settings
  （文件 scope、优先级、`env` 块、写入/重载、`~/.claude.json`）
- All settings / settings reference — https://code.claude.com/docs/en/settings-reference
  （`env`、`model`、`modelPicker`、`env` 何时应用/忽略哪些变量）
- Environment variables — https://code.claude.com/docs/en/env-vars
  （`ANTHROPIC_API_KEY`、`ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_BASE_URL`、`ANTHROPIC_MODEL`、`ANTHROPIC_DEFAULT_*`、`ANTHROPIC_SMALL_FAST_MODEL`、`CLAUDE_CODE_MAX_CONTEXT_TOKENS`、`CLAUDE_CODE_MAX_OUTPUT_TOKENS`、`ENABLE_TOOL_SEARCH`、`CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY`、`CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS`、`CLAUDE_CODE_SIMPLE_SYSTEM_PROMPT`、`CLAUDE_CODE_SKIP_FAST_MODE_ORG_CHECK`、`MAX_THINKING_TOKENS`）
- Model configuration — https://code.claude.com/docs/en/model-config
  （alias、槽位、pin、context window、custom model option、modelPicker、capabilities）
- Connect Claude Code to an LLM gateway — https://code.claude.com/docs/en/llm-gateway-connect
  （credential 变量、header 映射、base URL 示例、settings env、trust、troubleshooting）
- Claude Code gateway compatibility guide — https://code.claude.com/docs/en/llm-gateway-protocol
  （`/v1/messages`、`/v1/models`、beta/body 配对、cache_control、error retry、model discovery、attribution）
- Other LLM gateways — https://code.claude.com/docs/en/llm-gateway
  （subscription/OAuth 与 gateway credential 的关系）
- Deploy managed settings — https://code.claude.com/docs/en/managed-settings
  （macOS/Linux managed-settings 路径、优先级）
- Advanced setup — https://code.claude.com/docs/en/setup
  （安装路径、`~/.local/bin/claude`、`claude --version`、`claude doctor`）
- CLI reference — https://code.claude.com/docs/en/cli-reference
  （`-p`、`--output-format`、`--settings`、`--version`、`auth status`）
- Claude Code changelog — https://code.claude.com/docs/en/changelog
  （2.1.304 tool search 默认、2.1.236 `ANTHROPIC_DEFAULT_MODEL`、2.1.242 `modelPicker`、2.1.248/257/269 discovery、2.1.310 retry）
- Settings JSON schema — https://json.schemastore.org/claude-code-settings.json
  （根 `additionalProperties: true`）

### 12.2 tokencamp 文档与源码

客户文档：

- Migrate to tokencamp — https://github.com/0xnicholas/tokencamp-www/blob/640d204/docs-site/guides/migration.mdx#L74-L81
  （Anthropic SDK base URL `/v1`、Claude Code-style clients 的说法；tcamp 不要照抄 base URL）
- Migrate to tokencamp（渲染页）— https://docs.tokencamp.com/guides/migration
- Install with a coding agent — https://github.com/0xnicholas/tokencamp-www/blob/640d204/docs-site/guides/install.mdx#L15-L40
  （`TOKENCAMP_API_KEY`、`/v1/chat/completions`、`auto`）
- Base URL placeholder — https://github.com/0xnicholas/tokencamp-www/blob/640d204/docs-site/snippets/base-url.mdx#L1-L4
- Presets & aliases — https://docs.tokencamp.com/cookbook/presets
  （`@preset/slug`、`~author/family-latest`、`fallbacks`）
- Response caching — https://docs.tokencamp.com/guides/caching
  （`/v1/messages` 不走 response cache；与 Claude Code 的 prompt cache 是两回事）

tokencamp-pro 源码（`25db44b`）：

- `crates/gateway/src/routes/messages.rs:1-19` — `/v1/messages` near-passthrough、SSE relay、错误信封
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L1-L19
- `crates/gateway/src/routes/messages.rs:199-223` — model gates：必须存在 `ProviderType::Anthropic` deployment
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L199-L223
- `crates/gateway/src/routes/messages.rs:385-427` — stream relay、keep-alive、idle timeout
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L385-L427
- `crates/gateway/src/routes/messages.rs:653-682` — Anthropic `error.type` 映射
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/messages.rs#L653-L682
- `crates/gateway/src/routes/direct.rs:399-402` — 只读 `Authorization`
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/direct.rs#L399-L402
- `crates/gateway/src/routes/direct.rs:698-719` — `/v1/messages` 模型解析：`~` family / exact lookup，无 preset/auto
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/direct.rs#L698-L719
- `crates/gateway/src/pipeline/auth.rs:54-60` — 只接受 `Bearer `
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/pipeline/auth.rs#L54-L60
- `crates/core/src/anthropic.rs:17-55` — `MessagesRequest`：`tools`/`thinking`/`metadata`/`extra` 透传
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L17-L55
- `crates/core/src/anthropic.rs:66-118` — `SystemPrompt`/`MessagesContent`/`ContentBlock`（无 thinking/tool_reference/document）
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L66-L118
- `crates/core/src/anthropic.rs:122-154` — `MessagesResponse`/`MessagesUsage`
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/core/src/anthropic.rs#L122-L154
- `crates/gateway/src/upstream/anthropic.rs:83-140` — 上游只加 `x-api-key` + 固定 `anthropic-version`；不转发 `anthropic-beta`
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/upstream/anthropic.rs#L83-L140
- `crates/gateway/src/upstream/anthropic.rs:596-646` — SSE 原样 relay
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/upstream/anthropic.rs#L596-L646
- `crates/gateway/src/routes/mod.rs:52-65` — 路由表（无 `count_tokens`）
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/mod.rs#L52-L65
- `crates/gateway/src/routes/mod.rs:510-548` — `ModelObject` 无 context/display/max output
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/mod.rs#L510-L548
- `crates/gateway/src/upstream/normalize.rs:170-205` — 4xx 上游 message 透传、5xx generic
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/upstream/normalize.rs#L170-L205
- `crates/gateway/src/routes/feed.rs:40-60` — `/feed/models` 有 `context_window`，无 max output
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/routes/feed.rs#L40-L60
- `crates/gateway/src/config.rs:319-347` — `ModelConfig` 只有 `context_window`
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/config.rs#L319-L347
- `crates/gateway/src/config.rs:680-701` — `ProviderType` 枚举（`anthropic` / `openai_compatible`）
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/crates/gateway/src/config.rs#L680-L701
- `config/default.yaml:75-100` — `deepseek/deepseek-v4-pro` 是 `openai_compatible`；全文件无 `provider_type: anthropic`
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/config/default.yaml#L75-L100
- `docs/openapi.yaml:1645-1648` — `/v1/messages` 的 model 只承诺 catalog id / family alias
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/docs/openapi.yaml#L1645-L1648
- `docs/openapi.yaml:1459-1461` — chat completions 的 model 支持 `auto` / `@preset`
  https://github.com/0xnicholas/tokencamp-pro/blob/25db44b/docs/openapi.yaml#L1459-L1461

### 12.3 本报告实验

环境：

```text
Claude Code 2.1.316 (Claude Code)
macOS 13.7.8 x86_64
npm @anthropic-ai/claude-code-darwin-x64@2.1.316
HOME=/tmp/tcamp-home（临时；未碰真实 ~/.claude）
mock gateway: 127.0.0.1:8765，记录 request line / headers / body，返回合法 Anthropic SSE 或 JSON
```

关键观察：

1. 默认无 `ANTHROPIC_MODEL` 时，Claude Code 发 `claude-opus-5-5`、`max_tokens=128000`、12 个 tools、`thinking: adaptive`、`output_config.effort: medium`、`context_management`，以及 8 个 `anthropic-beta` 值；路径 `POST /v1/messages?beta=true`。
2. `ANTHROPIC_AUTH_TOKEN` → 只有 `Authorization: Bearer`；`ANTHROPIC_API_KEY` → 只有 `x-api-key`；两者都设 → 两个 header；`ANTHROPIC_API_KEY=""` → 只有 Bearer。
3. `ANTHROPIC_BASE_URL=.../v1` → `POST /v1/v1/messages?beta=true`，`claude -p` exit 1。
4. `ANTHROPIC_MODEL` 在 settings `env` 块里覆盖 `"model": "sonnet"`；settings env 覆盖 shell export；`--model` 覆盖两者。
5. `--model sonnet/opus/haiku/fable` 分别发 `qwen/qwen3-coder-480b` / `deepseek/deepseek-v4-pro` / `zhipu/glm-5.3` / `deepseek/deepseek-v4-pro`（按 §5 的映射）。
6. 只设 `ANTHROPIC_SMALL_FAST_MODEL` + `--model haiku` → `claude-haiku-5-5`（SMALL_FAST 被忽略）。
7. `--model 'qwen/qwen3-coder-480b[1m]'` → 实际发 `qwen/qwen3-coder-480b`；`~deepseek/...`、`@preset/cheap-chat`、`auto` 原样发出。
8. 开启 `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1`：GET `/v1/models?limit=1000`；mock 返回 4 个模型，`~/.claude/cache/gateway-models.json` 只缓存 `my-gateway-claude-sonnet-5`（唯一含 `claude` 的 id）。
9. 交互式 `/config verbose=true` 后 settings 文件被重写：新增 `"verbose": true`，保留未知 `"tcampUnknownKey"`，整体重排/2 空格缩进。
10. `claude --settings '<inline JSON>' -p ...` 可用，不写文件；`-p` 的 stdout 是纯 JSON，`[claude-code:unrecognized_model]` 在 stderr。
11. 最终 §5 配置 + mock：`is_error:false`、`result:"OK"`；body keys `max_tokens/messages/metadata/model/output_config/stream/system/tools`；`thinking` 无、`ToolSearch` 无、`system` 2 块、`max_tokens=8192`；仍发 `output_config.effort:"high"` 和 3 个 beta header（`claude-code-20250219,interleaved-thinking-2025-05-14,effort-2025-11-24`）。

tokencamp-core wire type 校验（`/tmp/tcamp-typecheck`）：

```text
REQUEST_ERR data did not match any variant of untagged enum MessagesContent   # messages 含 thinking block
CACHE_OK {"model":"…","messages":[{"role":"assistant","content":[{"type":"text","text":"hi"}]}],"max_tokens":1024}  # cache_control 被丢弃
RESP_ERR unknown variant `thinking`, expected one of `text`, `image`, `tool_use`, `tool_result`
ERR unknown variant `tool_reference`, expected one of `text`, `image`, `tool_use`, `tool_result`
ERR unknown variant `document`, expected one of `text`, `image`, `tool_use`, `tool_result`
ERR unknown variant `server_tool_use`, expected one of `text`, `image`, `tool_use`, `tool_result`
ERR unknown variant `redacted_thinking`, expected one of `text`, `image`, `tool_use`, `tool_result`
```

system/tools 的 `cache_control` round-trip：

```json
{"model":"deepseek/deepseek-v4-pro","messages":[{"role":"user","content":"hi"}],"max_tokens":1024,
 "system":[{"cache_control":{"type":"ephemeral"},"text":"sys","type":"text"}],
 "tools":[{"cache_control":{"type":"ephemeral"},"description":"read","input_schema":{"type":"object"},"name":"Read"}]}
```

实验边界：

- 没有 tokencamp API key；`api.tokencamp.com` 在本次网络环境 10s 超时。未对真实 tokencamp 发请求。
- mock 只模拟 wire 形状，不验证上游模型行为、计费、鉴权失败文案、真实 SSE 时序。
- 所有 Claude Code 实验使用临时 HOME；未读取/修改用户真实 `~/.claude`。实验后 mock 进程已停止。
