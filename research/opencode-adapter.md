# OpenCode 适配器契约（ticket #3）

- 状态：research 完成，2026-11-01
- 被测版本：`opencode` 1.19.5（npm `opencode-ai@1.19.5`，源码 tag `v1.19.5`，commit `810c0eb`；macOS 13.7.8 / x86_64）
- 范围：`tcamp` v1 的 OpenCode adapter；纯客户端，不改 tokencamp-pro
- 结论日期：2026-11-01（OpenCode 2 已进入 `next` 通道，见 §9）

## 0. 结论（Verdict）

**可配置，且只需一份 `provider.tokencamp` 配置，不需要插件或代理。** OpenCode 走
AI SDK 的 OpenAI-compatible 路径时，把 `baseURL` 指向 `https://api.tokencamp.com/v1`
就会得到：`POST /v1/chat/completions`、`Authorization: Bearer <key>`、
`stream: true`、`stream_options.include_usage: true`、完整 OpenAI function tools；
`model` 字段是纯 model id（例如 `auto`），不带 provider 前缀。这与 tokencamp 网关的
`/v1` 契约一致。

**必须用 `@ai-sdk/openai-compatible`，不能用 `@ai-sdk/openai`。** 后者走
`/v1/responses`，而 tokencamp 的 `/v1/responses` 是 stateless 子集，明确拒绝
`function_call_output`（多轮工具结果回放）[【文档】migration.mdx][【文档】providers]。
OpenCode 的 agent 循环每轮都会回放工具结果，所以 Responses 路径不可用。

对 `tcamp` 的实现要求（详见 §3、§5、§8）：

1. 写 `provider.tokencamp`：`npm = "@ai-sdk/openai-compatible"`，
   `options.baseURL = "https://api.tokencamp.com/v1"`（**必须带 `/v1`**）。
2. 每个模型**必须写 `limit.context` 和 `limit.output`**。不写 `limit.output` 时
   OpenCode 固定发 `max_tokens: 32000`，会撞上 tokencamp 的 context 预检并 400（§8.1）。
3. 模型清单只放 tool-capable 的模型；默认模型建议 `auto`（网关按请求能力过滤候选，
   不需要 tcamp 自己判断）(§8.2)。
4. key 走 `options.apiKey: "{env:TOKENCAMP_API_KEY}"` 或 `env: ["TOKENCAMP_API_KEY"]`。
   `opencode auth login --provider tokencamp` **不可用**（非 models.dev 的 provider id
   会被拒），只能交互式选 “Other”（§6）。
5. 只在 `model`/`small_model` 缺失时写入默认值，绝不覆盖用户已有选择。

## 1. Provider 形状与文件/键表

### 1.1 文件位置与优先级

OpenCode 的配置是**深合并**（deep merge），不是替换；同 key 后者覆盖前者，非冲突 key
全部保留；`instructions` 数组做并集，`plugin` 去重 [【源码】`config.ts:42-52`]。

| 顺序（低 → 高） | 路径 | 说明 | 依据 |
| --- | --- | --- | --- |
| 1 | `https://<provider>/.well-known/opencode`（远端） | 组织默认值，登录支持它的 provider 时拉取 | 【源码】`config.ts:370-410`；【文档】config |
| 2 | `~/.config/opencode/config.json` | 旧全局名（仍在读） | 【源码】`config.ts:272` |
| 3 | `~/.config/opencode/opencode.json` | 全局 | 【源码】`config.ts:273` |
| 4 | `~/.config/opencode/opencode.jsonc` | 全局；**OpenCode 自己首次运行创建的是这个文件** | 【源码】`config.ts:140-148, 274`；【实测】空 HOME 首次运行生成 `opencode.jsonc` |
| 5 | `$OPENCODE_CONFIG` | 自定义文件，夹在全局与项目之间 | 【源码】`config.ts:415-418` |
| 6 | 项目 `opencode.jsonc` → `opencode.json` | 从 cwd 逐级向上到 git worktree；越近越高，同目录 jsonc 高于 json | 【源码】`paths.ts:10-21`；【文档】config |
| 7 | `.opencode/opencode.json(c)` | 目录级配置 | 【源码】`config.ts:438-448` |
| 8 | `$OPENCODE_CONFIG_CONTENT` | 内联 JSON | 【源码】`config.ts:482-490` |
| 9 | 控制台账号配置 | 登录 OpenCode 账号时 | 【源码】`config.ts:509-519` |
| 10 | 系统托管目录 | macOS `/Library/Application Support/opencode/` | 【源码】`config.ts:530-536`；【文档】config |
| 11 | macOS 托管偏好 | `.mobileconfig`，域 `ai.opencode.managed`，最高 | 【文档】config |

XDG 基准：config `$XDG_CONFIG_HOME/opencode`（默认 `~/.config/opencode`）、data
`~/.local/share/opencode`、state `~/.local/state/opencode`、cache `~/.cache/opencode`
[【源码】`global.ts`]。

> **文档与源码不一致**：文档写全局配置是 `~/.config/opencode/opencode.json`；1.19.5
> 的 `globalConfigFile()` 候选顺序是 `opencode.jsonc` → `opencode.json` → `config.json`，
> 且首次运行创建 `opencode.jsonc` [【源码】`config.ts:140-148, 264-271`]。
> tcamp 应沿用 OpenCode 自己的顺序：优先编辑已存在的文件，都不存在时创建
> `opencode.json`（文档标准、JSON 更好写）。

### 1.2 `provider.<id>` 字段（v1 形状）

OpenCode 1.x 只认 `provider`（单数）；写成 v2 的 `providers` 会被判为 unsupported
（只打 compatibility warning，不报错），等于**静默不生效**
[【源码】`v2-compat.ts:117-118`]。
所有字段都可选；`baseURL` 实际必填，`npm` 建议显式写（省略时也能兜底，但别依赖）。

| JSON key | 类型 | 必填 | 说明 | 依据 |
| --- | --- | --- | --- | --- |
| `provider.tokencamp.npm` | string | 建议显式 | AI SDK 包名。省略时按 `model.provider.npm → provider.npm → 目录值 → "@ai-sdk/openai-compatible"` 兜底 | 【源码】`provider.ts:1547-1555` |
| `.name` | string | 否 | UI 显示名；缺省用 provider id | 【源码】`provider.ts:1535-1540` |
| `.options.baseURL` | string | **是** | 必须以 `/v1` 结尾；支持 `${ENV}` 插值 | 【源码】`provider.ts:1813-1830` |
| `.options.apiKey` | string | 三选一 | 支持 `{env:VAR}` / `{file:path}` 变量替换 | 【源码】`variable.ts:34-38` |
| `.options.headers` | object | 否 | 追加 header；`apiKey` 会先生成 `Authorization: Bearer` | 【文档】providers |
| `.options.timeout` / `headerTimeout` / `chunkTimeout` | int \| false | 否 | 毫秒；`chunkTimeout` 默认 300000 | 【文档】config |
| `.env` | string[] | 二选一 | 环境变量名列表，启动时取第一个非空值作为 provider key | 【源码】`provider.ts:1633-1641` |
| `.api` | string | 否 | baseURL 的另一种写法（模型级 `provider.api` 会覆盖它） | 【源码】`core/.../provider.ts:99`；`provider.ts:1566` |
| `.models` | object | **是** | 选择 id → 模型定义，见 1.3 | 【源码】`core/.../provider.ts:146` |
| `.whitelist` / `.blacklist` | string[] | 否 | 只影响 `/models` 选择器可见性 | 【文档】providers |

key 优先级（高 → 低）：`options.apiKey` > `auth.json` 的 `type:"api"` > `env` 列表命中的
环境变量；三者都没有则请求不带 Authorization
[【源码】`provider.ts:1633-1641, 1835`（auth 在 env 之后合并，故 auth 覆盖 env）]。

### 1.3 `models.<selection-id>` 字段

**表的 key 是选择 id，`id` 是真正发给 API 的 model 字符串**；
`apiID = model.id ?? existingModel?.api.id ?? modelID` [【源码】`provider.ts:1546`]。
所以 key 可以随便取，含 `/`、`@`、`~` 都可以。

| JSON key | 类型 | 必填 | 缺省行为 | 依据 |
| --- | --- | --- | --- | --- |
| `id` | string | 否 | 等于 key；用于 key 与 wire id 不同时重映射 | 【源码】`provider.ts:1546` |
| `name` | string | 否 | 依次回落到 existing → key | 【源码】`provider.ts:1556-1560` |
| `limit.context` | number | **强烈建议** | 0；为 0 时上下文统计/自动压缩完全关闭 | 【源码】`provider.ts:1610`；`overflow.ts:10, 27` |
| `limit.output` | number | **强烈建议** | 0 → 实际按 `OUTPUT_TOKEN_MAX = 32000` 发 `max_tokens` | 【源码】`transform.ts:18, 1481-1483` |
| `tool_call` | bool | 否 | `true`（新模型） | 【源码】`provider.ts:1575` |
| `reasoning` | bool | 否 | `false` | 【源码】`provider.ts:1573` |
| `interleaved` | bool \| string \| `{field}` | 否 | `false`；**deepseek 且 openai-compatible 时自动设 `reasoning_content`** | 【源码】`provider.ts:1593-1598` |
| `attachment` | bool | 否 | `false`（不发送图片） | 【源码】`provider.ts:1574` |
| `temperature` | bool | 否 | `false` → 不发送 `temperature` | 【源码】`provider.ts:1572`；`session/llm/request.ts:123-125` |
| `modalities` | `{input,output}` | 否 | 文本进/文本出 | 【源码】`provider.ts:1576-1590` |
| `cost` | object | 否 | 0；只影响 UI 成本显示 | 【源码】`provider.ts:1600-1608` |
| `options` / `headers` | object | 否 | 透传给 AI SDK | 【源码】`provider.ts:1609, 1614` |
| `status` / `family` / `release_date` / `variants` | — | 否 | UI 与 variant 用 | `core/.../provider.ts:24-96` |

## 2. 协议行为（本地 mock 实测）

用临时 HOME + 本地 mock `/v1`（记录 request line/header/body）跑
`opencode run --model tokencamp/auto`，OpenCode 1.19.5 实际发出：

```
POST /v1/chat/completions
authorization: Bearer sk-test-123
user-agent: opencode/1.19.5 ai-sdk/provider-utils/4.0.23 runtime/bun/1.4.4
content-type: application/json
```

body 关键字段：

| 字段 | 值 | 说明 |
| --- | --- | --- |
| `model` | `"auto"` | 纯 model id，不带 `tokencamp/` 前缀 |
| `stream` | `true` | OpenCode 始终走 `streamText`，没有非流式分支 |
| `stream_options.include_usage` | `true` | OpenCode 对 openai-compatible 强制打开 [【源码】`provider.ts:1809-1810`]；网关已支持并转发 usage [【源码】`tests/chat.rs:199-202`] |
| `max_tokens` | `8192` | 来自 `limit.output`（本次配置值） |
| `temperature` | 不发送 | 因为 `capabilities.temperature` 默认 false |
| `top_p` | 不发送 | 该 model id 未命中内置规则 |
| `tools` | 10 个 function | `bash/edit/glob/grep/read/skill/task/todowrite/webfetch/write` |
| `tool_choice` | 不发送 | 由 agent/权限决定 |

另外两条实测事实：

- **标题生成请求**（`You are a title generator...`）单独发一次，`tools: []`，model 走
  `small_model`；未设 `small_model` 时会回落到主模型（§4.2）。
- **OpenCode 不调用 `/v1/models`**。模型清单完全来自配置，网关的模型目录不会被发现。

## 3. 最小可用配置（已在 1.19.5 上验证）

写入 `~/.config/opencode/opencode.json`（或项目 `opencode.json`）：

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "model": "tokencamp/auto",
  "small_model": "tokencamp/auto",
  "provider": {
    "tokencamp": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "tokencamp",
      "options": {
        "baseURL": "https://api.tokencamp.com/v1",
        "apiKey": "{env:TOKENCAMP_API_KEY}"
      },
      "models": {
        "auto": {
          "name": "tokencamp auto",
          // 占位：应取 auto 池里的最小 context（未决，见 §8.6 / §10）
          "limit": { "context": 32000, "output": 8192 }
        }
      }
    }
  }
}
```

验证方式（tcamp 可以直接复用）：

```bash
opencode debug config      # 打印解析后的配置；apiKey 显示为 "***"
opencode models            # 输出 tokencamp/auto
opencode run -m tokencamp/auto "hi"
```

`opencode debug config` 实测会把 `{env:...}` 解析掉并把 key 打码；`opencode models`
实测输出 `tokencamp/auto`、`tokencamp/@preset/fast`、`tokencamp/~deepseek/...`、
`tokencamp/deepseek/deepseek-v4-pro` —— 四种形态都原样保留。

### 3.1 第二轮实测（默认值、工具往返、reasoning）

用同一套 mock 再验证三件事（日志见 §12）：

- **不写 `limit` 时**，主请求与标题请求都发 `max_tokens: 32000`（`OUTPUT_TOKEN_MAX`），
  证实 §8.1 的风险不是纸面推演。
- **工具往返可用**：mock 返回 `tool_calls` 后，OpenCode 发出的第二轮请求
  `messages` 形如 `[system, user, assistant(tool_calls), tool(tool_call_id: "call_1")]`，
  正是 tokencamp `Message { tool_calls, tool_call_id, content }` 能接受的形状。
- **reasoning 能显示**：mock 在 delta 里放 `reasoning_content` 时，`opencode run --thinking`
  打印 `Thinking: I am thinking about the answer`；但把 `reasoning: true` 写进模型配置后，
  请求体多出 `reasoning_effort: "low"`（网关会原样透传给上游）。

## 4. 模型命名

### 4.1 `auto` / `@preset/...` / `~family-latest` 都是普通 id

OpenCode 的 `parseModel` 只按**第一个 `/`** 切分：前半是 provider id，其余全部是
model id [【源码】`provider.ts:2083-2089`]。因此：

| tokencamp 写法 | OpenCode 配置 key | 实际发出的 `model` | 结论 |
| --- | --- | --- | --- |
| `auto` | `auto` | `auto` | 可用（网关 Auto-Router） |
| `@preset/fast` | `@preset/fast` | `@preset/fast` | 可用（网关 validate 阶段解析） |
| `~deepseek/deepseek-v4-latest` | `~deepseek/deepseek-v4-latest` | 同左 | 可用（网关 `resolve_family`） |
| `deepseek/deepseek-v4-pro` | `deepseek/deepseek-v4-pro` | 同左 | 可用 |

注意 `#` 是 OpenCode 的 variant 分隔符（`provider/model#variant`），model id 里不能带
`#`；tokencamp 的 id 都不带，没问题。

### 4.2 小模型 / 标题生成

- 顶层 `small_model: "provider/model"` 显式指定；标题生成优先用它
  [【源码】`prompt.ts:216-221`]。
- 未设置时 `getSmallModel()` 只在 provider 的模型里找 family 为
  `gemini-flash` / `gpt-nano` / `claude-haiku` 的模型；自定义 provider 的
  `family` 为空，找不到就**回落到主模型**（同一请求路径再发一次）
  [【源码】`provider.ts:1964-2020`；`prompt.ts:220-221`]。
- 对本项目的影响：标题请求无论如何都会发一次；不写 `small_model` 时它走主模型
  （按主模型计费），写了则走便宜模型。tcamp 建议显式写
  `small_model: "tokencamp/auto"`（或更便宜的模型）。

### 4.3 默认模型选择顺序

`--model` 参数 → 配置 `model` → 上次使用的模型（`~/.local/state/opencode/model.json`）
→ provider 内部优先级 [【文档】models；【源码】`provider.ts:2035-2057`]。
tcamp 只在 `model` 为空时写入，避免覆盖用户选择。

## 5. 非破坏性写入契约

1. **深合并**：OpenCode 把全局、`OPENCODE_CONFIG`、项目、`.opencode` 各层深合并；
   tcamp 只需保证写入的局部对象正确，不必复制用户的其他 key。
2. **未知 key**：解码用 `onExcessProperty: "ignore"` [【源码】`parse.ts:35-45`]，
   不认识的字段不会报错；但 tcamp 自己写文件时仍应保留用户原有的未知 key 与注释。
3. **JSONC**：OpenCode 支持注释与尾逗号 [【源码】`parse.ts:8-12`]；写 `.jsonc` 时
   必须保留注释与格式（Rust 侧用 format-preserving 编辑器，或只做最小 patch）。
4. **`$schema` 自动注入**：加载时若文件没有 `$schema`，OpenCode 会把
   `"$schema": "https://opencode.ai/config.json"` **写回文件**
   [【源码】`config.ts:245-249`]。tcamp 自己写入该字段可避免 OpenCode 改文件。
5. **旧 TOML 迁移**：若存在 `~/.config/opencode/config`（TOML），OpenCode 会把它转成
   `config.json` 并删除原文件 [【源码】`config.ts:276-290`]。tcamp 写入前应先让
   OpenCode 完成迁移，或直接读迁移后的文件，避免两边同时写。
6. **写入顺序**：优先编辑已存在的文件（`opencode.jsonc` → `opencode.json` →
   `config.json`）；都不存在时创建 `opencode.json`。原子写、保留文件 mode。
7. **配置目录副作用**：OpenCode 会在 `~/.config/opencode/` 写 `.gitignore`
   （`node_modules` 等）并在缺失时创建 `opencode.jsonc`
   [【源码】`config.ts:264-271, 309-326`；【实测】]。

## 6. 鉴权

| 方式 | 写法 | 评价 |
| --- | --- | --- |
| 配置内引用环境变量 | `options.apiKey: "{env:TOKENCAMP_API_KEY}"` | **推荐**；key 不落盘；未设置时替换为空串（不报错，会 401）[【源码】`variable.ts:36-38`] |
| provider env 列表 | `"env": ["TOKENCAMP_API_KEY"]` | 可用；启动时读进程环境，等价于 auth.json 的 `api` 类型 [【源码】`provider.ts:1633-1641`] |
| 凭据文件 | `~/.local/share/opencode/auth.json`：`{"tokencamp":{"type":"api","key":"sk-…"}}` | `opencode auth login` 交互式 “Other” 写入，mode 0600 [【源码】`auth/index.ts:10, 27-31, 79`] |
| 文件引用 | `options.apiKey: "{file:~/.secrets/tokencamp-key}"` | 可用；文件缺失会直接报配置错误 [【源码】`variable.ts:61-85`] |
| 明文写配置 | `options.apiKey: "sk-…"` | 能用但不建议 |

**`opencode auth login --provider tokencamp` 会失败**：`--provider` 只接受 models.dev
目录里的 id 或插件注册的 id，否则 `Unknown provider "tokencamp"`
[【源码】`cli/cmd/providers.ts:405-419`]。自定义 provider 只能交互式选 “Other”，
再输入 id（正则 `^[0-9a-z-]+$`，`tokencamp` 合法）[【源码】`cli/cmd/providers.ts:434-445`]。
tcamp 若想免交互，应直接写 `options.apiKey` 或 auth.json，不要调用 `auth login`。

OAuth 与本案无关：tokencamp 的 OAuth token 只做身份，不被 `/v1` 接受；OpenCode 这边
也不会对未知 provider 发起 OAuth。

## 7. 启动与检测

| 事项 | 事实 | 依据 |
| --- | --- | --- |
| 可执行文件 | `opencode`（npm 包 `opencode-ai`，平台包 `opencode-<os>-<arch>`） | 【实测】`opencode --version` → `1.19.5` |
| 版本 | `opencode --version` / `-v` | 【文档】cli |
| 非交互 | `opencode run "…"`，`--model/-m`、`--format json`、`--auto`、`--print-logs`；成功退出码 0 | 【文档】cli；【实测】 |
| 解析后配置 | `opencode debug config`（apiKey 打码为 `***`） | 【文档】cli；【实测】 |
| 模型列表 | `opencode models [provider]`，输出 `provider/model` | 【文档】cli；【实测】 |
| 凭据列表 | `opencode auth list` | 【文档】cli |
| 无头服务 | `opencode serve`，`opencode run --attach http://127.0.0.1:4096` | 【文档】cli |
| 配置覆盖 | `OPENCODE_CONFIG=/path/to.json` 只影响该进程 | 【源码】`config.ts:415` |
| 平台构建 | Intel Mac 上 `opencode-darwin-x64` 的二进制在本机被 SIGKILL（未签名、ad-hoc 签名报 `main executable failed strict validation`），`opencode-darwin-x64-baseline` 可运行 | 【实测】本机 macOS 13.7.8 |
| OpenCode 2 | npm `opencode-ai@next` 安装的二进制名是 `opencode2` | 【实测】npm dist-tags / `bin` |

**检测建议**：`opencode --version` 解析语义版本；`opencode debug config` 的退出码可判断
配置是否被接受（配置非法时 1.19.5 直接抛错退出）；用 `opencode models` 确认
`tokencamp/...` 出现在列表里。

## 8. 已知坑 / 设计约束

### 8.1 `limit.output` 不写 → `max_tokens: 32000` → 可能 400（最重要）

OpenCode 每个请求都显式带 `max_tokens = limit.output`（封顶 32000）
[【源码】`transform.ts:1481-1483`]；**不写 limit 时实测发 `32000`**（§3.1）。
tokencamp 的 validate 阶段用
`估算 prompt + max_tokens` 与模型的 `context_window` 比较，超了就 400
`context_length_exceeded` [【源码】`validate.rs:104-120`；`pipeline/mod.rs:555-575`]。
所以一个 32k 上下文的模型，只要 prompt 超过 0 就可能被拒。**tcamp 必须给每个模型写
`limit.output`（例如 8192），并让 `limit.context` ≤ 网关的 `context_window`。**

### 8.2 tools 是按模型 gate 的

网关在模型 `capabilities.tools = false` 时，对带 `tools` 的请求直接 400
“model does not support tools” [【源码】`validate.rs:66-68`]。OpenCode 默认每个主请求
都带 10 个工具，所以给用户列一个不支持工具的模型等于列一个必坏项。
`auto` 会在候选里按 `tools` 能力过滤 [【源码】`auto.rs:17-22, 60-70`]，因此
**`auto` 是最安全的默认模型**。

### 8.3 `/v1/models` 没有 context / capability

`ModelObject` 只有 `id`、`object`、`created`、`owned_by`、`kind`、`expiration_date`
[【源码】`routes/mod.rs:537-552`]。OpenCode 也不查它。所以 tcamp 无法从 `/v1/models`
拿到 `limit.context`；可用的公开来源是 `GET /feed/models`（无鉴权，含
`context_window`、`capabilities`、`price`）[【源码】`routes/feed.rs:41-60`]。
但 `/feed/models` 是营销 feed，不是 `/v1` 契约的一部分，属于**弱依赖**（§10）。

### 8.4 reasoning 内容会被网关丢掉；打开 `reasoning` 还会多带字段

OpenCode 自身**支持并显示** reasoning：mock 返回 `delta.reasoning_content` 时，
`opencode run --thinking` 打印 `Thinking: ...`（§3.1 实测）。它依赖
`interleaved`（对 id 含 `deepseek` 的模型自动设为 `reasoning_content`）
[【源码】`provider.ts:1593-1598`；`transform.ts:322-345`]。

但 tokencamp 的 `ChatChunk`/`Delta` 只建模 `role` / `content` / `tool_calls`，
`Message` 也没有 `reasoning_content` [【源码】`crates/core/src/chat.rs`]；
流式与非流式都在反序列化时丢弃该字段。结果：**OpenCode 拿不到 thinking**，
工具调用与正文不受影响。

另一个坑：给模型写 `"reasoning": true` 会让 OpenCode 在请求体里加
`reasoning_effort: "low"`（实测），网关通过 `ChatRequest.extra` 原样透传给上游；
上游是否接受因模型而异。**建议 tcamp 不写 `reasoning`（保持默认 false），
既避免多带字段，也避免用户以为能看到 thinking。**

### 8.5 只能走 chat completions

见 §0。`@ai-sdk/openai`（Responses）在 OpenCode 里会发 `/v1/responses`，
而该端点是 stateless 子集、拒绝 `function_call_output`，agent 多轮必然失败。
`npm` 必须写死 `@ai-sdk/openai-compatible`，不要依赖“省略时兜底”。

### 8.6 `auto` 的 context 是池里最小值

`auto` 的 context 在网关侧是虚拟模型（`context_window = 0`，跳过预检），真正执行时
换成具体模型 [【源码】`validate.rs:104-110`；`auto.rs`]。OpenCode 侧只有一个
`limit.context` 用于压缩阈值。若写得比实际路由到的模型大，压缩会太晚、请求先被网关
400；写得小则压缩偏早。**建议取 auto 池里的最小窗口**——具体数值需产品侧确认（§10）。

### 8.7 其他

- OpenCode 会带 `x-session-affinity`、`X-Session-Id`、`User-Agent`
  [【源码】`session/llm/request.ts:196-202`]；网关只从 `Authorization` 取 token，
  未知 header 不影响。
- 响应里的 `usage.cost` 是 tokencamp 扩展，OpenCode 不读；不影响功能。
- `@ai-sdk/openai-compatible` 在 OpenCode 内是**内置** provider 包
  （`BUNDLED_PROVIDERS`）[【源码】`provider.ts:157-158`]，不需要首次运行联网安装。
- `@preset/<slug>` 是 workspace 作用域的；全局配置里列 preset，换 workspace 会 404。
  建议只把 preset 当作可选条目，不要设成默认模型。
- OpenCode 自己会改配置文件：缺 `$schema` 时写回（§5.4）；`updateGlobal()` 也会重写
  全局文件（TUI/桌面端改设置时）[【源码】`config.ts:656-680`]。tcamp 的 manifest/
  撤销逻辑要能容忍这种外部修改。

## 9. 版本与日期 caveat

- 本报告基于 **1.19.5**（2026-11-01 npm `latest`）。OpenCode 迭代很快，配置键可能增删；
  实现前用 `opencode debug config` 做一次 schema 冒烟。
- **OpenCode 2 已存在**：npm `next` 通道（`0.0.0-next-202606270058`）安装的二进制叫
  `opencode2`，配置改用 `providers.<id>`，字段名变为 `package`（原 `npm`）、
  `settings`（原 `options`）、`modelID`（原 `id`）、`capabilities`/`compatibility`
  [【源码】`packages/schema/src/config/provider.ts:112-121`]。
- 但 **v2 会自动迁移 v1 的 `provider` 写法**：`migrateProvider()` 把 `npm → package`、
  `options → settings`、`options.headers → headers`、`api → settings.baseURL`
  [【源码】`packages/core/src/v1/config/migrate.ts:110-140`]；发布出来的
  `opencode.ai/config.json` 同时收录 v1/v2 两套 key，v1-only 的标 `deprecated`
  [【源码】`packages/schema/script/config-schema.ts`（`generate()` 上方注释）]。
  **结论：写 v1 `provider` 形状在 v1 与 v2 都能用，是当前正确的兼容选择。**
- 文档站是 v1/v2 混排的（同一页同时出现 `provider` 示例与 v2-only 的
  `experimental.policies` 说明），读文档时要区分。

## 10. 未决问题

1. **`auto` 该写多大的 `limit.context`？** 需要 tokencamp 产品/运维给出 auto 池的最小
   context（或明确 auto 池的准入下限）。
2. **`/feed/models` 能否作为 tcamp 的元数据来源？** 它是无鉴权营销 feed，不属于 `/v1`
   契约；如果 tcamp 依赖它，需要 tokencamp 承诺其稳定性，否则回退到静态表。
3. **`limit.output` 取多少？** tokencamp 目录不暴露每个模型的最大输出；8k 是保守值，
   需要按模型校准。
4. **reasoning 透传**：网关是否计划在 `ChatChunk.Delta` 增加 `reasoning_content`？
   v1 范围内不改 tokencamp-pro，但值得记录为后续需求。
5. **`.env` 加载**：文档称 OpenCode 会读项目 `.env`；1.19.5 的 `Env` 只是
   `process.env` 快照 [【源码】`env/index.ts:19-21`]，`.env` 是否在 CLI 启动时注入
   未在源码中确认。tcamp 不应依赖 `.env`，只依赖真实环境变量。
6. **安装形态检测**：curl 脚本 / brew / npm 三种安装的二进制路径与签名状态不同
   （本机 npm 的 x64 构建被 SIGKILL，baseline 可用）。tcamp 的“是否已安装/版本”
   探测需要覆盖这些形态。
7. **OpenCode 2 GA 时间**：若 tcamp v1 发布时 v2 已成默认，配置形状需要同时写
   `provider` 与（可选）`providers` 吗？当前结论是不需要，v2 会迁移。

## 11. 引用

### OpenCode 官方文档

- Config — https://opencode.ai/docs/config/
  （路径与优先级、深合并、`{env:}`/`{file:}`、`small_model`、托管配置）
- Providers — https://opencode.ai/docs/providers/
  （自定义 provider 形状、`@ai-sdk/openai-compatible` vs `@ai-sdk/openai`、auth.json、
  troubleshooting）
- Models — https://opencode.ai/docs/models/
  （`provider/model` 选择格式、默认模型顺序、`small_model`）
- CLI — https://opencode.ai/docs/cli/
  （`run`、`debug config`、`models`、`auth`、`serve`、环境变量）
- 发布 schema — https://opencode.ai/config.json
  （v1+v2 合并后的 JSON Schema；`provider` 标 deprecated）

### OpenCode 源码（tag `v1.19.5`，commit `810c0eb`）

- `packages/opencode/src/config/config.ts:140-148` — 全局文件候选与首选文件
- `packages/opencode/src/config/config.ts:245-249` — 缺 `$schema` 时写回文件
- `packages/opencode/src/config/config.ts:272-274` — 全局三层加载顺序
- `packages/opencode/src/config/config.ts:415-424, 438-448, 482-490, 530-548` — 自定义/项目/目录/内联/托管层
- `packages/opencode/src/config/paths.ts:10-21` — 项目配置向上查找
- `packages/opencode/src/config/variable.ts:34-38, 61-85` — `{env:}`/`{file:}` 替换
- `packages/opencode/src/config/v2-compat.ts:117-118` — v1 不认 `providers`
- `packages/core/src/v1/config/provider.ts:24-96, 98-147` — v1 Provider/Model schema
- `packages/opencode/src/provider/provider.ts:1546-1613` — 模型合并、`id` 重映射、默认能力、limit 缺省
- `packages/opencode/src/provider/provider.ts:1633-1641, 1809-1835` — env key、`includeUsage`、baseURL 插值、apiKey 回填
- `packages/opencode/src/provider/provider.ts:1964-2020` — `getSmallModel`
- `packages/opencode/src/provider/provider.ts:2083-2089` — `parseModel`（按第一个 `/` 切）
- `packages/opencode/src/provider/transform.ts:18, 322-345, 1481-1483` — `OUTPUT_TOKEN_MAX`、interleaved 回放、`maxOutputTokens`
- `packages/opencode/src/session/prompt.ts:216-221` — 标题生成的小模型回落
- `packages/opencode/src/session/overflow.ts:10, 27` — `limit.context = 0` 时关闭溢出判断
- `packages/opencode/src/auth/index.ts:10, 27-31, 79` — auth.json 路径、`api` 类型、0600
- `packages/opencode/src/cli/cmd/providers.ts:405-445, 485` — `auth login --provider` 只认目录 id；Other 流程
- `packages/schema/src/config/provider.ts:112-121`（dev/HEAD）— v2 `providers` 形状
- `packages/core/src/v1/config/migrate.ts:110-140`（dev/HEAD）— v2 迁移 v1 provider
- `packages/schema/script/config-schema.ts`（`generate()` 上方注释）（dev/HEAD）— v1+v2 合并发布 schema

### AI SDK

- OpenAI Compatible Providers — https://ai-sdk.dev/providers/openai-compatible-providers
  （`createOpenAICompatible` 的 `baseURL`/`apiKey`/`headers`/`includeUsage`；apiKey 生成
  `Authorization: Bearer`；工具调用与 reasoning content 支持）

### tokencamp 文档

- Migrate to tokencamp — `/Users/nicholasl/Documents/build-whatever/tokencamp-www/docs-site/guides/migration.mdx`
  （`baseURL`、Bearer、`auto`/`@preset/`/`~family-latest`、Responses 子集限制）
- Install with a coding agent — 同目录 `guides/install.mdx`

### tokencamp-pro 源码（本地）

- `crates/core/src/chat.rs` — `ChatRequest`（tools/tool_choice/extra 透传）、`ChatChunk`、
  `Delta`（无 `reasoning_content`）、`Message`（无 reasoning 字段）
- `crates/gateway/src/routes/mod.rs` — `/v1/models` 与 `ModelObject` 字段
- `crates/gateway/src/pipeline/validate.rs:66-68, 104-120` — tools 能力闸门、context 预检
- `crates/gateway/src/pipeline/mod.rs:555-575` — `ContextLengthExceeded`
- `crates/gateway/src/auto.rs:17-22, 60-70` — auto 的硬过滤（tools/streaming/vision）
- `crates/gateway/src/routes/feed.rs:41-60` — `/feed/models` 的 `context_window`/`capabilities`
- `crates/gateway/tests/chat.rs:199-202` — `stream_options.include_usage` 被强制并转发
- `crates/gateway/src/upstream/openai.rs:1-5` — openai-compatible 透传与 model 重写

## 12. 实验记录

所有实验使用临时 `HOME`（`/tmp/oc-research/home`、`/tmp/oc3/home`），未读写用户真实
`~/.config/opencode`。配置写在该临时 HOME 下，mock 服务只绑定 `127.0.0.1`。

```bash
# 1) 安装（npm 平台包；本机 x64 需要 baseline 构建）
npm install --prefix /tmp/oc3/install opencode-ai@1.19.5
opencode --version            # 1.19.5

# 2) 解析配置
HOME=/tmp/oc3/home TOKENCAMP_API_KEY=sk-test-123 \
  opencode debug config       # provider/models 原样保留，apiKey 打码

# 3) 模型 id 形态
HOME=/tmp/oc3/home TOKENCAMP_API_KEY=sk-test-123 opencode models
# tokencamp/@preset/fast
# tokencamp/~deepseek/deepseek-v4-latest
# tokencamp/auto
# tokencamp/deepseek/deepseek-v4-pro

# 4) 真实请求（本地 mock 记录）
HOME=/tmp/oc3/home TOKENCAMP_API_KEY=sk-test-123 \
  opencode run --model tokencamp/auto "Reply with exactly: pong"
# → POST /v1/chat/completions, Bearer, stream:true, include_usage:true,
#   max_tokens:8192, tools:10；标题请求 tools:0

# 5) 第二轮：默认值 / 工具往返 / reasoning
opencode run -m tokencamp/auto "hi"                        # max_tokens: 32000（无 limit）
opencode run --auto -m tokencamp/auto "TOOLTEST …"
#   第二轮 messages: [system, user, assistant(tool_calls), tool(tool_call_id)]
opencode run --thinking -m tokencamp/deepseek/deepseek-v4-pro "REASONING …"
#   stdout: Thinking: I am thinking about the answer；请求体 reasoning_effort: "low"
```

观察到的事实：配置被接受；四种 model id 形态保留；请求头/体如上表；主请求与标题请求
各一次；默认 `max_tokens` 为 32000；工具调用第二轮的消息形状与 tokencamp 的
`Message` 结构一致；`reasoning_content` 在 OpenCode 侧可显示，`reasoning: true`
会额外发送 `reasoning_effort`。

未做：通过真实 tokencamp 网关的端到端请求（本报告用 mock 观察 OpenCode 的线格式，
网关侧行为全部来自源码与既有网关测试）。
