# Pi 适配器契约（ticket #14）

- 状态：research 完成，2026-11-01
- 被测版本：`pi` 1.4.0（macOS x86_64；`~/.local/bin/pi` → `~/.pi/agent/bin/pi`；Node v22.23.2）
- 上游源码：`earendil-works/pi@2c8d1feb`（2026-11-01；`packages/coding-agent/package.json` 版本 1.4.0；最新 release 为 `v1.4.0`，2026-10-21）
- 平台侧：`tokencamp-pro@25db44b`（2026-10-31）；客户文档 `tokencamp-www@640d204`（2026-10-22）
- 范围：`tcamp` v1 的 Pi adapter；纯客户端，不改 tokencamp-pro
- 结论日期：2026-11-01（Pi 迭代很快，见 §11）

## 0. 结论（Verdict）

**GO（当前默认 catalog）／条件 GO（Anthropic deployment 模型）。**

Pi 1.4.0 今天就能接 tokencamp：写 `~/.pi/agent/models.json` 里的一个
`providers.tokencamp`，`api = "openai-completions"`，
`baseUrl = "https://api.tokencamp.com/v1"`，key 用 `"$TOKENCAMP_API_KEY"`。
Pi 会向 `/v1/chat/completions` 发 `Authorization: Bearer`、`stream: true`
和标准 OpenAI tools；tool loop 第二轮的 `assistant.tool_calls` + `role: "tool"`
+ `tool_call_id` 也都在 tokencamp 的 `ChatRequest` 模型内。**不需要扩展、Pi 包或代理。**

必须写进 v1 契约的四个限制：

1. **不要用 `api: "openai-responses"`。** tokencamp 的 `/v1/responses` 是
   stateless 子集，明确拒绝 `function_call_output`（以及 `reasoning`、`input_image`、
   `previous_response_id` 等）。Pi 的 tool loop 第二轮就会 400。【源码】
   `tokencamp-pro/crates/core/src/responses.rs:96-99`、
   `crates/gateway/src/routes/responses.rs:1728-1731`。
2. **`/v1/messages` 今天不能作为默认路径。** tokencamp 的 `/v1/messages` 要求模型至少
   有一个 `provider_type: anthropic` deployment；当前 `config/default.yaml` 的 7 个
   deployment 全是 `openai_compatible`，**0 个 Anthropic**。当前 seed 的任何模型走
   `/v1/messages` 都会 404 `model_not_found`（文案带
   `no anthropic deployment for /v1/messages`）。`tokencamp/auto` 和
   `@preset/...` 也不能走 `/v1/messages`：direct endpoint 没有 preset 层，auto
   没有 deployments。【源码】`crates/gateway/src/routes/messages.rs:199-215`、
   `crates/gateway/src/routes/direct.rs:814-818`、`config/default.yaml:797-801`。
3. **chat skin 对 Anthropic deployment 会静默丢 tools。** tokencamp 的
   chat→Anthropic adapter 明确丢弃 `tools` / `tool_choice`、`role: tool`、assistant
   `tool_calls`，流式只发 text delta。若平台以后把某模型接到 Anthropic deployment，
   不能继续让 Pi 用 `openai-completions` 访问它；要对该模型单独写
   `api: "anthropic-messages"` + origin baseUrl + `authHeader: true`，且只在确认该模型
   有 Anthropic deployment 时写。【源码】
   `crates/gateway/src/upstream/anthropic.rs:1-12, 244-247`。
4. **Anthropic Messages 路径的 baseUrl 是 origin，不带 `/v1`。** Pi 用 Anthropic SDK，
   会在 baseUrl 后拼 `/v1/messages`；写 `.../v1` 会变成 `/v1/v1/messages`。chat 路径相反，
   OpenAI SDK 的 baseUrl 必须带 `/v1`。【实测】

一句话给 v1：**先把 `models.json` + `openai-completions` 做对；把
`anthropic-messages` 作为“模型有 Anthropic deployment 时才启用”的 per-model 覆盖，
不要把它当成默认或 auto 的路径。**

## 1. 方法与证据等级

- Pi 侧：在临时 `PI_CODING_AGENT_DIR`（`/tmp/tcamp-pi-agent*`）下运行本机 `pi` 1.4.0，
  配本地 mock gateway（只绑 `127.0.0.1:18901-18904`），记录 request line、关键 header
  和完整 body；未读取、未修改用户真实 `~/.pi/agent`，**未读 `auth.json`**。
- 平台侧：读 `tokencamp-pro@25db44b` 源码与 `config/default.yaml`；读
  `tokencamp-www@640d204` 文档。没有 tokencamp API key，**未对真实网关做端到端请求**；
  平台侧结论是源码/契约级。
- 引用标记：`【文档】` 官方文档；`【源码】` 源码；`【实测】` 本报告实验；
  `【推断】` 由类型/代码直接推出但未跑真实网关。

实验命令的固定前缀：

```bash
export PI_CODING_AGENT_DIR=/tmp/tcamp-pi-agent
export PI_OFFLINE=1 PI_TELEMETRY=0
export PATH="$HOME/.local/bin:$PATH"
```

## 2. 配置位置与优先级

### 2.1 文件表

| 路径 | 作用 | 谁写 | 依据 |
| --- | --- | --- | --- |
| `~/.pi/agent/settings.json` | 用户级 settings：默认模型、thinking、工具、包、主题等 | Pi；tcamp 只在缺失时补 `defaultProvider` / `defaultModel` | 【文档】`docs/configuration.md:13` |
| `~/.pi/agent/models.json` | **自定义 provider / model / modelOverrides 的唯一入口** | 只由用户或 tcamp 写；Pi 只读 | 【文档】`docs/configuration.md:16`；【源码】`core/model-config.ts:168-185` |
| `~/.pi/agent/auth.json` | `/login` 保存的 API key / OAuth token | Pi；**tcamp 不读不写** | 【文档】`docs/providers.md:12,18` |
| `~/.pi/agent/models-store.json` | 远程 model catalog 缓存（pi.dev / OpenRouter 等） | Pi；**不是配置，不要手改** | 【源码】`core/model-runtime.ts:223-227` |
| `<project>/.pi/settings.json` | 项目级 settings；trust 后生效，覆盖用户级 | Pi / 用户 | 【文档】`docs/configuration.md:3,30`；`docs/settings.md:3` |
| `<project>/.pi/models.json` | **不读**；Pi 没有项目级 models.json | — | 【源码】`core/model-runtime.ts:220-222`；【实测】 |
| `$PI_CODING_AGENT_DIR/models.json` | `PI_CODING_AGENT_DIR` 覆盖 agent dir 后的实际路径 | 同上 | 【文档】`docs/environment-variables.md:81`；【源码】`src/config.ts:629-636` |
| `~/.local/bin/pi` → `~/.pi/agent/bin/pi` | installer 的 PATH 入口（本机布局） | installer | 【实测】；【文档】`docs/quickstart.md:9-23` |
| `$(npm prefix -g)/bin/pi` | npm 全局安装的入口 | npm | 【源码】`package.json:9-11` |

`models-store.json` 的实际内容是按 provider 分组的 model catalog 缓存（本机看到
`xiaomi-token-plan-cn` / `kimi-coding` 等条目，带 `etag` / `checkedAt` / `baseUrl`），
由 `FileModelsStore` 写入；自定义 provider 不会写进去。【源码】
`core/models-store.ts:47-56`、`core/model-runtime.ts:223-227`。

### 2.2 优先级

| 维度 | 优先级（高 → 低） | 依据 |
| --- | --- | --- |
| 模型选择 | `--models` scoped 列表 > settings 的 `defaultProvider`+`defaultModel` > 第一个有凭据的可用模型 | 【源码】`core/model-resolver.ts:545-600` |
| settings | 项目 `.pi/settings.json` > 用户 `~/.pi/agent/settings.json`（项目需 trust） | 【文档】`docs/settings.md:3`、`docs/configuration.md:3` |
| 凭据 | `--api-key` > `auth.json` 存储凭据 > `models.json` 的 `apiKey` > provider 环境变量/ambient | 【文档】`docs/models.md:23` |
| 模型模式 | `--provider` 把 `--model` 限定到该 provider；`--model` 覆盖默认；`--thinking` 覆盖 `:level` 后缀 | 【文档】`docs/cli.md:55-90` |

**没有通用的 `PI_API_KEY`。** `PI_*` 变量是进程控制（`PI_CODING_AGENT_DIR`、
`PI_OFFLINE`、`PI_TELEMETRY`、`PI_SKIP_VERSION_CHECK` 等）；provider key 要自己在
`models.json` 的 `apiKey` 里用 `$NAME` 引用，或走 `--api-key` / `auth.json`。
【文档】`docs/environment-variables.md:75-100`。

### 2.3 `pi config` 与 `pi auth` 到底做什么

| 命令 | 行为 | 对 tcamp 的含义 |
| --- | --- | --- |
| `pi config` | 打开 TUI，启用/禁用 package resources；`-l` 编辑项目 `.pi/settings.json`；Tab 切 global/project | **不编辑 provider/model**；tcamp 不要把它当配置入口。【实测】`pi config --help`；【文档】`docs/cli.md:260-267` |
| `pi auth check` | 打印 `ready` / `not_ready` / `invalid`，exit `0` / `1` / `2`；`--json` 输出结构化结果 | 检测凭据是否配置；**ready 只代表“解析到了凭据”，不验证 key 是否有效**。【文档】`docs/cli.md:303-326`；【源码】`src/main.ts:198-206` |
| `pi auth print-api-key` | 打印解析后的明文 key | tcamp 不要用；会把 secret 带到 stdout。【文档】`docs/cli.md:311-326` |
| `pi auth print-bearer-token` | 打印 OAuth bearer token | 只对 OAuth provider 有意义；tokencamp API key 不适用。 |
| `/login`（交互式） | 选择 provider，走 OAuth 或 API-key 流程，写入 `auth.json` | 用户自己的持久化路径；tcamp 不写 `auth.json`。 |

`pi auth` 没有“登录/设置 key”的子命令；它只做 check/print。真正的登录是交互式
`/login`。【文档】`docs/providers.md:8-18`。

## 3. 自定义 provider 的准确形状

### 3.1 `providers.<id>` 字段

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `name` | string | 否 | 显示名。 |
| `baseUrl` | string | 自定义 provider 必填 | API base。**chat 路径必须含 `/v1`；Anthropic Messages 路径必须只写 origin。** |
| `apiKey` | string | 建议必填 | 字面量、`$NAME` / `${NAME}` 环境插值、或 `!command` 输出。tokencamp 用 `"$TOKENCAMP_API_KEY"`。 |
| `api` | string | 自定义 provider 必填 | `openai-completions` / `openai-responses` / `anthropic-messages` / `google-generative-ai` 等。 |
| `headers` | object | 否 | 额外请求头；值同样支持 `$NAME` / `!command`。不建议用它替代 `apiKey`（见 §4.3）。 |
| `compat` | object | 否 | provider 级兼容开关；model 级可覆盖。 |
| `authHeader` | boolean | 否，默认 `false` | 把解析出的 API key 作为 `Authorization: Bearer` 再发一份。**Anthropic Messages 路径必须设 `true`。** |
| `models` | object[] | 否 | 该 provider 的模型列表；同 id 会替换内置/已有条目。 |
| `modelOverrides` | object | 否 | 只改已有 model 的元数据，不替换 provider 的模型列表；未知 id 忽略。 |

依据：【文档】`docs/models-configuration.md:11-24`；【源码】
`core/model-config.ts:89-115`。

### 3.2 `models[]` 字段

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `id` | string | 必填 | **原样发给 API 的 model 字符串**；可含 `/`、`@`、`~`、`:`。 |
| `name` | string | — | 显示名，也参与 `--model` 模糊匹配。 |
| `api` | string | 继承 provider | 单模型覆盖协议。混合 chat / Anthropic 模型时用。 |
| `baseUrl` | string | 继承 provider | 单模型覆盖 baseUrl。Anthropic 模型用 origin。 |
| `reasoning` | boolean | `false` | 是否支持 thinking；决定 thinking 级别是否可选、是否解析 reasoning 内容。 |
| `thinkingLevelMap` | object | — | Pi level → provider 值；`null` 表示不支持。 |
| `input` | `("text"\|"image")[]` | `["text"]` | 视觉能力；不写则 Pi 不把图片当可用输入。 |
| `inputLimits` | object | — | 图片 resize / 请求大小限制；可选。 |
| `cost` | object | — | USD / 1M tokens；子字段 `input`/`output`/`cacheRead`/`cacheWrite`。只影响 Pi 的会话成本显示。 |
| `contextWindow` | number | `128000` | Pi 用它做上下文/压缩判断；**不填就是 128k，不管真实模型多大**。 |
| `maxTokens` | number | `16384` | 映射到 `max_tokens` 或 `max_completion_tokens`；**不填就是 16384**。 |
| `samplingParams` | object | — | 合并进每次请求 body；仅 OpenAI-compatible API。 |
| `samplingParamsByThinkingLevel` | object | — | 按 Pi thinking level 合并 sampling 参数。 |
| `headers` | object | — | 单模型请求头。 |
| `compat` | object | — | 单模型兼容开关。 |

依据：【文档】`docs/models-configuration.md:26-57`；【源码】
`core/model-config.ts:48-69`（默认 128000 / 16384）。

### 3.3 协议类型与 tokencamp 的匹配

| `api` | tokencamp 对应端点 | 对 Pi 的适用性 |
| --- | --- | --- |
| `openai-completions` | `POST /v1/chat/completions` | **v1 默认。** Bearer、streaming、tools、images 都走 OpenAI 形状；当前 catalog 的 deployment 全是 `openai_compatible`。 |
| `anthropic-messages` | `POST /v1/messages` | **per-model 可选。** 协议原生、tools/thinking 保留；但要求模型有 Anthropic deployment，且 baseUrl 是 origin、`authHeader: true`。 |
| `openai-responses` | `POST /v1/responses` | **不要用。** stateless 子集拒绝 `function_call_output`，tool loop 第二轮必失败。 |
| `google-generative-ai` | 无 | 不适用。 |

**provider 包/扩展解析：不需要。** Pi 的官方建议就是“当服务已经支持某个受支持的
API 时用 `models.json`，不要写 provider extension”；`models.json` 是内置能力，
不涉及 npm 包、`pi install` 或 `pi config`。只有需要动态 model discovery、自定义认证
或非标准 wire protocol 时才要 `pi.registerProvider()` 扩展；tokencamp 不属于这一类。
【文档】`docs/custom-provider.md:1-15`、`docs/models.md:45-47`。如果用户已装了一个同名
`tokencamp` provider extension，`models.json` 会 overlay 在 extension 之上；tcamp
应避免和它抢同一个 id，或在冲突时让用户选择。

## 4. 鉴权

### 4.1 tokencamp 只认 Bearer

tokencamp 的 `/v1` direct 路径只从 `Authorization` header 取 token，并且只接受
`Bearer ` 前缀；没有 `x-api-key` 分支。【源码】
`crates/gateway/src/routes/direct.rs:831-833`；【文档】
`docs-site/api-reference/authentication.mdx:1-12`。

### 4.2 `openai-completions`：Bearer 是自动的

Pi 用 OpenAI SDK 构造客户端：`new OpenAI({ apiKey, baseURL: model.baseUrl })`。
OpenAI SDK 自己会发 `Authorization: Bearer <apiKey>`，**不需要 `authHeader`**。
【源码】`packages/ai/src/api/openai-completions.ts:776-782`；【实测】见 §6 第 1 行。

### 4.3 `anthropic-messages`：必须加 `authHeader: true`

Pi 用 Anthropic SDK 构造客户端：`new Anthropic({ apiKey, baseURL: model.baseUrl })`。
Anthropic SDK 默认发 `x-api-key`，tokencamp 不认。`authHeader: true` 让 Pi 在
`auth.headers` 里再加 `Authorization: Bearer <resolved key>`；实测请求同时带
`x-api-key` 和 `Authorization`，tokencamp 只看后者。【源码】
`packages/ai/src/api/anthropic-messages.ts:1097-1104`、
`core/provider-composer.ts:380-390, 428-434`；【实测】§6。

`authHeader` 要求“已解析出 API key”；如果只写 `headers: {"Authorization": "Bearer ..."}`
而没有任何 `apiKey`，provider 不会被 Pi 认为已认证，`--list-models` 直接显示
“No models available”。【实测】；【源码】`core/model-registry.ts:88-115`。

### 4.4 key 存哪里

| 方式 | 写法 | 评价 |
| --- | --- | --- |
| 环境变量插值 | `"apiKey": "$TOKENCAMP_API_KEY"` | **推荐。** 不落盘 secret；`pi auth check` 能检测 env 是否存在。 |
| 命令输出 | `"apiKey": "!security find-generic-password -ws tokencamp"` | 可选；Pi 在请求时运行命令，不缓存；`auth check` 只检查“配置了命令”，不执行。 |
| 字面量 | `"apiKey": "sk-..."` | 不推荐；`models.json` 会含明文。 |
| `auth.json` + `/login` | 用户交互式登录 | 用户自己的选择；tcamp 不读不写。 |
| `--api-key` | `pi --api-key "$KEY" --model ...` | 运行时最高优先级；不会持久化；会出现在进程 argv 里，注意本机其他用户可见。 |

`models.json` 的 `$NAME` / `${NAME}`、`!command`、`$$` / `$!` 规则见
【文档】`docs/models.md:65`。

### 4.5 凭据优先级与冲突

Pi 的凭据优先级是 `--api-key` > `auth.json` > `models.json` 的 `apiKey` > provider
环境变量。【文档】`docs/models.md:23`。这意味着：**如果用户以前对同一个 provider id
跑过 `/login`，`auth.json` 里的旧 key 会压过 tcamp 写的 `models.json` `apiKey`。**
`pi auth check --json` 不带 `--credentials` 时只输出 `status` / `authType`，无法区分
来源；带 `--credentials` 会打印明文 key，tcamp 不应使用。§12 把这一点列为未决问题。

## 5. 最小可用配置

### 5.1 chat-only（当前 catalog 可直接用）

`~/.pi/agent/models.json`：

```json
{
  "$schema": "https://pi.dev/schemas/models.schema.json",
  "providers": {
    "tokencamp": {
      "name": "tokencamp",
      "baseUrl": "https://api.tokencamp.com/v1",
      "api": "openai-completions",
      "apiKey": "$TOKENCAMP_API_KEY",
      "models": [
        {
          "id": "tokencamp/auto",
          "name": "tokencamp auto",
          "input": ["text", "image"],
          "contextWindow": 128000,
          "maxTokens": 16384,
          "compat": {
            "maxTokensField": "max_tokens",
            "supportsDeveloperRole": false,
            "supportsStore": false,
            "supportsReasoningEffort": false
          }
        }
      ]
    }
  }
}
```

`~/.pi/agent/settings.json`（只在缺失时补，见 §8）：

```json
{
  "defaultProvider": "tokencamp",
  "defaultModel": "tokencamp/auto"
}
```

启动：

```bash
export TOKENCAMP_API_KEY=sk-...
pi --provider tokencamp --model tokencamp/auto

# 一次性（tcamp 验证/CI 用）
pi -p --no-session --provider tokencamp --model tokencamp/auto "hi"
```

为什么这些 `compat`：

| 开关 | 值 | 原因 |
| --- | --- | --- |
| `maxTokensField` | `"max_tokens"` | Pi 对未知 URL 默认发 `max_completion_tokens`；tokencamp 的 ChatRequest 两种都收，但会原样转给上游，当前 catalog 的 OpenAI-compatible 上游更保守地支持 `max_tokens`。 |
| `supportsDeveloperRole` | `false` | 默认会把 system prompt 发成 `developer` role；tokencamp 的 chat→Anthropic adapter 不认 `developer`，会被丢掉。设 `false` 后 Pi 发 `system`。 |
| `supportsStore` | `false` | 默认会发 `store: false`；tokencamp 会把它放进 `extra` 透传，部分上游不认。关掉可减少无关字段。 |
| `supportsReasoningEffort` | `false` | auto 可能选到不支持 `reasoning_effort` 的模型；关掉后 Pi 不发送该字段，避免上游 400。需要 thinking 的直连模型再单独打开。 |

依据：【源码】`packages/ai/src/api/openai-completions.ts:823-834, 1638-1646, 1220`；
【实测】§6。这里的 `128000` / `16384` 是 Pi 的默认值，也是对 `tokencamp/auto`
的保守假设；auto 的真实窗口见 §12.2，写之前最好用实际请求校验。

### 5.2 可选：给有 Anthropic deployment 的模型加 per-model 覆盖

只有在确认该模型有 `provider_type: anthropic` deployment 时加；当前默认 catalog
没有任何这样的模型。

```json
{
  "id": "anthropic/claude-sonnet-4-5",
  "name": "Claude Sonnet 4.5 (tokencamp)",
  "api": "anthropic-messages",
  "baseUrl": "https://api.tokencamp.com",
  "reasoning": true,
  "input": ["text", "image"],
  "contextWindow": 200000,
  "maxTokens": 8192
}
```

同时 provider 级加 `"authHeader": true`。**不要给 `tokencamp/auto` 或
`@preset/...` 配 `anthropic-messages`**；它们没有 Anthropic deployment，messages
路径会 404。也不要给同一个模型同时写 chat 和 messages 两个条目；Pi 的同 id 条目
会互相替换。

## 6. 协议行为（本地 mock 实测）

实验方式：`PI_CODING_AGENT_DIR=/tmp/tcamp-pi-agent*`，mock HTTP server 只绑
`127.0.0.1`，记录 request line、关键 header 和 body；Pi 用 `-p --no-session
--no-extensions`，避免加载用户扩展。

| 配置 | 实际请求 | 关键 header / body | 结果 |
| --- | --- | --- | --- |
| `openai-completions`，baseUrl `http://127.0.0.1:18903/v1` | `POST /v1/chat/completions` | `Authorization: Bearer tcamp-test-key`；`model` 原样；`stream: true`；`tools` 4 个；`max_tokens`（设了 `maxTokensField`） | 成功；模型 id 含 `/`、`@`、`~` 都原样发出 |
| 同上但 baseUrl 只有 host（无 `/v1`） | `POST /chat/completions` | Bearer | 路径错误；OpenAI SDK 不补 `/v1` |
| `openai-completions` + tool mock | 第 1 轮 `POST /v1/chat/completions`；第 2 轮同路径 | 第 2 轮 body 含 `assistant.tool_calls` 和 `role: "tool"` + `tool_call_id` | 成功；Pi 执行了 `read` 工具并回传结果 |
| `anthropic-messages`，baseUrl `http://127.0.0.1:18902`（origin），`authHeader: true` | `POST /v1/messages?beta=true` | `x-api-key` + `Authorization: Bearer`；`thinking: {type: enabled, budget_tokens: 7168, display: summarized}`；`tools` 4 个；`max_tokens: 8192` | 成功（客户端侧） |
| 同上但 `authHeader` 缺省/`false` | `POST /v1/messages?beta=true` | 只有 `x-api-key` | tokencamp 会 401；Bearer-only |
| `anthropic-messages`，baseUrl `http://127.0.0.1:18902/v1` | `POST /v1/v1/messages?beta=true` | x-api-key + Bearer | 路径错误；会 404 |
| `headers` 里放 `Authorization: Bearer ...`，没有 `apiKey` | 无请求 | — | `--list-models` 显示 No models available；provider 不被认为已认证 |
| `openai-responses` | 未在本地跑通 tool loop | — | tokencamp 源码拒绝 `function_call_output`，不能用于 Pi 的 tool loop |
| `openai-completions` + Anthropic deployment 的模型 | chat→Anthropic | tokencamp adapter 丢 tools/tool_calls/thinking | 不适用；必须改用 `anthropic-messages` |

补充观察：

- Pi 的 OpenAI-compatible 请求默认包含 `store: false`（除非 `supportsStore: false`）
  和 `reasoning_effort`（除非 `supportsReasoningEffort: false`）；`max_tokens` vs
  `max_completion_tokens` 由 `compat.maxTokensField` 决定。默认对未知 URL 选
  `max_completion_tokens`、`developer` role、`store: false`、`supportsReasoningEffort:
  true`。【源码】`openai-completions.ts:1638-1646`；【实测】。
- Anthropic 路径的 thinking 走 body 的 `thinking` 字段；tokencamp 的
  `MessagesRequest` 把它作为 `serde_json::Value` 原样转发，不解释。
  【源码】`crates/core/src/anthropic.rs:41-43`。
- tokencamp 的 `/v1/messages` 是 near-passthrough，请求里的 `tools` / `tool_choice`
  原样转发；流式响应原样 relay（`event:` + `data:`）。【源码】
  `crates/core/src/anthropic.rs:36-43`、`crates/gateway/src/routes/messages.rs:1-25`。
- 但 tokencamp 的 `ContentBlock` 只有 `text` / `image` / `tool_use` / `tool_result`，
  没有 `thinking` / `redacted_thinking`。**非流式** `/v1/messages` 响应如果带 thinking
  block，反序列化会失败；Pi 始终用流式，所以当前不受影响，但这是平台侧的类型缺口。
  【源码】`crates/core/src/anthropic.rs:95-118`；【推断】。

## 7. 模型命名

### 7.1 tokencamp 的 name 原样进 Pi 的 `id`

Pi 的 `models[].id` 没有字符限制（只要求非空），会原样放进请求的 `model` 字段。
【源码】`core/model-config.ts:48-49`。实测以下 id 都原样发出：

| tokencamp name | Pi `models[].id` | Pi CLI 选择 | 适用协议 |
| --- | --- | --- | --- |
| `tokencamp/auto` | `tokencamp/auto` | `--provider tokencamp --model tokencamp/auto` | chat only |
| `deepseek/deepseek-v4-pro` | 同左 | `--provider tokencamp --model deepseek/deepseek-v4-pro` | chat；若该模型有 Anthropic deployment 则 messages |
| `@preset/cheap-chat` | 同左 | `--provider tokencamp --model @preset/cheap-chat` | chat only；workspace preset |
| `~deepseek/deepseek-v4-latest` | 同左 | `--provider tokencamp --model '~deepseek/deepseek-v4-latest'` | chat；messages 仅在 target 有 Anthropic deployment 时。family 是否存在以 catalog 的 `family`/`released_on` 为准 |

**不要写裸 `auto`。** tokencamp 的 catalog 用 exact lookup；`config/default.yaml`
里的虚拟模型名是 `tokencamp/auto`，没有 `auto` alias。客户文档 quickstart 里的
`"model": "auto"` 与 auto-router 文档的 `tokencamp/auto` 不一致；源码侧以
`tokencamp/auto` 为准。【源码】`crates/gateway/src/catalog.rs:65-76`、
`config/default.yaml:797-801`；【文档】`docs-site/quickstart.mdx:30-38` 与
`docs-site/guides/auto-router.mdx:6-11` 的矛盾。

### 7.2 Pi 的 `provider/id` 解析

Pi 先做“完整文本 = 某个 model id”的精确匹配，再按已知 provider id（最长前缀）拆
`provider/rest`。所以：

- 当 `models[].id` 就是 `tokencamp/auto` 时，`--model tokencamp/auto` 会命中
  **完整 id**，不需要写 `tokencamp/tokencamp/auto`；实测两者都发
  `tokencamp/auto`。
- 如果同一个 id 也在别的 provider 上存在，或者有歧义，用 `--provider tokencamp`
  限定。tcamp 启动命令建议始终带 `--provider`。
- `--model` 的 `:high` 只在“完整文本没有匹配”时才当 thinking 后缀；id 本身含冒号
  （如 OpenRouter 的 `:exacto`）不会被误拆。tokencamp 的 id 目前不含冒号。
  【源码】`core/model-resolver.ts:128-142, 235-262, 508-533`。

### 7.3 默认槽位

- `defaultProvider` + `defaultModel` 是**精确**查找；`defaultModel` 必须等于某个
  `models[].id`，不是 `provider/id` 语法（除非该 id 本身含 `/`）。
  `defaultProvider: "tokencamp"` + `defaultModel: "tokencamp/auto"` 实测有效。
- 如果写 `defaultModel: "tokencamp/other"` 但 id 是 `other`，Pi 找不到，回退到
  “第一个可用模型”（实测发的是 `auto`）。【源码】
  `core/model-resolver.ts:545-600`。
- `modelThinkingLevels` 的 key 是 `${provider}/${modelId}`；对 `tokencamp/auto`
  就是 `tokencamp/tokencamp/auto`（看起来怪，但这是规则）。一般不需要写。
  【文档】`docs/settings.md:16`。
- **没有独立的“小模型/快模型”槽位。** compaction / branch summary 默认用当前模型；
  `compaction.modelOverrides` 只调 token 预算，不换模型。classifier / image 模型是
  另一类 operation，不参与 chat。若 v1 想要“小模型”，只能通过 `--models` 或
  `enabledModels` 暴露另一个模型，或写 Pi 扩展/虚拟模型。【文档】
  `docs/settings.md:11-18`、`docs/compaction.md:35`；【源码】
  `core/settings-schema.ts:12-25`。

### 7.4 `--list-models` 如何发现模型

`pi --list-models` 调 `modelRuntime.getAvailable()`：**只列出 provider 凭据已解析的
模型**，不是 models.json 里的全部条目。凭据缺失时输出
“No models available.” 并 exit 0（不是错误）。【源码】
`core/list-models.ts:32-45`、`packages/ai/src/models.ts:701-721`；【实测】。

因此 tcamp 的检测不能把“`--list-models` 里有 tokencamp”当成配置成功；要用
`pi auth check --provider tokencamp --json` + 实际请求。

### 7.5 `/v1/models` 不能直接生成 Pi 模型列表

`GET /v1/models` 是公开只读，返回 `id` / `owned_by` / `kind` / `expiration_date`，
**没有 context_window、max output、thinking、vision，也没有 deployment 数量或
provider_type**。而且它列出所有未过期条目，包括 `display_only`（零 deployment）的
模型；这些模型在 chat 请求时会 `no_available_provider` / `model_not_found`。
【源码】`crates/gateway/src/routes/mod.rs:466-527`、`crates/gateway/src/catalog.rs:92-100`；
【文档】`docs-site/api-reference/authentication.mdx:1-12`。

结论：v1 用**精选静态列表**（至少 `tokencamp/auto` + 少量确认可路由的直连模型），
不要盲目把 `/v1/models` 全量写成 Pi models。`/feed/models` 有 `context_window` /
`capabilities` / 首个 deployment 的 `provider`，但它是非 `/v1` 的公开营销 feed，
不是客户 API 契约；可作为可选 enrichment，不能作为稳定依赖。【源码】
`crates/gateway/src/routes/feed.rs:37-60, 179-180`。

## 8. 非破坏性写入契约

### 8.1 `models.json`

- Pi **只读**，从不写回；`JSON.parse(stripJsonComments(stripBom(content)))`，所以
  JSONC 注释、尾逗号、BOM 都能接受。【源码】`core/model-config.ts:168-185`；
  变更日志 `packages/coding-agent/CHANGELOG.md:2270,2280`。
- 未知 key 在 provider / model 层被接受（实测 `"x-tcamp-managed": true`、
  `"x-note": "hello"` 都能正常加载）。这意味着 tcamp 可以加自己的标记；但为了
  不依赖未文档化的 schema 行为，仍建议用 sidecar manifest 记录插入了哪些条目。
- 没有项目级 `models.json`；只写 `<agent-dir>/models.json`，并尊重
  `PI_CODING_AGENT_DIR`。

安全写法：

1. 解析 JSONC（保留注释/尾逗号/键顺序）；解析失败就中止并提示，绝不覆盖。
2. 确保 `providers.tokencamp` 存在；只增改该 provider 下的 tcamp 模型，不删除
   其他 provider / 其他模型 / `modelOverrides`。
3. 同名 model id 的冲突要显式处理：默认拒绝并让用户选择“覆盖/改名/跳过”，不要静默
   替换用户已有条目。
4. 原子写（temp + rename），写前备份，最终 mode `0600`（即使 key 用 env 引用，
   文件里也可能出现命令或敏感信息）。
5. sidecar manifest（例如 `~/.config/tcamp/pi-state.json` 或 agent dir 下的
   `.tcamp-state.json`）记录 tcamp 插入的 provider/model id、写入时间和 hash，用于
   幂等与撤销；不要依赖 `models.json` 里的注释做状态。

### 8.2 `settings.json`

- 严格 JSON；`JSON.parse` 不认注释。【源码】`core/settings-manager.ts:380`。
- Pi 自己的 writer 会读当前文件、`{ ...currentFileSettings }`、只改 modified fields，
  然后 `JSON.stringify(mergedSettings, null, 2)` 整文件重写。**未知顶层 key 会被保留，
  但格式/键顺序/注释会丢。**【源码】`core/settings-manager.ts:600-626`。
- 普通 `pi -p` 运行不会自动重写 settings（实测 mtime 不变）；但 `/settings`、
  模型选择 Ctrl+S、`pi install/remove/config`、changelog 版本更新都可能触发写。
- tcamp 只应在 `defaultProvider` / `defaultModel` **缺失或为空**时写入；不要覆盖用户
  已有默认（除非显式 `--set-default`）。不要加注释或 managed block。
- 如果 Pi 正在运行，写 settings 可能与 Pi 的 `proper-lockfile` 锁竞争；建议在 Pi
  未运行时写，或复用 Pi 的锁机制再写。

### 8.3 `models-store.json` / `auth.json`

- `models-store.json` 是 Pi 的 catalog 缓存，`pi update --models` 会重写；tcamp 不要
  碰。【源码】`core/model-runtime.ts:223-227`、`core/models-store.ts:47-56`。
- `auth.json` 是 Pi 的凭据库；tcamp 不读不写，避免复制/破坏 OAuth token。用户要持久化
  就走 `/login`；tcamp 用 env / `!command` / `--api-key`。

### 8.4 项目级配置

- `.pi/settings.json` 只有在项目被 trust 后才生效；`pi -p` 下也要 `--approve` 才会
  应用（实测：不 `--approve` 时项目 default 被忽略，`--approve` 后生效）。
  【文档】`docs/configuration.md:3`；【实测】。
- `.pi/models.json` 完全不被读取。所以 tcamp 不能把 provider 配置写到项目里；项目级
  只能放非敏感的默认模型选择，而且不能依赖它生效。

## 9. 启动与检测

### 9.1 二进制与版本

| 项 | 值 | 依据 |
| --- | --- | --- |
| PATH 名 | `pi` | 【实测】`which -a pi` |
| installer 路径 | `~/.local/bin/pi` → `~/.pi/agent/bin/pi`（本机） | 【实测】`readlink` |
| npm 路径 | `$(npm prefix -g)/bin/pi`，bin 指向 `dist/bundle/cli.js` | 【源码】`package.json:9-11` |
| 版本 | `pi --version` → `1.4.0` | 【实测】 |
| Node | 22.19+（npm 安装）；installer 自带 Node | 【文档】`docs/quickstart.md:19-23` |

`pi --version` 输出裸版本号；`--help` 会包含已加载扩展注册的 flag，所以 tcamp 不要
解析 `--help` 的固定 flag 集，只解析 `--version` 和需要的几个核心 flag。

### 9.2 检测顺序（建议）

```text
1. command -v pi            # 不存在 -> not installed
2. pi --version             # 解析 semver；< 1.4.0 警告（见 §11）
3. $PI_CODING_AGENT_DIR 或 ~/.pi/agent
4. models.json 是否存在、是否能解析、是否有 providers.tokencamp
5. pi auth check --provider tokencamp --json
   exit 0 ready / 1 not_ready / 2 invalid
6. 需要确认 key 真的有效时，发一次最小请求：
   pi -p --no-session --no-tools --provider tokencamp --model tokencamp/auto "ping"
```

注意：

- `auth check` 的 `ready` 只表示“解析到了凭据”；它不验证 key 是否被 tokencamp 接受，
  也不执行 `!command`。真正的验证要发请求。
- `--no-tools` 可以缩小最小请求的 body；但 tokencamp 的 chat 校验对 `tools: null`
  没问题。
- `--no-session` 避免在用户 session 目录留下探测记录。
- 如果用户已有 `auth.json` 凭据，`auth check` 仍会 ready；tcamp 无法区分来源（§4.5）。

### 9.3 非交互输出

| 模式 | 行为 | 用途 |
| --- | --- | --- |
| `-p` / `--print` | 跑完 prompt，把最终 assistant 文本写 stdout，退出 | tcamp 验证、脚本调用；stderr 是诊断 |
| `--mode json` | JSONL 事件流；第一条是 session header，随后 `agent_start` / `message_*` / `turn_*` / `agent_end` / `agent_settled`；stdout 只放协议 | 需要结构化结果时 |
| `--mode rpc` | 从 stdin 读 JSONL 命令，向 stdout 写响应/事件，长驻 | 不作为 tcamp v1 的默认启动方式 |
| `--mode text` | text 输出；在 TTY 下仍会开 TUI | 不适合自动化 |

依据：【文档】`docs/cli.md:30-51`、`docs/json.md:1-40`；【实测】`--mode json` 第一行
是 `{"type":"session",...}`。

## 10. 已知坑 / 平台面

1. **默认 catalog 没有 Anthropic deployment（v1 blocker for `/v1/messages`）。**
   `config/default.yaml` 的 7 个 deployment 全是 `openai_compatible`；`/v1/messages`
   对当前 seed 的所有模型 404。Claude Code 研究（ticket #2）也独立确认了这一点。
   Pi 的 chat 路径
   不受影响；但任何“Pi 走 Anthropic Messages 接 Claude”的方案今天不可用。
2. **chat skin 对 Anthropic deployment 丢 tools。** 见 §0 第 3 条；这是 tokencamp
   adapter 的明确 v1 范围，不是 Pi 的 bug。
3. **`/v1/messages` 不支持 `@preset` / `auto`。** direct endpoint 没有 preset 层；
   auto 是 virtual model，没有 deployments。`~family-latest` 在 messages 路径可以
   解析（`resolve_chain` 对 `~` 走 `resolve_family`），但目标仍必须有 Anthropic
   deployment。【源码】`direct.rs:700-708`、`messages.rs:199-215`。
4. **`/v1/responses` 不能跑 tool loop。** 见 §0 第 1 条。
5. **模型元数据缺口。** `/v1/models` 没有 context / max output / thinking / vision；
   Pi 的默认值是 128k / 16384。把 1M 模型写成 128k 会让 Pi 过早压缩；把 32k 模型写成
   128k 会被 tokencamp 的本地预检 400。需要 tcamp 维护静态表，或可选读 `/feed/models`
   （非 `/v1` 契约）。【源码】`core/model-config.ts:59-64`、
   `crates/gateway/src/routes/mod.rs:510-527`、`feed.rs:37-60`。
6. **`max_completion_tokens` / `developer` / `store` 默认值。** Pi 对未知 URL 默认发
   `max_completion_tokens`、`developer` role、`store: false`；tokencamp 会原样透传。
   对当前 OpenAI-compatible 上游，建议显式设 `maxTokensField: "max_tokens"`、
   `supportsDeveloperRole: false`、`supportsStore: false`。见 §5.1。
7. **thinking 不是统一能力。** chat skin 的 `reasoning_effort` 只是 `extra` 透传：
   OpenAI-compatible 上游可能认，Anthropic deployment 会丢；auto 还可能选到不支持的
   模型。原生 messages 的 `thinking` body 会被转发，但平台当前没有 Anthropic
   deployment。v1 对 `tokencamp/auto` 建议关闭 `supportsReasoningEffort`，不要承诺
   thinking 一定生效。
8. **`/v1/models` 会列出不可路由的 `display_only` 模型。** 不要全量生成 Pi models；
   否则 `--list-models` 里一堆模型一用就 503 `no_available_provider`（或过期模型
   404 `model_not_found`）。
9. **`context_length_exceeded` 能被 Pi 识别。** tokencamp 的 400 错误 code 是
   `context_length_exceeded`；Pi 的 overflow 正则含
   `/context[_ ]length[_ ]exceeded/i`，所以 Pi 的 compaction/overflow recovery 有机会
   生效。`max_tokens_exceeded` 不在 Pi 的 overflow 列表里，按普通错误处理。
   【源码】`packages/ai/src/utils/overflow.ts:63,144-156`；【源码】
   `tokencamp-pro/docs/error-codes.md:140`。
10. **auth.json 旧凭据压过 models.json。** 见 §4.5；可能导致“配置写对了但请求用了
    旧 key”。v1 要么用 `--api-key` 运行时覆盖，要么用独立 provider id，要么在文档里
    提示用户 `/logout`。
11. **项目 trust。** `.pi/settings.json` 不 `--approve` 不生效；`.pi/models.json`
    根本不存在。不要把 provider 配置放项目级。
12. **默认模型写错会静默回退。** `defaultModel` 不是精确 id 时，Pi 会回退到第一个
    可用模型，不报错；tcamp 写 defaults 后应回读校验。
13. **Pi 的 system prompt / tools 体积不小。** 普通请求也带 4 个 built-in tools
    和一大段 system prompt；对 tokencamp 的 token 计费/上下文预检有影响。不是 bug，
    但 v1 的“最小探测”可以用 `--no-tools` 降低开销。
14. **`pi --help` 会被扩展污染。** 用户安装的扩展会注册额外 flag；不要依赖 help 的
    固定输出做版本/能力检测。

## 11. 版本与日期 caveat

- 本报告实测 `pi` **1.4.0**（release 2026-10-21）。上游 `main` 在 2026-11-01 的
  commit `2c8d1feb` 仍标 1.4.0，但包含 unreleased changelog；本报告的行号引用来自
  该 commit。
- `models.json` 的 `authHeader`、`compat`、JSONC 支持在 1.4.0 已存在（实测加载
  成功）；更老的 Pi 版本未验证。建议 tcamp 把 `>=1.4.0` 作为已验证版本，低于时警告
  而不是硬拒。
- tokencamp 侧：`tokencamp-pro@25db44b`（2026-10-31）；`config/default.yaml` 是
  dev seed，生产 DB 的 catalog/deployment 可能不同。`/v1/messages` 的 Anthropic
  deployment 数量必须以实际部署为准，不能从 seed 推断。
- 客户文档的 base URL 仍是 placeholder：`docs-site/snippets/base-url.mdx:1-4` 明确写
  “provisional，launch 前确认”。`https://api.tokencamp.com` 是当前文档/OpenAPI 用的
  地址，但上线前要再确认。
- Pi 的 model catalog 会通过 `pi update --models` 刷新，`models-store.json` 会变；
  但自定义 `models.json` 不受影响。

## 12. 未决问题

1. **生产 catalog 是否有 Anthropic deployment？** 如果有，哪些模型？`/v1/models`
   不暴露 `provider_type`，tcamp 无法自动决定某个模型该用 chat 还是 messages。
   需要平台提供可查询的 deployment/provider 元数据，或 tcamp 维护静态白名单。
2. **`auto` 的真实 context_window 是多少？** `config/default.yaml` 没写，源码默认
   32768；portal 测试用 128000；`/v1/models` 不返回。Pi 的 `contextWindow` 该写
   32k、128k 还是别的？这直接影响 Pi 何时压缩。
3. **裸 `auto` 在生产是否可用？** 源码 exact lookup 只有 `tokencamp/auto`，但
   quickstart 写 `"model": "auto"`。如果生产 DB 有 alias，tcamp 可以少写一层前缀；
   当前按 canonical 处理。
4. **`/v1/models` 是否应该作为模型发现源？** 它含 display_only 的不可路由模型，且
   无元数据；v1 用静态列表还是“全量 + 运行时容错”，需要产品决策。
5. **provider id 用 `tokencamp` 还是独立 id？** `tokencamp` 直观，但会和用户已有的
   `auth.json` 凭据冲突；独立 id（如 `tokencamp-gateway`）能隔离，但模型 id/CLI 更啰嗦。
6. **默认模型策略。** 只在缺失时写 `defaultProvider`/`defaultModel`，还是提供
   `--set-default` 显式覆盖？与地图的“可安全撤销”目标如何配合？
7. **thinking 的 v1 承诺。** 是明确“chat skin 不保证 thinking”，还是只对确认支持
   `reasoning_effort` 的直连模型开放？auto 的 `reasoning` 元数据怎么设？
8. **`/v1/messages` 的 thinking 非流式缺口。** tokencamp 的 `ContentBlock` 没有
   `thinking`；如果未来 Pi 或别的客户端发非流式请求，会反序列化失败。平台是否补类型？
9. **`anthropic-beta` header。** Pi 发 `?beta=true` 和
   `anthropic-beta: interleaved-thinking-2025-05-14`；tokencamp 的 `/v1/messages`
   只转发 body，不转发 beta header。interleaved thinking / fine-grained tool streaming
   等 beta 能力是否受影响，需要平台确认。
10. **最低 Pi 版本。** `authHeader` / JSONC / `maxTokensField` 在哪个版本引入？1.4.0
    已验证；是否需要检测并拒绝更老版本，还是只警告。

## 13. 引用

> 路径基准：Pi 的路径相对于
> `https://github.com/earendil-works/pi/blob/2c8d1feb147f6b3d97e855da803bfe7b19f3c403/`；
> tokencamp-www 的路径相对于
> `https://github.com/0xnicholas/tokencamp-www/blob/640d204/`；tokencamp-pro 为本地
> 仓库相对路径（commit `25db44b`）。

### Pi 官方文档（`earendil-works/pi@2c8d1feb`）

- Configuration — `packages/coding-agent/docs/configuration.md`
  （agent dir、`PI_CODING_AGENT_DIR`、`models.json`、项目 `.pi`、trust）
- Models — `packages/coding-agent/docs/models.md`
  （compatible endpoint、`$NAME`/`!command`、凭据优先级、模型选择）
- Models Configuration — `packages/coding-agent/docs/models-configuration.md`
  （provider/model/modelOverrides/compat 全字段）
- Providers — `packages/coding-agent/docs/providers.md`
  （`/login`、`auth.json`、env key、`!command`）
- Settings — `packages/coding-agent/docs/settings.md`
  （`defaultProvider`/`defaultModel`/`modelThinkingLevels`/`enabledModels`）
- Environment Variables — `packages/coding-agent/docs/environment-variables.md`
  （`PI_CODING_AGENT_DIR`、`PI_OFFLINE`、`PI_TELEMETRY`）
- Command Line — `packages/coding-agent/docs/cli.md`
  （`-p`、`--mode json/rpc`、`--provider/--model/--api-key`、`pi auth`）
- JSON Event Stream — `packages/coding-agent/docs/json.md`
  （JSONL、session header、事件）
- Custom Providers — `packages/coding-agent/docs/custom-provider.md`
  （“兼容 API 用 models.json，不要写扩展”）
- Quickstart — `packages/coding-agent/docs/quickstart.md`
  （installer、npm、`pi --version`）
- Changelog — `packages/coding-agent/CHANGELOG.md`
  （1.4.0、JSONC、models.json 历史）

### Pi 源码（`earendil-works/pi@2c8d1feb`）

- `packages/coding-agent/src/config.ts:629-646` — `getAgentDir()` / `getModelsPath()`
- `packages/coding-agent/src/core/model-config.ts:48-69, 89-115, 168-185`
  — model/provider schema、默认 128k/16k、JSONC 加载
- `packages/coding-agent/src/core/model-runtime.ts:218-227`
  — `models.json` / `models-store.json` 路径
- `packages/coding-agent/src/core/models-store.ts:47-56`
  — `FileModelsStore`
- `packages/coding-agent/src/core/provider-composer.ts:380-390, 428-434`
  — `authHeader` → `Authorization: Bearer`
- `packages/ai/src/api/openai-completions.ts:776-782, 823-834, 1220, 1638-1646`
  — OpenAI client、`max_tokens` 字段、`developer` role、compat 默认
- `packages/ai/src/api/anthropic-messages.ts:1097-1104`
  — Anthropic client（`apiKey`、`baseURL`）
- `packages/coding-agent/src/core/model-resolver.ts:128-142, 235-262, 301-311, 508-533, 545-600`
  — `provider/id` 解析、完整 id 优先、fallback model、默认模型选择
- `packages/ai/src/models.ts:701-721` — `getAvailable()` 只返回已认证 provider
- `packages/coding-agent/src/cli/list-models.ts:32-45` — `--list-models`
- `packages/coding-agent/src/core/settings-manager.ts:380, 600-626`
  — settings 严格 JSON、整文件 `JSON.stringify` 写入
- `packages/coding-agent/src/core/settings-schema.ts:12-25`
  — compaction 只有 token overrides，没有独立模型
- `packages/coding-agent/src/main.ts:198-206` — `pi auth check` 退出码
- `packages/ai/src/utils/overflow.ts:63, 144-156`
  — `context_length_exceeded` 识别

### tokencamp 文档（`tokencamp-www@640d204`）

- Quickstart — `docs-site/quickstart.mdx:27-38`
  （`https://api.tokencamp.com/v1`、Bearer、`model: auto`）
- Authentication — `docs-site/api-reference/authentication.mdx:1-12`
  （Bearer-only）
- Auto-Router — `docs-site/guides/auto-router.mdx:6-11`
  （canonical `tokencamp/auto`）
- Presets & model aliases — `docs-site/cookbook/presets.mdx:10-49`
  （`@preset/slug`、`~author/family-latest`）
- Base URL note — `docs-site/snippets/base-url.mdx:1-4`
  （provisional）

### tokencamp-pro 源码（`25db44b`）

- `crates/gateway/src/routes/direct.rs:814-818, 831-833`
  — direct endpoint 无 preset 层；Bearer-only 取 token
- `crates/gateway/src/routes/messages.rs:199-215`
  — `/v1/messages` 要求 Anthropic deployment
- `crates/gateway/src/pipeline/validate.rs:18-45`
  — chat skin 的 `@preset` / `~` 解析
- `crates/gateway/src/routes/mod.rs:466-527`
  — `/v1/models` 公开列表与 `ModelObject` 字段
- `crates/gateway/src/catalog.rs:65-76`
  — exact name/alias lookup
- `crates/gateway/src/config.rs:680-682`
  — 默认 context_window 32768
- `config/default.yaml:797-801`
  — `tokencamp/auto` 条目（无 deployments）
- `crates/core/src/chat.rs:15-53`
  — `ChatRequest` 字段（`max_tokens`、`max_completion_tokens`、`tools`、`tool_calls`、`tool_call_id`、`extra`）
- `crates/core/src/anthropic.rs:17-55, 95-118`
  — `MessagesRequest`（tools/thinking 原样转发）、`ContentBlock`（无 thinking）
- `crates/gateway/src/upstream/openai.rs:154-168`
  — OpenAI-compatible 薄透传（只改 model、强制 stream usage）
- `crates/gateway/src/upstream/anthropic.rs:1-12, 238-250`
  — chat→Anthropic 丢 tools/thinking
- `crates/gateway/src/routes/responses.rs:185-230, 1728-1731`
  — `/v1/responses` stateless 拒绝面（含 `function_call_output`）
- `crates/core/src/responses.rs:96-99`
  — 只服务 message item
- `docs/error-codes.md:140`
  — `context_length_exceeded` 400

### 本报告实验

所有 Pi 实验都在临时 `PI_CODING_AGENT_DIR`（`/tmp/tcamp-pi-agent*`）下进行；mock
server 只绑 `127.0.0.1:18901-18904`；未读取/修改用户真实 `~/.pi/agent`，未读
`auth.json`。实验后已关闭 mock，临时目录保留用于复核。

关键命令：

```bash
pi --version
pi --list-models
pi auth check --provider tokencamp --json
pi -p --no-session --no-extensions --provider tokencamp --model tokencamp/auto "hi"
pi -p --no-session --no-extensions --provider tokencamp --model anthropic/claude-sonnet-4-5 --thinking high "hi"
```

观察到的事实：

- `openai-completions` + baseUrl `.../v1` → `POST /v1/chat/completions`，
  `Authorization: Bearer <key>`，`model` 原样，`stream: true`，4 个 tools。
- `openai-completions` + baseUrl 无 `/v1` → `POST /chat/completions`。
- tool mock：第 2 轮 body 含 `assistant.tool_calls` 和 `role: "tool"` +
  `tool_call_id`；Pi 成功执行 `read` 并回传文件内容。
- `anthropic-messages` + origin + `authHeader: true` → `POST /v1/messages?beta=true`，
  `x-api-key` + `Authorization: Bearer`，`thinking` budget 7168，4 个 tools。
- `anthropic-messages` + origin + `authHeader: false` → 只有 `x-api-key`。
- `anthropic-messages` + baseUrl `.../v1` → `POST /v1/v1/messages?beta=true`。
- `apiKey: "$TOKENCAMP_TEST_KEY"` 未设置时 `--list-models` 输出
  “No models available.”（exit 0）；`pi auth check --provider tokencamp --json`
  输出 `{"status":"not_ready","provider":"tokencamp","reason":"credentials_not_configured"}`，
  exit 1；设置后输出 `{"status":"ready","provider":"tokencamp","authType":"api_key"}`，
  exit 0。
- `models.json` 含 `//` 注释、未知 provider/model key 时仍能加载；`settings.json`
  不能被 Pi 的 writer 保留注释（严格 JSON）。
- 项目 `.pi/settings.json` 不 `--approve` 时被忽略；`--approve` 后生效。
  `.pi/models.json` 完全不被读取。
- `defaultModel: "tokencamp/other"`（id 为 `other`）不会匹配，回退到第一个可用模型。
