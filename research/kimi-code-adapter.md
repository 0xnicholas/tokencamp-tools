# Kimi Code 适配器契约（ticket #4）

- 状态：research 完成，2026-11-01
- 被测版本：`kimi` 2.1.1（macOS x86_64，安装于 `~/.kimi-code/bin/kimi`）
- 范围：`tcamp` v1 的 Kimi Code adapter；纯客户端，不改 tokencamp-pro
- 结论日期：2026-11-01（Kimi Code 更新频繁，见 §9）

## 0. 结论（Verdict）

**可配置，且 `type = "openai"` 就够。** Kimi Code 的 `[providers.<name>]` 接受任意
OpenAI-compatible endpoint；把 `base_url` 指向 tokencamp 的
`https://api.tokencamp.com/v1` 后，Kimi Code 会向 `/v1/chat/completions` 发
`Authorization: Bearer <key>`，并把 `model` 原样发出。这是 v1 应采用的协议路径。

**不要用 `type = "anthropic"`。** 协议层面 Kimi Code 支持 Anthropic Messages，但它的
Anthropic 客户端默认发 `x-api-key`（不是 `Authorization: Bearer`），而 tokencamp 网关
`/v1` 只认 Bearer；实测该模式下请求头是 `x-api-key`，还带
`anthropic-beta: interleaved-thinking-2025-05-14,context-management-2025-06-27` 和
`?beta=true`。tokencamp 的 `/v1/messages` 是 near-passthrough，会把这些字段转给上游；
即使加 `custom_headers` 补一个 Bearer，`x-api-key` 仍会同时发出，属于不必要的凭据暴露，
且 beta 字段的行为未在真实网关上验证。v1 不采用。

对 `tcamp` 的实现要求（详见 §5）：

1. 写 `[providers.tokencamp]`，`type = "openai"`，`base_url = "https://api.tokencamp.com/v1"`，
   `api_key` 或 `api_key_env` 二选一。
2. 为每个要暴露的模型写 `[models."tokencamp/..."]`，**必须设 `max_context_size`，
   并强烈建议设 `max_output_size`**（不设会触发 tokencamp 的 context-window 闸门，见 §8.1）。
3. 只在 `default_model` 缺失/为空时写入，绝不覆盖用户已有选择（除非显式 `--set-default`）。
4. 用 format-preserving 的 TOML 编辑器（Rust 侧 `toml_edit`）只增改自己的表；
   不重排、不删除其他 provider/model/hook/permission，不丢注释。
5. 原子写、最终 mode `0600`；写前备份或用 sidecar manifest 记录插入项，支持撤销。

## 1. Provider 形状与文件/键表

### 1.1 文件位置

| 路径 | 用途 | 依据 |
| --- | --- | --- |
| `~/.kimi-code/config.toml` | 运行时配置；首次运行自动创建 | 【文档】[Configuration files](https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/config-files.html) |
| `$KIMI_CODE_HOME/config.toml` | 数据目录迁移后的配置路径；文件名始终是 `config.toml` | 【文档】同上 |
| `~/.kimi-code/tui.toml` | TUI 偏好，与 provider 无关 | 【文档】同上 |
| `~/.kimi-code/credentials/<name>.json` | OAuth token（mode 0600，父目录 0700）；静态 API key 不放这里 | 【文档】[Providers](https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html)；【源码观察】OAuth storage |
| `~/.kimi-code/bin/kimi` | 官方安装脚本的默认可执行文件 | 【文档】[Getting started](https://www.kimi.com/code/docs/en/kimi-code-cli/guides/getting-started.html)；[install.sh](https://code.kimi.com/kimi-code/install.sh) |

### 1.2 `[providers.<name>]` 字段

| TOML key | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `type` | string | 是 | `kimi` / `openai` / `openai_responses` / `anthropic` / `google-genai` / `vertexai` |
| `base_url` | string | 否 | API base；`openai` 必须含 `/v1`，`anthropic` 的尾部 `/v1` 会被去掉（实测） |
| `api_key` | string | 三选一 | 明文写在 config.toml；与 `api_key_env`、`oauth` 互斥 |
| `api_key_env` | string | 三选一 | 环境变量名；每次请求重读；未设置/为空时请求报错并点名该变量 |
| `oauth` | table | 三选一 | 登录流程自动注入；手工通常不写。观察到的字段：`storage`（默认 `"file"`）、`key`、`oauth_host` |
| `env` | `table<string,string>` | 否 | 凭据兜底来源，如 `[providers.<name>.env]` 里的 `KIMI_API_KEY`；仅在 `api_key`/`api_key_env` 都缺席时使用 |
| `custom_headers` | `table<string,string>` | 否 | 每个请求附加的 HTTP header |

凭据优先级（【文档】Providers 页）：`api_key` 或 `api_key_env`（二选一） >
`[providers.<name>.env]` 的约定 key > 都没有则启动失败。CLI 不会自动回退到 shell 里
`export` 的 `KIMI_API_KEY`/`OPENAI_API_KEY` 等变量。

支持的 `type` 值（【文档】Providers 页的表格；与 2.1.1 二进制里的类型列表一致）：

| `type` | 协议 | 典型用途 | 对 tokencamp 的适用性 |
| --- | --- | --- | --- |
| `openai` | OpenAI Chat Completions | OpenAI 及兼容服务、DeepSeek、Qwen 等 | **推荐**；Bearer + `/v1/chat/completions` |
| `kimi` | OpenAI-compatible | Kimi Code managed service、Kimi Platform key | 可用（实测同 wire），但带 Kimi 自有语义；不选 |
| `openai_responses` | OpenAI Responses API | OpenAI 新接口 | tokencamp 有 `/v1/responses` 的 stateless 子集；v1 未采用，未测 |
| `anthropic` | Anthropic Messages | Claude 模型族 | 协议可用，但默认 `x-api-key` 与 Bearer-only 网关不兼容；不选 |
| `google-genai` | Google GenAI | Gemini API | 不适用 |
| `vertexai` | Google GenAI on Vertex | GCP Vertex | 不适用 |

### 1.3 `[models.<alias>]` 字段

`default_model` 和 `-m` 使用的都是**别名**（`models` 表的 key），不是 wire model id。
每个别名至少要有：

| TOML key | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `provider` | string | 是 | 必须指向已存在的 provider 名 |
| `model` | string | 是 | 真正发给服务端的 model id，可含 `/`、`@`、`~` |
| `max_context_size` | integer | 是 | ≥1；Kimi 用它做上下文/压缩判断 |
| `max_input_size` | integer | 否 | 单请求输入上限；一般不必写 |
| `max_output_size` | integer | 否 | 映射到请求的 `max_tokens`；**tokencamp 场景强烈建议写**（§8.1） |
| `capabilities` | array<string> | 否 | `thinking`、`image_in`、`tool_use` 等；只增不减。实测不写也会发送 25 个 tools，v1 可不写 |
| `display_name` | string | 否 | UI 显示名 |

顶层 `default_model` 是可选 string，但一旦写了就必须是 `[models]` 里的 key；
Kimi Code 不会把它当 wire model id。

## 2. 协议行为（本地 mock 实测）

实验方式：`KIMI_CODE_HOME` 指向临时目录，`kimi -p "hi"` 指向本地 HTTP mock
（返回 OpenAI SSE / Anthropic SSE），记录 request line、关键 header 和 body。
未读取、未修改用户真实 `~/.kimi-code/config.toml`。

| 配置 | 实际请求 | 认证 header | 结果 |
| --- | --- | --- | --- |
| `type = "openai"`，`base_url = "http://127.0.0.1:PORT/v1"` | `POST /v1/chat/completions` | `Authorization: Bearer tcamp-test-key` | 与 tokencamp 匹配 |
| 同上但 `base_url` 只有 host（无 `/v1`） | `POST /chat/completions` | Bearer | 路径错误；`openai` 不会自动补 `/v1` |
| `type = "kimi"`，`base_url` 含 `/v1` | `POST /v1/chat/completions` | Bearer | 可用；不选 |
| `type = "anthropic"`，`base_url` 含或不含 `/v1` | `POST /v1/messages?beta=true`（两种写法都归一成 `/v1/messages`） | `x-api-key: ...`；另有 `anthropic-beta: interleaved-thinking-2025-05-14,context-management-2025-06-27` | tokencamp 只认 Bearer，默认会 401 |

OpenAI 模式下观察到的 body key：`model`、`messages`、`tools`（25 个）、`stream`、
`stream_options`、`max_tokens`、`prompt_cache_key`；Claude 命名的模型还会带
`reasoning_effort`。`model` 原样透传，例如 `anthropic/claude-sonnet-4.5`。
tokencamp 的 `ChatRequest` 对未建模字段用 `#[serde(flatten)] extra` 接收
（`tokencamp-pro:crates/core/src/chat.rs:37-40`），ADR-0018 明确 extra passthrough；
chat→Anthropic upstream 适配器不转发这些 extra（`tokencamp-pro:crates/gateway/src/upstream/anthropic.rs:249`），
所以 `prompt_cache_key`/`reasoning_effort` 不会把 Anthropic 上游打 400。

Anthropic 模式的具体不兼容点：

- tokencamp 的 `/v1` 认证只从 `Authorization: Bearer` 取 token
  （`tokencamp-pro:crates/gateway/src/routes/direct.rs:832`）。
- Kimi 的 Anthropic 客户端发 `x-api-key`（实测）；`custom_headers = { Authorization = "Bearer ..." }`
  会**追加**一个 Authorization，但 `x-api-key` 仍然存在（实测），形成双凭据，且把
  tokencamp key 以 `x-api-key` 形式发给网关；不建议。
- Kimi 的 Anthropic 模式固定发 `thinking`、`context_management` 和 beta header；
  tokencamp `/v1/messages` 是 near-passthrough，body 里未建模的字段会 flatten 后转上游
  （`tokencamp-pro:crates/core/src/anthropic.rs:51-54`）；`anthropic-beta` header 是否
  原样转发未验证，这些 beta 字段是否被上游接受也未在真实网关验证。
- `/v1/messages` 是 direct endpoint，**不支持 `@preset/<slug>`**；preset 只在 chat
  pipeline 里解析（`tokencamp-pro:crates/gateway/src/routes/direct.rs:816-817`）。

## 3. 最小配置（已用 `kimi doctor config` 与 mock 请求验证）

```toml
# ~/.kimi-code/config.toml
default_model = "tokencamp/auto"

[providers.tokencamp]
type = "openai"
base_url = "https://api.tokencamp.com/v1"
api_key = "tc-REPLACE_ME"

[models."tokencamp/auto"]
provider = "tokencamp"
model = "tokencamp/auto"
max_context_size = 128000
max_output_size = 8192
```

要点：

- `base_url` 必须含 `/v1`：`openai` 类型把 base 当 OpenAI SDK 的 baseURL，只追加
  `/chat/completions`，不会补 `/v1`（实测）。
- `models` 的 key 是别名；`model = "tokencamp/auto"` 是 tokencamp 的规范 auto 名
  （`tokencamp-pro:crates/gateway/src/config.rs:1193`）。tokencamp 文档里也出现过裸
  `auto`（quickstart / OpenAPI 描述），但代码与测试使用 `tokencamp/auto`；v1 用规范名，
  避免依赖未确认的 shorthand。
- `max_context_size = 128000` 是保守值，不是从 tokencamp 公共 API 读到的；见 §8.2。
- `max_output_size = 8192` 不是可选项：不写时 Kimi 会按
  `min(131072, max_context_size)` 发 `max_tokens`，对 `max_context_size ≤ 131072`
  的模型等于把整个窗口都当输出上限，tokencamp 的 context 闸门会直接拒绝（§8.1）。

如果要暴露具体模型：

```toml
[models."tokencamp/anthropic-claude-sonnet-4.5"]
provider = "tokencamp"
model = "anthropic/claude-sonnet-4.5"
max_context_size = 200000
max_output_size = 16384
```

`api_key_env` 变体（不把 key 写进 config；环境变量缺失时 Kimi 会明确报错）：

```toml
[providers.tokencamp]
type = "openai"
base_url = "https://api.tokencamp.com/v1"
api_key_env = "TOKENCAMP_API_KEY"
```

实测 `api_key_env` 生效：请求头为 `Authorization: Bearer env-key-123`；未设置时
`kimi -p` 报 `Provider "tcamp" declares api_key_env = ... but the environment variable
is not set or is empty`。

## 4. 模型命名

Kimi Code 有两层名字：

- **别名**（`models` 表 key）：`default_model` 和 `-m/--model` 只接受它。
- **wire model**（`model` 字段）：原样放进请求 body，tokencamp 侧解析 catalog id、
  `tokencamp/auto`、`@preset/<slug>`、`~author/family-latest`。

| 用户看到的别名 | `model`（wire） | 可用性 | 说明 |
| --- | --- | --- | --- |
| `tokencamp/auto` | `tokencamp/auto` | 实测 | 推荐；tokencamp 的 auto 虚拟模型 |
| `tokencamp/deepseek-v4-pro` | `deepseek/deepseek-v4-pro` | 实测 | catalog id 原样 |
| `tokencamp/cheap-chat` | `@preset/cheap-chat` | 文档 + 机制 | preset 仅 chat pipeline；需要 workspace key |
| `tokencamp/deepseek-latest` | `~deepseek/deepseek-v4-latest` | 文档 + 机制 | 请求时解析到该 family 最新成员 |
| `anthropic/claude-sonnet-4.5` | `anthropic/claude-sonnet-4.5` | 实测 | 1:1 别名也可行，但含 `/`/`.`，TOML key 必须加引号 |

实测：

- `default_model = "tcamp/auto"`，`model = "auto"` 时请求 body 的 `model` 是 `auto`。
- `-m "@preset/cheap-chat"` 能选中别名并在 body 里发 `@preset/cheap-chat`。
- `-m "auto"`（没有这个别名）报 `Model "auto" is not configured in config.toml.`，
  证明 `-m` 只接受别名。
- 别名和 `model` 里出现 `/`、`.`、`@`、`~` 时，TOML 表名必须写成
  `[models."..."]`；裸 key 只允许字母数字、`_`、`-`。
- tokencamp 的 preset/family 解析发生在请求时；Kimi 侧只是字符串透传，不需要为每个
  preset 建别名，但建别名后用户体验更好（`-m tokencamp/cheap-chat`）。

建议：别名统一加 `tokencamp/` 前缀（如 `tokencamp/auto`、`tokencamp/claude-sonnet-4.5`），
避免与用户已有的 `kimi-code/*`、`openai/*` 别名冲突；别名与 catalog id 不必相同。

## 5. 非破坏性写入契约

### 5.1 定位

1. 若设置了 `KIMI_CODE_HOME`，配置是 `$KIMI_CODE_HOME/config.toml`；否则
   `$HOME/.kimi-code/config.toml`（`~` 展开）。
2. `KIMI_CODE_HOME` 只改数据目录，不改二进制安装目录；检测二进制时不要拿它拼路径
   （安装路径是 `KIMI_INSTALL_DIR`，默认 `$HOME/.kimi-code`，见 §7）。
3. 文件可能不存在；首次运行 Kimi 才创建。`tcamp` 应自己创建父目录（0700）和文件
   （0600），不要依赖 Kimi 先跑一次。

### 5.2 只增改自己的表

推荐做法（Rust）：

1. 用 `toml_edit::DocumentMut` 读原文件，保留注释、空行、其他表顺序。
2. **upsert** `[providers.tokencamp]`：
   - 不存在则新建；存在则只改 `type`/`base_url`/`api_key`（或 `api_key_env`）。
   - 保留用户可能加的 `custom_headers`、`env` 等未知字段；不要重建整张表。
3. **upsert** `[models."tokencamp/..."]`：
   - 只写 `provider`、`model`、`max_context_size`、`max_output_size`（以及可选的
     `display_name`）。
   - 如果同名 alias 已存在且 `provider` 不是 `tokencamp`，这是冲突：报错退出，不要静默覆盖。
4. **`default_model`**：
   - 不存在或为空 → 写入 `tokencamp/auto`（fresh setup 的合理默认）。
   - 已存在 → 不动，除非 `--set-default` 明确要求；tcamp 不应替用户改主模型。
5. 不删除任何其他 provider/model；不调用 `kimi provider remove`（它会连带删除引用该
   provider 的 alias，见 §6）。
6. 写回：同目录临时文件 → `fsync` → `rename`；最终 `chmod 0600`。Kimi 有 config
   watcher（`watch.enabled` 默认 true），原子替换可避免它读到半个文件。
7. 校验：`kimi doctor config <path>` 只查 TOML 语法/shape，**不查引用完整性**（§8.7）；
   `tcamp` 自己检查 `default_model` 在 `models` 里、每个 `provider` 在 `providers` 里。
8. 撤销：写前把原文件复制为 `config.toml.tcamp.bak`，或在
   `~/.kimi-code/.tcamp-state.json` 里记录插入的 alias、provider 是否新建、原
   `default_model` 值；撤销时按记录删自己的表并恢复默认值，不要整文件覆盖。

### 5.3 Kimi 自己的 writer 是否保留注释/格式？

实测（`kimi provider remove existing`，临时 HOME）：

- 顶层注释、行尾注释、其他 provider 的注释、其他表全部保留。
- 被删表前的注释会变成孤立注释（`# existing provider comment` 留在空行上方），
  Kimi 不会清理。
- 文件 mode 从 `644` 变成 `600`。
- 另一次 `kimi provider add` 保留了用户注释和 `default_model`，但删掉了一个表前空行；
  `api_key` 写成 `""`，并新增 `[providers.tokencamp.source]`。

结论：Kimi 的 writer 是 format-preserving 的（类似 `toml_edit`），但**不保证字节级不变**。
`tcamp` 若用 `toml::to_string` 整文件重写，会丢注释和用户格式；必须用 format-preserving
编辑。Kimi 的 writer 不会替 `tcamp` 清理由 `tcamp` 删除表留下的注释，所以撤销逻辑要
自己处理注释块。

## 6. `kimi provider add`（registry import）作为替代路径

`kimi provider add <url>` 会抓取一个 models.dev 形状的 `api.json`，把其中每个 provider
及其 models 写进 config。实测 schema（【二进制源码观察】+ 实测）：

```json
{
  "tokencamp": {
    "id": "tokencamp",
    "name": "tokencamp",
    "api": "https://api.tokencamp.com/v1",
    "type": "openai",
    "env": ["TOKENCAMP_API_KEY"],
    "models": {
      "auto": {
        "id": "auto",
        "name": "Auto",
        "tool_call": true,
        "reasoning": true,
        "limit": { "context": 128000, "output": 8192 }
      }
    }
  }
}
```

导入结果（实测）：

- 生成 `<providerId>/<modelKey>` 形式的 alias；上面的例子得到 `tokencamp/auto`。
  如果 registry 的 model key 本身已带 provider 前缀，别名会叠两次（如
  `tokencamp/tokencamp/auto`），与 `tcamp` 自己的命名方式不同。
- 用 `limit.context` 填 `max_context_size`，`tool_call`/`reasoning` 生成
  `capabilities`，`name` 生成 `display_name`。
- **不写有效 API key**：`api_key = ""`，只打印 `env` 提示让你自己设 `api_key_env`。
- 新增 `[providers.tokencamp.source]`（`kind = "apiJson"`、`url`、`apiKey`），启动时
  会按该 URL 刷新；registry 不可达时刷新失败。
- `setDefaultWhenUnset: false`，不会改 `default_model`。

因此它**不是** `tcamp` v1 的推荐路径：`tcamp` 仍要补 key，且引入对 registry URL 的
运行时依赖。只有在“想批量导入整套 models 元数据”时才值得考虑；导入后还要删
`source` 表并改 alias，反而更复杂。

## 7. 启动与检测

| 项目 | 事实 | 依据 |
| --- | --- | --- |
| 可执行文件名 | `kimi`；没有 `kimi-code`（`which kimi-code` 不存在） | 实测 |
| 版本 | `kimi --version` → `2.1.1`（纯版本号） | 实测 |
| 非交互 | `kimi -p "<prompt>"`；`--output-format text\|stream-json` | 【文档】Getting started / `kimi --help` |
| 配置校验 | `kimi doctor config [path]`；exit 0 合法，1 有问题 | 实测 |
| 列出 provider | `kimi provider list`（plain 输出不含 key）；`--json` **包含明文 apiKey** | 实测 |
| 官方安装脚本 | macOS/Linux，默认装到 `$HOME/.kimi-code/bin/kimi`，并改 shell rc 的 PATH | [install.sh](https://code.kimi.com/kimi-code/install.sh) |
| 安装目录覆盖 | `KIMI_INSTALL_DIR`（默认 `$HOME/.kimi-code`） | install.sh |
| npm 安装 | `npm install -g @moonshot-ai/kimi-code`；Node.js ≥ 22.19.0 | 【文档】Getting started |
| 数据目录覆盖 | `KIMI_CODE_HOME`（默认 `$HOME/.kimi-code`） | 【文档】Configuration files |
| Windows | v1 不支持；官方有 PowerShell 安装脚本，但 `tcamp` v1 范围是 macOS/Linux | 地图 issue #1 |

检测顺序建议：

1. `command -v kimi`（PATH 优先；覆盖 npm 全局安装和已改 PATH 的脚本安装）。
2. `$KIMI_INSTALL_DIR/bin/kimi`（若环境变量存在）。
3. `$HOME/.kimi-code/bin/kimi`（脚本默认路径；与 `KIMI_CODE_HOME` 无关）。
4. 找不到 → 报错并给出安装命令；v1 不自动安装。

检测到后跑 `kimi --version`，解析版本；建议 `>= 2.1.1`（本报告实测版本），
低于 2.x 时警告配置形状可能不同。`kimi doctor config <path>` 可用来校验候选文件，
但不要用它判断 `default_model`/provider 引用是否有效（§8.7）。

## 8. 已知坑 / 设计约束

### 8.1 `max_tokens` 与 tokencamp 的 context 闸门（最重要）

Kimi 在未显式设 `max_output_size` 时，实测按
`max_tokens = min(131072, max_context_size)` 发请求：

| `max_context_size` | 无 `max_output_size` 时的 `max_tokens` |
| --- | --- |
| 200000 | 131072 |
| 128000 | 128000 |
| 32000 | 32000 |
| 128000 + `max_output_size = 8192` | 8192 |

tokencamp 在 `/v1/chat/completions` 的 validate 阶段（以及 execute 阶段对 auto 选中的
真实模型再检查一次）用
`estimated_prompt_tokens + max_tokens > model.context_window` 直接拒绝
（`tokencamp-pro:crates/gateway/src/pipeline/validate.rs:102-127`、
`crates/gateway/src/pipeline/execute.rs:84`）。`estimated_prompt_tokens` 只数 messages
的字符（chars/4），不含 tools。

后果：对任何 `max_context_size ≤ 131072` 的模型，Kimi 默认把窗口全部当输出上限，
再加任何 prompt 都超窗，**每个请求都会被 tokencamp 拒绝**（`context_length_exceeded`）。
`auto` 也不能幸免：validate 阶段虚拟模型 `context_window = 0` 会跳过，但 execute
阶段选中真实模型后会再查一次。

结论：`tcamp` 必须为每个 `[models.*]` 写 `max_output_size`，且要留出 prompt 预算。
建议默认 `8192`，或按 `min(32768, max(1024, max_context_size - 24576))` 计算；
对 32k 模型取 4096 更稳（Kimi 的 system prompt + 25 个 tools 本身约 90 KB，
按 tokencamp 的 chars/4 估算约 20k tokens，见 §8.3）。

### 8.2 上下文窗口元数据拿不到

`GET /v1/models` 只返回 `id`、`object`、`created`、`owned_by`、`kind`、
`expiration_date`（`tokencamp-pro:docs/openapi.yaml` 的 `ModelObject`），**没有
context window**。`/feed/models` 有 `context_window` 和 `capabilities`
（`tokencamp-pro:crates/gateway/src/routes/feed.rs:49`），但它是给 www 营销站用的
unauthenticated feed（ADR-0038），不是客户 API 契约，v1 不应依赖。

因此 `max_context_size` 只能：

- 用保守默认（如 128000）并允许用户覆盖；
- 或按用户显式选择的模型手工给值；
- 或未来让 tokencamp 在 `/v1/models` 暴露 context（需要 tokencamp-pro 改动，超出 v1 范围）。

### 8.3 Kimi 的 system prompt/tools 很大

实测 Anthropic 模式请求的 `content-length` 为 91119 字节（system prompt + 25 个 tools）；
OpenAI 模式同样带 system prompt 和 25 个 tools，量级相当。按 tokencamp 的 chars/4
粗估，仅 messages 就约 20k tokens 量级。很小的上下文模型（16k、8k）即使把
`max_output_size` 设到 1k 也可能放不下；`tcamp` 应在模型选择时过滤或警告。

### 8.4 `auto` 的上下文是“池里最小值”

一个 `[models."tokencamp/auto"]` 只能写一个 `max_context_size`，但 auto 路由可能选到
不同窗口的模型。写大了，Kimi 会等到超过大窗口才压缩，实际被选中的小模型先撞
tokencamp 的闸门；写小了，浪费大模型窗口。保守做法：`auto` 用池内最小窗口
（如 128k），或干脆不暴露 auto，让用户选具体模型；如果 tokencamp 有 workspace
auto-router pool，理论上可以取 min，但 v1 客户端拿不到该池。

### 8.5 Anthropic 模式的双重凭据

见 §2。`custom_headers` 只能追加 Authorization，不能去掉 `x-api-key`；如果未来
tokencamp 网关开始接受 `x-api-key`，或 Kimi 支持关闭 `x-api-key`，这条路径才值得重开。
v1 明确不做。

### 8.6 文档与实测不一致：`max_output_size`

官方配置文档说 `max_output_size` “currently only the anthropic provider reads it”，
但 2.1.1 实测 `type = "openai"` 也把它映射成 `max_tokens`（8192 生效）。以实测为准，
同时在 `tcamp` 里不要把该字段只写给 anthropic。

### 8.7 `kimi doctor config` 不查引用完整性

实测：

- `default_model = "missing/model"` → doctor 仍 OK。
- `models.*.provider = "nope"` → doctor 仍 OK。

它只验证 TOML 语法和字段 shape。`tcamp` 必须自己检查 `default_model` 是否在
`models` 里、每个 `provider` 是否在 `providers` 里，否则用户会在启动时才看到
`Model "..." is not configured` / provider 解析错误。

### 8.8 `kimi provider list --json` 会打印明文 key

实测输出包含 `"apiKey": "tcamp-test-key"`。`tcamp` 做检测/诊断时不要把它写进日志、
不要放进错误消息；解析 config.toml 本身即可（但注意不要在日志里打印 key 值）。

### 8.9 配置 watcher 与并发

Kimi 默认监听 config.toml（`watch.enabled`），编辑过程中可能 reload。`tcamp` 必须
原子替换（temp + rename），不要原地 truncate/write；否则 Kimi 可能读到半截 TOML
并回退/报错。

### 8.10 上游 `max_tokens` 上限

即使过了 tokencamp 的 context 闸门，Kimi 默认的 131072 也可能超过上游模型的最大输出
（tokencamp 会把请求转给上游）。`max_output_size` 设小既能过闸门，也能降低上游 400
的概率；具体上限由 tokencamp/上游处理，未在真实网关验证。

## 9. 版本与日期 caveat

- 本报告实测 `kimi` **2.1.1**（macOS x86_64，2026-11-01）；官方文档同日抓取。
- Kimi Code 是 TypeScript/npm 分发的 CLI，更新频繁（`kimi upgrade`）；provider 类型
  列表、`max_tokens` 默认值、`provider list --json` 的输出都可能随版本变化。
- 未验证 2.1.1 之前的版本；`KIMI_CODE_HOME`、`type = "openai"`、`api_key_env` 在
  2.1.1 上可用，不能保证 1.x 相同。
- `openai_responses` 未实测；tokencamp 的 `/v1/responses` 是 stateless 子集（地图
  issue #1 的已核实约束），不在 v1 Kimi adapter 范围。
- 本环境没有可用的 tokencamp 生产 key/网关（`api.tokencamp.com` 请求超时），wire
  行为全部对本地 mock 验证；真实网关的端到端验证仍是 open question（§10）。

## 10. 未决问题

1. **真实网关端到端**：用真实 tokencamp key 跑一次 `kimi -p`，确认
   `/v1/chat/completions` 接受 Kimi 的 `prompt_cache_key`、`reasoning_effort`、
   `stream_options.include_usage`，并确认流式 usage 计费正常。
2. **裸 `auto` 是否可用**：tokencamp quickstart/OpenAPI 写 `"auto"`，auto-router 文档
   与代码写 `"tokencamp/auto"`。v1 用规范名；若要支持裸 `auto`，需在网关侧确认。
3. **context window 来源**：是否接受 `/feed/models` 作为可选 enrichment（它不是客户
   API 契约），还是只用保守默认 + 用户覆盖；这会影响 `auto` 的 `max_context_size`。
4. **`max_output_size` 默认策略**：固定 8192，还是按 context 计算；对 32k 模型是否
   直接拒绝配置。
5. **默认模型策略**：仅在 `default_model` 缺失时写，还是提供 `--set-default`；
   与地图的“可安全撤销”目标如何配合。
6. **provider 名冲突**：用户已有 `[providers.tokencamp]`（手工配置）时，覆盖、报错
   还是加后缀；建议报错并让用户选择。
7. **模型暴露范围**：只写 `tokencamp/auto`，还是把整个 catalog 写成几十个 alias；
   后者会让 config 膨胀，但 `-m` 体验更好。
8. **Anthropic 路径是否永久放弃**：如果 tokencamp 未来接受 `x-api-key` 或 Kimi 支持
   自定义认证头替换，可重新评估；当前不做。
9. **最低 Kimi 版本**：2.1.1 已验证；是否需要检测并拒绝更老版本，需看 2.0.x 的
   config 形状。

## 11. 引用

### 官方 Kimi Code 文档

- Providers and models — https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html
  （provider 类型、字段、凭据优先级、custom registry、OAuth）
- Configuration files — https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/config-files.html
  （`KIMI_CODE_HOME`、`providers`/`models` 全字段、`default_model`、model overrides）
- Getting started — https://www.kimi.com/code/docs/en/kimi-code-cli/guides/getting-started.html
  （安装脚本、npm、`kimi -p`、数据目录）
- 安装脚本 — https://code.kimi.com/kimi-code/install.sh
  （`KIMI_INSTALL_DIR`、`$HOME/.kimi-code/bin`、PATH）

### tokencamp 文档

- Quickstart — https://docs.tokencamp.com/quickstart
  （`base_url = https://api.tokencamp.com/v1`、Bearer、`model: auto`）
- Presets & model aliases — https://docs.tokencamp.com/cookbook/presets
  （`@preset/slug`、`~author/family-latest`）
- Auto-Router — https://docs.tokencamp.com/guides/auto-router
  （`tokencamp/auto` 的行为）

### tokencamp-pro 源码（本地）

- `crates/gateway/src/routes/direct.rs:832` — `/v1` 只从 `Authorization: Bearer` 取 token
- `crates/gateway/src/routes/direct.rs:816-817` — direct endpoint 无 preset 层
- `crates/gateway/src/routes/messages.rs:1-11` — `/v1/messages` near-passthrough
- `crates/core/src/anthropic.rs:51-54` — MessagesRequest 的 `extra` flatten
- `crates/core/src/chat.rs:37-40` — ChatRequest 的 `extra` flatten（ADR-0018 passthrough）
- `crates/gateway/src/upstream/anthropic.rs:249` — chat→Anthropic 不转发 extra
- `crates/gateway/src/pipeline/validate.rs:102-127` — context-window 闸门
- `crates/gateway/src/pipeline/execute.rs:84` — auto 选中后对真实模型再查
- `crates/gateway/src/routes/feed.rs:49` — `/feed/models` 的 `context_window`
- `crates/gateway/src/config.rs:1193` — `tokencamp/auto` 规范名
- `docs/openapi.yaml` — `ModelObject` 无 context 字段；`/v1/messages`、`/v1/chat/completions`

### 本报告实验

所有 Kimi 实验都在临时 `KIMI_CODE_HOME`（`/tmp/tcamp-kimi-*`）下进行，未读取/修改
用户真实 `~/.kimi-code/config.toml`。mock 服务只绑定 `127.0.0.1`，记录 request line、
关键 header 和 body 摘要；实验后已关闭。

关键命令：

```bash
kimi --version
KIMI_CODE_HOME=/tmp/tcamp-kimi-g kimi -p "hi"
kimi doctor config /tmp/tcamp-final/config.toml
KIMI_CODE_HOME=/tmp/tcamp-kimi-g kimi provider list --json
```

观察到的事实：

- `type=openai` + `base_url=.../v1` → `POST /v1/chat/completions`，`authorization: Bearer ...`，
  body `model` 原样，`tools_count=25`。
- `type=openai` + `base_url` 无 `/v1` → `POST /chat/completions`（路径错误）。
- `type=anthropic` → `POST /v1/messages?beta=true`，`x-api-key`，`anthropic-beta` 两条；
  `base_url` 带不带 `/v1` 都归一成 `/v1/messages`。
- `custom_headers` 追加的 `authorization` 与 `x-api-key` 同时存在。
- `max_tokens` 实测规律与 `max_output_size` 生效见 §8.1。
- `provider remove` 保留注释，config mode 变 600；`provider add` 写 `api_key=""` + `source`。
- `kimi provider list --json` 输出明文 `apiKey`。
- `kimi doctor config` 对悬空 `default_model`/`provider` 仍返回 OK。
