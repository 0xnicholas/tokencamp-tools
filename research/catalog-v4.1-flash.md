# `deepseek/deepseek-v4.1-flash` catalog 契约与共享暴露（ticket #39）

- 状态：research 完成，2026-11-14
- 仓库基线：`tokencamp-tools@4ab275c`（`main`，2026-11-14）
- 平台基线：`tokencamp-pro@60718c1`（2026-11-14）；`config/default.yaml` 的 v4.1-flash 条目在 HEAD 的 `362-385` 行
- 范围：只回答 catalog 契约、共享暴露、测试/文档影响与发布顺序；不写生产代码，不改 `main`
- 结论日期：2026-11-14。`/v1/models` 的真实 API 在本机不可达（连接超时），平台侧结论均为 catalog/源码级，未做真实网关请求

## 0. 结论（Verdict）

**Catalog 暴露：GO。Tool loop：条件 GO。**

- 规范 id 是 `deepseek/deepseek-v4.1-flash`，family 是 `deepseek/deepseek-v4-flash`。它是 DeepSeek 的 cheap-fast/vision 线，**不是** `deepseek/deepseek-v4-pro` 的变体；两者 family、upstream 名、定位都不同。
- 模型原生能力：1,048,576 context、**393,216（384K）max output**、text+image、streaming/tools/JSON schema、thinking + non-thinking（默认 thinking）。这些都有 DeepSeek 官方文档和 tokencamp-pro catalog 双重来源。
- 共享暴露：`EXPOSED_MODELS` 从 5 项改为 6 项；Pi 和 Kimi 各增加 1 个模型；dsh 复用同一张表，暴露 6 个模型并把默认模型设为 v4.1-flash。**Claude 的 `CLAUDE_SELECTABLE_MODELS`、`CLAUDE_SLOTS` 和四个槽位全部不变。**
- tcamp 写入的 max output 是**curated 上限**，不是模型原生上限：Pi/dsh 建议 `32768`，Kimi 沿用 `8192`，Claude 仍是 `32768`。原生 `393216` 不写进 Pi/Kimi/dsh，除非产品明确要放开输出上限；见 §2.1。
- `/v1/models` 公开列表和 `/v1/models/{id}` 不需要 tcamp 改代码：tokencamp-pro 的列表直接来自非过期 catalog，v4.1-flash 已有 deployment，会自然出现；`preflight` 只在“本次选中的模型”上查 `/v1/models/{id}`，也没有硬编码模型名。
- 一个必须先验证的**网关级阻塞风险**：DeepSeek thinking 模式在带 tools 的多轮请求里要求回传 `reasoning_content`，而 tokencamp-pro 的 `ChatRequest::Message` 没有这个字段（也没有 `flatten`），请求和响应都会丢掉它。Pi/Kimi/dsh 的 tool loop 若走 thinking 模式，第二轮可能 400。解决方式见 §2.4；在这个问题闭环前，不要把“v4.1-flash 可用于 coding harness 的 tool loop”当作已验证事实。
- 发布建议：catalog 更新单独发一个 minor（建议 `v0.2.0`），dsh 随后发 `v0.3.0`。不要把 dsh 作为 v4.1-flash 的首次暴露；dsh 默认模型依赖已发布的 catalog。若两个 PR 都已就绪，也可以同一版本发布，但保留两个独立 commit/PR 和两条 changelog。

## 1. 元数据表

| 字段 | 值 | 依据 / 说明 |
| --- | --- | --- |
| canonical id | `deepseek/deepseek-v4.1-flash` | tokencamp-pro `config/default.yaml:362` `name:`；这也是唯一会被 tcamp 写进配置的 id |
| catalog aliases | 无 | 该条目没有 `aliases:`；同一文件的 v4-pro 有 `aliases: ["deepseek-pro"]`，v4.1-flash 没有。不要把它当成已有 alias |
| upstream / 第一方模型名 | `deepseek-flash` | catalog `upstream_model: deepseek-flash`（`default.yaml:382`）；DeepSeek 文档写明 `deepseek-flash` 由 `DeepSeek-V4.1-Flash-1017` 服务 |
| DeepSeek 仍接受的旧名 | `deepseek-v4-flash`、`deepseek-v4-flash-vision-exp` | DeepSeek 文档：旧模型已退役，请求由 V4.1-Flash 服务并按 Flash 计费。**这是上游 API 名，不是 tcamp catalog alias**，不要写进 tcamp 暴露表 |
| tcamp Pi alias（新增，本地约定） | `deepseek-v4.1-flash` | 与现有 Pi alias 的“去 vendor 前缀”约定一致（`deepseek-v4-pro`、`glm-5.3`、`kimi-k2.7-code`） |
| tcamp Kimi alias（新增，本地约定） | `tokencamp/deepseek-v4.1-flash` | 与现有 Kimi alias 的 `tokencamp/<short>` 约定一致 |
| display name（tcamp 新定） | `DeepSeek V4.1 Flash` | catalog 没有独立的 display name 字段，只有 `display.desc`；沿用现有 `DeepSeek V4 Pro` / `GLM 5.3 Flash` 的写法 |
| family | `deepseek/deepseek-v4-flash` | catalog `family:`（`default.yaml:367`） |
| 与 v4-pro 的关系 | 同代、不同产品线；不是 pro 的别名或子型号 | v4-pro：family `deepseek/deepseek-v4-pro`，upstream `deepseek-v4-pro`，旗舰-reasoning、text-only、2026-08-13；v4.1-flash：family `deepseek/deepseek-v4-flash`，upstream `deepseek-flash`，cheap-fast、vision、2026-09-10。两者都是 MIT |
| context window | 1,048,576（1M） | catalog `context_window: 1048576`；DeepSeek 文档 `CONTEXT LENGTH 1M` |
| max output（原生） | 393,216（384K） | DeepSeek 文档 `MAX OUTPUT MAXIMUM: 384K`；384 × 1024 = 393,216。catalog **没有** max output 字段，不能从 catalog 得到这个数 |
| input modalities | text + image | catalog `capabilities.vision: true`；DeepSeek 文档 `Vision ✓` |
| streaming | 支持 | catalog `streaming: true`；DeepSeek 文档 chat completions 支持 `stream: true` |
| tools / tool calls | 支持 | catalog `tools: true`；DeepSeek 文档 `Tool Calls ✓`。注意 §2.4 的 thinking + tools 前置条件 |
| JSON schema | 支持 | catalog `json_schema: true`；DeepSeek 文档 `Json Output ✓` |
| reasoning | thinking + non-thinking；thinking 默认开启，默认 effort `high` | DeepSeek Thinking Mode 文档：`Thinking mode is enabled by default, with the default effort being high`；OpenAI 格式用 `thinking: {type: enabled/disabled}` 和 `reasoning_effort: low/high/max` |
| reasoning 输出字段 | `reasoning_content`（与 `content` 同级） | DeepSeek Thinking Mode 文档；带 tools 时必须在后续请求回传 |
| released_on | 2026-09-10 | catalog `released_on` 与 `display.date` |
| license | MIT | catalog `license.id: mit` |
| price（用户侧） | prompt $0.0030 / completion $0.0120 per 1M | catalog `price`；upstream cost $0.30 / $1.20 peak（valley $0.15 / $0.60） |
| deployment | `deepseek` / `openai_compatible` / `https://api.deepseek.com` | catalog `deployments[0]`；`upstream_model: deepseek-flash`，weight 100 |

### 1.1 命名结论

- **规范 id 只有一个**：`deepseek/deepseek-v4.1-flash`。tokencamp-pro catalog 没有给它的 alias；`deepseek-flash` 是 upstream 名，`deepseek-v4-flash` / `deepseek-v4-flash-vision-exp` 是 DeepSeek 仍接受的旧名。tcamp 不应把这三个名字加进 `EXPOSED_MODELS`。
- **tcamp 自己新增两个 alias**：Pi 的 `deepseek-v4.1-flash`，Kimi 的 `tokencamp/deepseek-v4.1-flash`。这两个 alias 只存在于 tcamp 的 harness 适配层，不改变 tokencamp-pro catalog。
- 如果产品希望用户也能在 tcamp 里输入 `deepseek-flash`，那应该先在 tokencamp-pro catalog 加 `aliases: ["deepseek-flash"]`（改另一个仓库），而不是在 tcamp 里造一个与 catalog 不一致的别名。本 ticket 不建议这样做；dsh 的默认模型直接用 canonical id 即可。
- display name 用 `DeepSeek V4.1 Flash`，不要写 `DeepSeek V4.1 Flash (tokencamp)`；后者会把 provider 名混进模型名，与现有条目不一致。

## 2. 各 harness 的契约

### 2.1 context / max output

| Harness | tcamp 写入的 context | tcamp 写入的 max output | 原生能力 | 说明 |
| --- | --- | --- | --- | --- |
| Pi | `1048576` | `32768` | `393216` | `maxTokens` 来自 `catalog.max_output`；现有 `GLM_MAX_OUTPUT_TOKENS = 32768` 是所有非 auto 模型的 curated 上限。不写会落 Pi 默认 `16384`，把 1M 模型写成 128k 也会让 Pi 过早压缩 |
| Kimi Code | `1048576` | `8192` | `393216` | `MAX_OUTPUT_SIZE = 8192` 是 harness 级常量，对全部 alias 生效；它避免 Kimi 默认 `min(131072, max_context_size)` 触发 tokencamp 的 context 闸门 |
| Claude Code | 不适用 | 不适用 | `393216` | v4.1-flash **不进入** `CLAUDE_SELECTABLE_MODELS`；Claude 仍是 `CLAUDE_MAX_OUTPUT_TOKENS = 32768`。若未来把它加进 Claude，context 取主模型+槽位最小已知窗口，输出仍是 32768 |
| dsh | `1048576` | 建议 `32768` | `393216` | dsh 的 `maxTokens` 既声明能力，也会成为该模型的 per-request 默认；不写会落 dsh 的 `DEFAULT_MAX_TOKENS = 128000`（不等于模型原生值）。建议与 Pi 对齐 32768；若要放开到 393216，应作为单独决策并验证网关/成本 |

**结论：不要写 393216。** tcamp 的 curated 表一直是保守输出上限（Pi/Claude 32768、Kimi 8192），没有按模型原生值写。v4.1-flash 的原生 384K 只作为“模型能力”记录；把它写进 Pi/dsh 会改变现有输出策略，且可能放大单次请求成本，不属于本 ticket。

`GLM_CONTEXT_TOKENS` / `GLM_MAX_OUTPUT_TOKENS` 这两个名字已经同时用于 DeepSeek 和 Kimi，属于历史命名遗留。本 ticket 可以继续复用它们，避免无谓 diff；若要重命名，另开 cleanup，不要和模型新增混在一起。

### 2.2 input modalities

| Harness | v4.1-flash 的 image 暴露 | 需要改什么 |
| --- | --- | --- |
| Pi | 必须写 `"input": ["text", "image"]` | 当前 `model_fields()` 只对 `HAIKU_MODEL` 加 `"image"`（`src/harness/pi.rs:272-275`）。新增条件 `id == models::HAIKU_MODEL || id == models::DEEPSEEK_V4_1_FLASH_MODEL` |
| Kimi Code | 当前 tcamp 契约不写 `capabilities`，因此 Kimi 不会声明 image | Kimi 的 `[models.<alias>]` 支持可选的 `capabilities = ["thinking", "image_in", "tool_use"]`。现有 5 个 alias 都没写；为保持一致，本 ticket 可以只加 alias、不写 capabilities。若要让 Kimi 真正接受图片，需给该 alias 写 `capabilities`，并做一次真实 image 请求验证。见 §2.5 |
| Claude Code | 不适用 | v4.1-flash 不可选 |
| dsh | 必须写 `input: [text, image]` | dsh 的 hand-declared model 默认 `[text]`；不写会把 image 在发送前拒掉。见 §3.4 |

### 2.3 reasoning / thinking

- DeepSeek 官方语义：thinking 默认开启，默认 effort `high`；`thinking: {type: "disabled"}` 关闭，`reasoning_effort: low/high/max` 调 effort。thinking 模式下 `temperature`、`presence_penalty`、`frequency_penalty` 不生效（不报错，但被忽略）；`top_p` 在 deepseek-flash 上只在 thinking 模式生效，范围 0.95–1.0。
- **Pi**：现有 tcamp 不写 `reasoning`，Pi 的默认是 `false`，所以 Pi 不会主动声明 thinking，也不会发 `reasoning_effort`。这不等于上游不 thinking：DeepSeek 仍会按默认开启 thinking。若要暴露 thinking，需要模型级 `reasoning: true`、`thinkingLevelMap`，以及 `compat.thinkingFormat: "deepseek"` / `supportsReasoningEffort: true`；这会影响请求形状和 tool loop，本 ticket 不建议顺手打开。先保持与 v4-pro 相同的“不声明 reasoning”策略。
- **Kimi Code**：`[models.<alias>]` 的 `capabilities` 可声明 `thinking`，但 tcamp 当前对所有 alias 都不写。Kimi 是否主动发 `reasoning_effort` 取决于它自己的模型能力判断；tcamp 不写 `capabilities` 时不要承诺 thinking UI 生效。
- **dsh**：hand-declared model 没有 `reasoningEfforts` 就不声明 reasoning；要提供 Effort 菜单，需在模型上写 `reasoningEfforts`，并用 `compat.thinkingFormat: "deepseek"` 让 `off` 发 `thinking: {type: disabled}`。DeepSeek 的 off 不能只靠省略字段，因为默认是 thinking。见 §3.4。
- **Claude Code**：全局 `MAX_THINKING_TOKENS = 0`，且 v4.1-flash 不可选；无变化。
- **共同前置条件**：见 §2.4。只要走 thinking + tools，`reasoning_content` 必须能被网关往返；否则不要默认开启 thinking。

### 2.4 阻塞风险：thinking + tools 需要 `reasoning_content` 往返

DeepSeek Thinking Mode 文档明确：

> If the request carries the tools parameter: the reasoning_content of all previous turns should be passed back to the API and will be concatenated into the context. ... If your code does not correctly pass back reasoning_content, the API will return a 400 error.

tokencamp-pro 当前实现：

- 客户端请求被反序列化为 `tokencamp_core::chat::ChatRequest`（`crates/gateway/src/routes/mod.rs:198`）。
- `Message` 只有 `role` / `content` / `name` / `tool_calls` / `tool_call_id`，没有 `reasoning_content`，也没有 `#[serde(flatten)]`（`crates/core/src/chat.rs:57-69`）。
- 上游请求由 `adapt_request()` 直接 `request.clone()` 后序列化（`crates/gateway/src/upstream/openai.rs:156-168`），不会补回被 `Message` 丢掉的字段。
- 响应侧 `ChatCompletion.choices[].message` 和流式 `Delta` 也复用 `Message` / 没有 `reasoning_content`（`crates/core/src/chat.rs:90-101, 166-175`），所以客户端也拿不到 reasoning 内容。

【推断，未做真实网关请求】在 thinking 模式下带 tools 的两轮请求，第一轮模型返回 `reasoning_content`，网关丢弃；第二轮客户端无法回传，DeepSeek 按文档返回 400。现有的 `tests/mock_pi.rs` / `tests/mock_kimi.rs` 只 mock `zhipu/glm-5.3`，不会触发这个校验，所以测试全绿也不能证明 v4.1-flash 的 tool loop 可用。

实现前必须二选一：

1. **（推荐）tokencamp-pro 补 `reasoning_content` 往返**：在 `tokencamp_core::chat::Message` 加 `Option<String>`（请求/响应共用），在 `Delta` 加流式字段；上游 OpenAI-compatible 路径原样转发。这是另一个仓库的改动，应单独开 ticket，不能由 tcamp 侧绕过。
2. **强制 non-thinking**：Pi 用模型级 `samplingParams: {"thinking":{"type":"disabled"}}`；dsh 用 `reasoningEfforts` + `compat.thinkingFormat: "deepseek"` 并默认 `off`；Kimi 是否能强制关闭需要实测。该方案会牺牲 thinking 能力，而且三个 harness 的配置方式不统一，只能作为临时降级。

在这条闭环前：

- catalog 暴露本身可以发；Pi/Kimi 的“模型可选、可配置、可 preflight”可以发。
- 不要在 release note 或 SPEC 里写“v4.1-flash tool loop 已验证”。
- 若 dsh 默认模型设为 v4.1-flash，dsh 的 E2E 必须包含两轮 tool call；失败则按 map #37 的 GO/NO-GO 规则处理。

### 2.5 Kimi 的 `capabilities`（可选，不作为本 ticket 的默认改动）

Kimi 的 `[models.<alias>]` 支持：

```toml
capabilities = ["thinking", "image_in", "tool_use"]
```

现有 tcamp 的 5 个 alias 都不写这个字段，Kimi 仍会发送 tools；但 image 输入是否被接受取决于 `image_in`。v4.1-flash 是 vision 模型，若要让 Kimi 用户真正用图片，需要：

- 给 `Alias` 结构加一个可选的 `capabilities: &'static [&'static str]`；
- 只对 v4.1-flash 写 `["image_in", "tool_use"]`（`thinking` 先不写，避免和 §2.4 冲突）；
- 用 `kimi doctor config` + 一次真实 image 请求验证。

这属于“Kimi vision 暴露”增量，不是 catalog 6 模型本体的必需项。建议本 ticket 先保持 5 个旧 alias 的现状，另开 follow-up；否则会把一个未验证的 Kimi 能力混进 catalog 变更。

## 3. 精确代码改动

### 3.1 `src/models.rs`

当前（`main@4ab275c`）：

- `EXPOSED_MODELS` 是 `[CatalogModel; 5]`，顺序为 `zhipu/glm-5.3`、`deepseek/deepseek-v4-pro`、`zhipu/glm-5.3-flash`、`kimi/kimi-k2.7-code`、`tokencamp/auto`。
- `catalog_model()` 只做 canonical id 的精确匹配；alias 由各 harness 自己处理。

建议改动：

```rust
pub const DEEPSEEK_V4_1_FLASH_MODEL: &str = "deepseek/deepseek-v4.1-flash";

pub const EXPOSED_MODELS: [CatalogModel; 6] = [
    // 现有 5 项保持原顺序……
    CatalogModel {
        id: DEEPSEEK_V4_1_FLASH_MODEL,
        context_window: GLM_CONTEXT_TOKENS,
        max_output: GLM_MAX_OUTPUT_TOKENS,
    },
    // ……
];
```

- 新条目放在 `tokencamp/auto` **之后**，保持现有 5 项的索引/顺序不变，也匹配 map #37 列出的 6 模型顺序（v4.1-flash 最后）。若产品更希望 vendor 分组（放在 v4-pro 后面），只影响 Pi/Kimi 新文件的 `models[]` 顺序和测试向量，不影响语义；实现时二选一并在 PR 里说明。
- `catalog_model()` 不需要改：新 id 自动命中。
- `CLAUDE_SELECTABLE_MODELS` **不改**，仍是：
  `[DEFAULT_MAIN_MODEL, OPUS_MODEL, HAIKU_MODEL, KIMI_MODEL]`（4 项）。
- `CLAUDE_SLOTS` **不改**，仍是 opus/sonnet/haiku/fable 四项。
- `CLAUDE_MAX_OUTPUT_TOKENS` / `UNKNOWN_CONTEXT_TOKENS` / `claude_context_tokens()` / `claude_model_ids()` **都不改**。
- 不建议为“更好听的常量名”重命名 `GLM_CONTEXT_TOKENS` / `GLM_MAX_OUTPUT_TOKENS`；这会扩大 diff，且与本次行为无关。

### 3.2 Pi `src/harness/pi.rs`

三处：

1. `MODEL_NAMES`（`30-36`）从 `[(&str, &str); 5]` 改为 6，追加：
   ```rust
   (models::DEEPSEEK_V4_1_FLASH_MODEL, "DeepSeek V4.1 Flash"),
   ```
2. `MODEL_ALIASES`（`38-44`）从 5 改为 6，追加：
   ```rust
   ("deepseek-v4.1-flash", models::DEEPSEEK_V4_1_FLASH_MODEL),
   ```
   不建议同时加 `("deepseek-flash", ...)`：catalog 没有这个 alias，会增加一个 tcamp 独有的名字，和 §1.1 的结论冲突。如果产品明确需要，先改 tokencamp-pro catalog。
3. `model_fields()`（`270-291`）的 image 条件从：
   ```rust
   if id == models::HAIKU_MODEL {
       input.push(CstInputValue::from("image"));
   }
   ```
   改为：
   ```rust
   if id == models::HAIKU_MODEL || id == models::DEEPSEEK_V4_1_FLASH_MODEL {
       input.push(CstInputValue::from("image"));
   }
   ```
   其余字段自动生效：`maxTokens` 来自 `catalog.max_output`（32768），`contextWindow` 来自 1048576，`compat` 沿用 provider/model 的四个开关。

`exposed_ids()`、`merge_models()`、`managed_model_ids()`、`inspect_models()` 都不需要改：它们都从 `EXPOSED_MODELS` 派生。已有文件的 merge 会把新模型追加到 `models[]` 末尾；这是现有行为，不需要为“顺序”额外排序。

### 3.3 Kimi `src/harness/kimi.rs`

- `ALIASES`（`27-53`）从 `[Alias; 5]` 改为 6，追加（建议放在 `tokencamp/auto` 之后，与 §3.1 的顺序一致）：
  ```rust
  Alias {
      name: "tokencamp/deepseek-v4.1-flash",
      model: models::DEEPSEEK_V4_1_FLASH_MODEL,
      display_name: "DeepSeek V4.1 Flash",
  },
  ```
- `MAX_OUTPUT_SIZE`（`18`）保持 `8192`，不要按模型改成 32768 或 393216。
- `DEFAULT_ALIAS` 保持 `tokencamp/glm-5.3`；Kimi 的默认模型不变。
- `merge_config()`（`163-177`）会自动为新 alias 写 `provider`、`model`、`max_context_size = 1048576`、`max_output_size = 8192`、`display_name`。
- `resolve_model()` 会自动接受新 alias 和 canonical id；`exposed_aliases()` / `exposed_model_ids()` / manifest 都自动派生。
- 若决定加 Kimi `capabilities`（§2.5），需要改 `Alias` 结构和 `merge_config()` 的写入逻辑；本 ticket 默认不做。

### 3.4 dsh（未来，不在本 ticket 实现）

按 map #37 的已定方向：专属 profile `$DSH_HOME/profiles/tokencamp/`，`dsh --profile tokencamp`，provider 走 `llm-pi-ai` 的 custom provider，`api: openai-completions`，`apiKeyEnv: TOKENCAMP_API_KEY`。

模型列表（6 个，顺序可跟 `EXPOSED_MODELS`）：

| # | model id | display name | contextWindow | maxTokens | input |
| --- | --- | --- | --- | --- | --- |
| 1 | `zhipu/glm-5.3` | GLM 5.3 | 1048576 | 32768 | `[text]` |
| 2 | `deepseek/deepseek-v4-pro` | DeepSeek V4 Pro | 1048576 | 32768 | `[text]` |
| 3 | `zhipu/glm-5.3-flash` | GLM 5.3 Flash | 1048576 | 32768 | `[text, image]` |
| 4 | `kimi/kimi-k2.7-code` | Kimi K2.7 Code | 262144 | 32768 | `[text]` |
| 5 | `tokencamp/auto` | Tokencamp Auto | 200000 | 8192 | `[text]` |
| 6 | `deepseek/deepseek-v4.1-flash` | DeepSeek V4.1 Flash | 1048576 | 32768 | `[text, image]` |

配置文件形状（`$DSH_HOME/profiles/tokencamp/cordis.patch.yml`）：

```yaml
- id: llm-pi-ai
  config:
    providers:
      tokencamp:
        apiKeyEnv: TOKENCAMP_API_KEY
        api: openai-completions
        baseURL: <tokencamp-root>/v1
        compat:
          supportsDeveloperRole: false
          maxTokensField: max_tokens
        models:
          - id: zhipu/glm-5.3
            name: GLM 5.3
            contextWindow: 1048576
            maxTokens: 32768
            input: [text]
          - id: deepseek/deepseek-v4-pro
            name: DeepSeek V4 Pro
            contextWindow: 1048576
            maxTokens: 32768
            input: [text]
          - id: zhipu/glm-5.3-flash
            name: GLM 5.3 Flash
            contextWindow: 1048576
            maxTokens: 32768
            input: [text, image]
          - id: kimi/kimi-k2.7-code
            name: Kimi K2.7 Code
            contextWindow: 262144
            maxTokens: 32768
            input: [text]
          - id: tokencamp/auto
            name: Tokencamp Auto
            contextWindow: 200000
            maxTokens: 8192
            input: [text]
          - id: deepseek/deepseek-v4.1-flash
            name: DeepSeek V4.1 Flash
            contextWindow: 1048576
            maxTokens: 32768
            input: [text, image]
```

默认模型：

```yaml
- id: agent-default-model
  config:
    provider: tokencamp
    model: deepseek/deepseek-v4.1-flash
```

要点：

- dsh 的 hand-declared model 不写 `input` 就是 `[text]`，v4.1-flash 必须显式 `[text, image]`。
- `maxTokens` 会同时成为该模型的 per-request 默认；不写会落 dsh 的 `DEFAULT_MAX_TOKENS = 128000`，不是模型原生 384K，也不是 tcamp 的 curated 32768。
- 若要提供 Effort 菜单，给 v4.1-flash 加 `reasoningEfforts`（至少 `off` + 一个非 off），并加 `compat.thinkingFormat: deepseek`；`off` 才会发 `thinking: {type: disabled}`。在 §2.4 闭环前，建议 dsh 默认 `reasoningEffort: off`，或先不声明 `reasoningEfforts`。
- dsh 的 `maxTokens` 若写 32768，会固定每次请求的输出上限；这与 Pi 的 curated 策略一致。若产品要 393216，需单独验证 tokencamp 网关是否接受、成本是否可接受。

### 3.5 暴露矩阵

| 模型 | Claude 槽位 | Claude 可选 | Pi | Kimi | dsh | 默认模型 |
| --- | --- | --- | --- | --- | --- | --- |
| `zhipu/glm-5.3` | sonnet / main | 是 | 是 | 是 | 是 | Pi/Kimi/Claude 主模型 |
| `deepseek/deepseek-v4-pro` | opus / fable | 是 | 是 | 是 | 是 | 否 |
| `zhipu/glm-5.3-flash` | haiku | 是 | 是 | 是 | 是 | 否 |
| `kimi/kimi-k2.7-code` | 无 | 是 | 是 | 是 | 是 | 否 |
| `tokencamp/auto` | 无 | 否 | 是 | 是 | 是 | 否 |
| `deepseek/deepseek-v4.1-flash` | **无** | **否** | **是（新增）** | **是（新增）** | **是（新增）** | **dsh 默认（新增）** |

- Claude 的 `CLAUDE_SELECTABLE_MODELS` 仍是 4 项：`zhipu/glm-5.3`、`deepseek/deepseek-v4-pro`、`zhipu/glm-5.3-flash`、`kimi/kimi-k2.7-code`。v4.1-flash 不进 Claude。
- dsh 的默认模型按 map #37 定为 `deepseek/deepseek-v4.1-flash`；这是 dsh 的默认，不是 Pi/Kimi 的默认。Pi/Kimi 默认仍是 `zhipu/glm-5.3` / `tokencamp/glm-5.3`。
- dsh 的 6 个模型应复用 `EXPOSED_MODELS`，不要另建一份模型表；dsh 只是把 `CatalogModel` 映射成 dsh 的 `id/name/contextWindow/maxTokens/input`。

### 3.6 `src/doctor.rs` 的计数检查（必须改）

`static_mapping()` 目前对所有 harness 都断言：

```rust
if ids.len() != 5 {
    problems.push(format!("expected 5 mapped models, found {}", ids.len()));
}
```

Pi/Kimi 变成 6 项后，`tcamp doctor` 会把 `harness.pi.models` / `harness.kimi.models` 判为 fail，`tests/doctor_network.rs` 的全量 network 测试也会红。建议改成按来源动态取期望值，而不是再写死 6：

```rust
let expected = match harness {
    Harness::Claude => 5, // main + four slots; this branch is expressions, not unique ids
    Harness::Pi => pi::exposed_ids().len(),
    Harness::Kimi => kimi::exposed_aliases().len(),
};
if ids.len() != expected {
    problems.push(format!(
        "expected {expected} mapped models, found {}",
        ids.len()
    ));
}
```

- Claude 的 `5` 表示“主模型 + 4 个槽位”的表达式数，不是 5 个模型；上面的 `expressions.len() != 5` 检查可以保留。
- Pi/Kimi 用 `exposed_ids()` / `exposed_aliases()` 的长度，未来再加模型不需要改 doctor。`apply_test_mapping_break()` 的 `pop()` 仍会让长度不匹配，`tests/doctor.rs` 的 gap 测试继续有效。
- 不要只把 `5` 改成 `6`；那会把下一个模型新增再变成一次 doctor 改动。

## 4. `/v1/models` 与 preflight

### 4.1 公开列表

tokencamp-pro 的实现：

- `GET /v1/models` → `state.catalog.available_models()`（`crates/gateway/src/routes/mod.rs:466-477`）。
- `available_models()` 过滤过期条目，返回全部非过期模型（`crates/gateway/src/catalog.rs:105-112`），包括 `display_only`；v4.1-flash 有实际 deployment，不是 display-only，所以会出现在列表里。
- `GET /v1/models/{id}` 用 `find_available_model()` 做 canonical/alias 精确匹配（`routes/mod.rs:479-527`）。v4.1-flash 的 canonical id 会命中；因为 catalog 没有 alias，`deepseek-flash` / `deepseek-v4-flash` 会 404。
- `/v1/models` 的 `ModelObject` 只有 `id`、`object`、`created`、`owned_by`、`kind`、`expiration_date`，没有 context / max output / vision / thinking。所以 tcamp 不能从 `/v1/models` 推导 1M/384K/vision，必须用编译期 curated 表。这一点与现有 Pi/Kimi 设计一致。

**结论：tcamp 不需要改 `/v1/models` 相关代码。** 需要改的是测试里的“全量模型路径”列表（§5）和 doctor 的计数检查（§3.6）。

### 4.2 preflight

- Pi/Kimi 的 Launch 前 preflight 只检查**本次选中的模型**：`src/cli.rs` 里 `model_ids = vec![selection.model.clone()]`，然后 `preflight::check_models()` 对每个 id 发 `GET {base_url}/v1/models/{id}`（`src/harness/preflight.rs:21-88`）。没有硬编码模型名，新增模型不需要改逻辑。
- Claude 的 preflight 检查 `claude_model_ids()` 去重后的主模型 + 四个槽位；v4.1-flash 不在其中，不受影响。
- 只要用户选中 v4.1-flash，preflight 就会查 `/v1/models/deepseek/deepseek-v4.1-flash`；如果部署的 tokencamp-pro 还没有这个条目，会 404 → exit 2。这是期望行为。

### 4.3 部署侧注意

tokencamp-pro 的 README 明确：数据库是配置权威，YAML 只 seed（`README.md:190`）；`catalog_service::seed_from_config()` 只插入缺失行（`crates/gateway/src/catalog_service.rs:181`，`ON CONFLICT DO NOTHING`）。因此：

- 代码合并到 tokencamp-pro 不等于现有部署的 DB 已经有 v4.1-flash；需要正常的重启/seed 流程，或由 ops 在 Control 里添加。
- 不要为了一个新模型跑 `tokencamp catalog-reset --yes`（README:229 明确这是 destructive re-seed）；除非接受重置目录的后果。
- 验证顺序：`GET /v1/models` 能看到 canonical id → `GET /v1/models/deepseek/deepseek-v4.1-flash` 200 → 选中该模型跑一次真实 chat → 再跑两轮 tool call（§2.4）。

## 5. 测试与文档影响清单

### 5.1 测试

| 文件 / 行 | 当前内容 | 需要怎么改 |
| --- | --- | --- |
| `tests/models.rs:26-49` | 只测 Claude 静态槽位；没有 Pi/Kimi 的 5 模型测试 | 保留现有测试不动；新增一个 `tcamp::models` 测试，断言 `EXPOSED_MODELS` 有 6 项、顺序含 v4.1-flash、context 1048576 / max_output 32768、`catalog_model()` 命中；再断言 `CLAUDE_SELECTABLE_MODELS` 仍是 4 项且不含 v4.1-flash。这样 SPEC 的 MOD-01/MOD-06 才有真实证据 |
| `tests/config_pi.rs:78` | 测试名 `...five_models` | 改为 `...six_models` |
| `tests/config_pi.rs:112-121` | `ids` 期望 5 项，最后是 `tokencamp/auto` | 追加 `"deepseek/deepseek-v4.1-flash"`（放在 auto 之后，与 §3.1 一致） |
| `tests/config_pi.rs:123-129` | `(id, context, max)` 5 组 | 追加 `("deepseek/deepseek-v4.1-flash", 1_048_576, 32768)` |
| `tests/config_pi.rs:147-154` | 只断言 glm-5.3 是 text、glm-5.3-flash 是 text+image | 追加 v4.1-flash 的 `["text", "image"]` 断言 |
| `tests/config_pi.rs:279-283` | 断言 provider models 长度 6（5 managed + 1 user extra），文案 five | 长度改 7；文案改 six |
| `tests/config_pi.rs:448-457` | 预置一个用户模型后，manifest 记录 4 个 tcamp 新增模型 | 追加 v4.1-flash，变成 5 项；说明“pre-existing model 不算 tcamp-created” |
| `tests/config_pi.rs:491-500` | manifest 全量 `model_ids` 5 项 | 追加 v4.1-flash，变成 6 项 |
| `tests/config_pi.rs:750-785` | alias → canonical 映射 7 组 | 追加 `("deepseek-v4.1-flash", "deepseek/deepseek-v4.1-flash")`，并追加 canonical 自身的映射（`("deepseek/deepseek-v4.1-flash", "deepseek/deepseek-v4.1-flash")`，如果现有测试也覆盖 canonical 输入） |
| `tests/config_pi.rs:787-817` | 测试名 `...five_exposed_models`；hint 列表 5 项 | 改名 six；hint 列表追加 v4.1-flash |
| `tests/config_kimi.rs:94` | 测试名 `...five_aliases` | 改为 `...six_aliases` |
| `tests/config_kimi.rs:139-148` | alias 列表 5 项 | 追加 `"tokencamp/deepseek-v4.1-flash"`（放在 auto 之后） |
| `tests/config_kimi.rs:150-160` | `(name, model, context)` 5 组 | 追加 `("tokencamp/deepseek-v4.1-flash", "deepseek/deepseek-v4.1-flash", 1_048_576)` |
| `tests/config_kimi.rs:502-511` | 已有 `tokencamp/auto` 用户 alias 时，manifest 记录 4 个 tcamp 新增 alias | 追加新 alias，变成 5 项 |
| `tests/config_kimi.rs:534-543` | manifest 全量 5 个 alias | 追加新 alias，变成 6 项 |
| `tests/config_kimi.rs:920-952` | alias/canonical 输入映射 9 组 | 追加 `("tokencamp/deepseek-v4.1-flash", "tokencamp/deepseek-v4.1-flash")` 和 `("deepseek/deepseek-v4.1-flash", "tokencamp/deepseek-v4.1-flash")` |
| `tests/config_kimi.rs:954-990` | 测试名 `...five_aliases`；hint 列表 5 项 | 改名 six；hint 列表追加 `tokencamp/deepseek-v4.1-flash` |
| `tests/support/mod.rs:155-174` | `model_routes()` 只有 4 个模型（不含 auto） | 加 `/v1/models/deepseek/deepseek-v4.1-flash` 的 200 路由。这样 `all_model_server()`（config_pi/config_kimi 各自在 model_routes 上追加 auto）自动覆盖新模型；`tests/preflight.rs` 的 Claude 精确路径不受影响 |
| `tests/doctor_network.rs:187-193` | 全量 network 检查期望 5 个模型路径 | 追加 v4.1-flash；doctor 现在会检查 6 个 |
| `tests/doctor_network.rs:747-751` | Claude 去重后的 3 个模型路径 | **不动**。v4.1-flash 不进 Claude |
| `tests/doctor.rs:738-776` | `TCAMP_TEST_DOCTOR_MODEL_GAP=pi` 触发“缺一个模型”失败 | **不用改**；`static_mapping` 改成动态期望后，pop 一个仍然失败 |
| `tests/preflight.rs:48-87` | Claude 精确请求路径断言（3 个） | **不用改**；新增 `model_routes()` 不会让 Claude 请求新模型。可选：新增一个 Pi/Kimi 选中 v4.1-flash 的 preflight 测试，断言只查该 id，404 时 exit 2 |
| `tests/mock_pi.rs` / `tests/mock_kimi.rs` | 只用 `zhipu/glm-5.3` 做两轮 tool loop | **不用改**；它们不覆盖 thinking + v4.1-flash。若要覆盖 §2.4，需要在 tokencamp-pro 侧做集成测试，而不是在这两个 mock 里伪造 |
| `tests/docs.rs:197-243` | README Config 表断言恰好 5 行（header+separator+3 harness） | **catalog ticket 不用改**；dsh 发布时必须改成 4 个 harness（7 行），并加 dsh 的 target/launch token |
| `tests/config_claude.rs` | Claude overlay/slot 断言 | **不用改**；v4.1-flash 不进 Claude |

### 5.2 文档

| 文件 / 行 | 当前内容 | 处理 |
| --- | --- | --- |
| `SPEC.md:291-293` | Pi `models[]` 5 项；input 只给 glm-flash 加 image；`--model` 只接受 5 个 | 改为 6 项，列表加 v4.1-flash；image 条件加 v4.1-flash；`5 个` → `6 个` |
| `SPEC.md:309-311` | Kimi 5 个 alias；`--model` 只接受 5 个 | 改为 6 个，列表加 `tokencamp/deepseek-v4.1-flash`；`5 个` → `6 个` |
| `SPEC.md:330` | Pi/Kimi 暴露 5 个模型 | 改为 6 个并加 v4.1-flash；说明 auto 仍是显式选项、不是默认 |
| `SPEC.md:336-337` | context/max output 表按厂商分类（glm/deepseek/kimi/auto） | 不需要改；v4.1-flash 落在 “deepseek 1048576 / 具体模型 32768” 与 “Kimi 全部 8192” 的既有分类里。建议加一句“deepseek 含 v4-pro 与 v4.1-flash”避免歧义 |
| `SPEC.md:341` | Pi/Kimi 只接受 5 个暴露模型；hint 列 5 个 | 改为 6 个 |
| `SPEC.md:613` | CFG-PI-03：5 个模型列表 | 改 6 个并加 v4.1-flash |
| `SPEC.md:618` | CFG-PI-08：5 个暴露模型/只接受 5 个 | 改 6 个 |
| `SPEC.md:625` | CFG-KIMI-02：5 个 alias 列表 | 改 6 个并加 `tokencamp/deepseek-v4.1-flash` |
| `SPEC.md:630` | CFG-KIMI-07：5 个 alias/只接受 5 个 | 改 6 个 |
| `SPEC.md:663` | DR-09：Claude 5 个去重 id、Pi/Kimi 5 个 id | Claude 部分不动；Pi/Kimi 改 6 个 |
| `SPEC.md:672` | MOD-01：Pi/Kimi 5 个模型 | 改 6 个；注意该行 evidence 写 `tests/models.rs`，但当前 `tests/models.rs` 并没有 Pi/Kimi 列表测试。本 ticket 的新增测试应补上这个缺口，否则 SPEC 与测试继续不一致 |
| `SPEC.md:675` | MOD-04：Kimi alias 5 个 | 改 6 个 |
| `SPEC.md:677` | MOD-06：Pi/Kimi 5 个/只接受 5 个 | 改 6 个 |
| `SPEC.md:748` | SMOKE-03 的 Claude deferred 触发条件已提到 v4.1-flash | **不改**；它说的是 ops 添加 anthropic deployment 后的 Claude smoke，和本次 Pi/Kimi/dsh 暴露不是一件事 |
| `RELEASE-CHECKLIST.md:90,95,97,102,129,134,162,163` | v0.1.0 的历史证据，写的是 “five models/aliases” | **不要改历史行**。这些行记录的是 2026-11-08 对 5 模型版本的实测；改成 6 会伪造证据。为下一个 release 新增 6 模型的行/新 checklist |
| `CHANGELOG.md:24` | 0.1.0 条目 “five exposed Catalog models” | **不要改历史**；在 `[Unreleased]` 下新增条目（见 §6.3） |
| `README.md` / `README.zh-CN.md` | 没有模型清单，只有 harness 名和 Config 表 | 本 ticket 不用改；dsh 发布时加 `tcamp dsh` quickstart + Config 表一行，并同步 `tests/docs.rs` |
| `GLOSSARY.md` | 无模型清单 | 不改（ticket 也明确禁止） |

### 5.3 建议新增的测试

1. `tests/models.rs::the_catalog_exposes_six_models_with_the_v4_1_flash_contract`
   - `EXPOSED_MODELS.len() == 6`，id 顺序符合 §3.1；
   - `catalog_model("deepseek/deepseek-v4.1-flash")` 是 Some，context 1_048_576，max_output 32_768；
   - `CLAUDE_SELECTABLE_MODELS == [DEFAULT_MAIN_MODEL, OPUS_MODEL, HAIKU_MODEL, KIMI_MODEL]`；
   - `CLAUDE_SLOTS` 仍是 4 项、顺序不变。
2. `tests/config_pi.rs::configure_writes_models_json_with_the_provider_and_six_models`（改名+扩展现有测试）
   - 新条目 `input == ["text","image"]`、`contextWindow == 1048576`、`maxTokens == 32768`、`compat` 四个字段与旧条目一致、无 `cost`。
3. `tests/config_pi.rs::an_invalid_model_exits_2_and_lists_the_six_exposed_models`（改名+扩展）
   - hint 必须包含 `deepseek/deepseek-v4.1-flash`。
4. `tests/config_pi.rs::v4_1_flash_alias_and_canonical_id_resolve`（可在现有 alias 测试里加 case）
   - `--model deepseek-v4.1-flash` 和 `--model deepseek/deepseek-v4.1-flash` 都启动为 canonical id。
5. `tests/config_kimi.rs::configure_writes_config_toml_with_the_provider_and_six_aliases`（改名+扩展）
   - 新 alias 的 `model`、`max_context_size = 1048576`、`max_output_size = 8192`、`display_name` 非空。
6. `tests/config_kimi.rs::an_invalid_model_exits_2_and_lists_the_six_aliases`（改名+扩展）
   - hint 包含 `tokencamp/deepseek-v4.1-flash`。
7. `tests/doctor_network.rs::network_checks_are_free_and_report_reachability_auth_and_models`（扩展现有）
   - 断言 `/v1/models/deepseek/deepseek-v4.1-flash` 被请求。
8. （可选）`tests/preflight.rs::pi_preflight_checks_the_selected_v4_1_flash`
   - 选中 `deepseek/deepseek-v4.1-flash`，断言只请求该 id；404 时 exit 2，不写配置。

不建议在 `tests/mock_pi.rs` / `tests/mock_kimi.rs` 里伪造一个“DeepSeek thinking + tools”的 400/200 分支；那会把网关责任混进客户端 mock。§2.4 的验证应放在 tokencamp-pro 的集成测试或真实网关上。

## 6. 发布顺序与 changelog

### 6.1 推荐顺序

**Step 1：catalog PR（单独发布，建议 `v0.2.0`）**

- 改动：`src/models.rs`、`src/harness/pi.rs`、`src/harness/kimi.rs`、`src/doctor.rs`、`tests/support/mod.rs`、`tests/config_pi.rs`、`tests/config_kimi.rs`、`tests/models.rs`、`tests/doctor_network.rs`、`SPEC.md`。
- 不改 `main` 以外的 dsh 代码；不改 tokencamp-pro。
- 发布后 Pi/Kimi 用户立即可以 `--model deepseek/deepseek-v4.1-flash` 或短 alias。
- 版本：新增模型是向后兼容 feature，按 SemVer 0.x 用 minor（`0.1.0` → `0.2.0`）。如果项目把模型新增视为 patch，按现有 release 约定执行，但不要混进 dsh 的 feature 版本。

**Step 2：dsh PR（建议 `v0.3.0`）**

- 依赖 Step 1 的 `EXPOSED_MODELS` 6 项和已发布的 catalog；dsh 默认模型 `deepseek/deepseek-v4.1-flash` 不应指向未发布的表。
- dsh 自己新增 `src/harness/dsh.rs`、`Harness::Dsh`、CLI 子命令、doctor/status/uninstall、README/`tests/docs.rs`/`RELEASE-CHECKLIST` 的 experimental 行。
- dsh 的 E2E 必须覆盖：Web UI 启动、6 模型可见、默认 v4.1-flash、`--model` 切换、流式、两轮 tool call。tool call 失败按 map #37 的 GO/NO-GO 处理（核心路径失败 = NO-GO）。

**Step 3：ops 验证（与 Step 1 并行/之后）**

- 确保目标环境 tokencamp-pro 的 DB catalog 有 v4.1-flash；YAML seed 只插缺失行，不更新已有部署。
- 验证 `/v1/models` 列表、`/v1/models/deepseek/deepseek-v4.1-flash`、真实 chat、两轮 tool call。
- 在 §2.4 未闭环前，发布说明不要承诺 thinking + tools。

**如果必须同一版本发布：** 可以，但保留两个独立 commit/PR（catalog 在前，dsh 在后）和两条 changelog；不要让 dsh 的 PR 顺手改 `EXPOSED_MODELS`，否则回滚 dsh 会带走 Pi/Kimi 的模型。版本仍建议 `v0.2.0` 同时包含两者，或 `v0.2.0` catalog + `v0.2.1` dsh，取决于 release manager 对“新 harness”是否算 minor。

### 6.2 回滚

- 删除 v4.1-flash 是向后兼容的（老用户如果没写死这个 id，不受影响）；但已经用 `--model deepseek/deepseek-v4.1-flash` 的用户在降级后会 exit 2。release note 应提示。
- `EXPOSED_MODELS` 的顺序变化会影响新建 Pi `models.json` 的顺序，不影响已有文件（merge 追加）；回滚时不需要迁移配置。

### 6.3 Changelog 文案

在 `CHANGELOG.md` 的 `## [Unreleased]` 下新增（不要改 0.1.0 的 `five exposed` 历史行）：

```markdown
### Added

- `deepseek/deepseek-v4.1-flash` (1M context, text+image) is exposed to Pi
  and Kimi Code; Kimi alias `tokencamp/deepseek-v4.1-flash`.
- `tcamp dsh` configures DeepSeek Harness through the `tokencamp`
  `openai-completions` provider; it exposes six models and defaults to
  `deepseek/deepseek-v4.1-flash`.
```

如果两个 PR 分开发布，第一条放 catalog release，第二条放 dsh release；不要把 dsh 的条目提前写进 catalog release。

## 7. 未决问题 / 实现前需要拍板

1. **max output 策略**：Pi/dsh 写 32768（与现有 curated 策略一致）还是 393216（模型原生）。本文建议 32768；若选 393216，需要单独验证网关接受度、成本和 Pi/dsh 的行为，并可能影响所有模型的输出策略。
2. **Kimi `capabilities`**：本 ticket 是否顺手给 v4.1-flash 写 `capabilities = ["image_in", "tool_use"]`。本文建议不在 catalog ticket 做，另开 Kimi vision follow-up；否则需要真实 image 请求验证。
3. **reasoning_content 往返**：tokencamp-pro 是否补 `Message.reasoning_content` / `Delta.reasoning_content`；如果不补，Pi/Kimi 是否强制关闭 thinking，dsh 是否默认 `reasoningEffort: off`。这是 tool loop GO/NO-GO 的关键。
4. **Pi/Kimi 是否暴露 thinking**：与第 3 条绑定。现有 tcamp 不写 `reasoning`，保持现状最安全；要暴露就同时改 Pi 模型字段和 dsh `reasoningEfforts`，并做真实两轮 tool call。
5. **dsh 的 max output 与默认 effort**：32768 vs 393216；默认 `off` vs `high`。map #37 只定了默认模型，没有定这两个字段。
6. **顺序**：本文建议把 v4.1-flash 追加在 `tokencamp/auto` 之后，与 map #37 的 6 模型列表一致；如果产品希望 vendor 分组（v4-pro 后面），只改顺序和测试向量，语义不变。
7. **alias**：是否需要在 tokencamp-pro catalog 加 `aliases: ["deepseek-flash"]`。本文建议不加；dsh 用 canonical id，Pi/Kimi 用 tcamp 自己的短 alias。若加，先改 tokencamp-pro，再改 tcamp 测试。
8. **Kimi/Pi 的 vision 暴露差异**：Pi 本次会写 image；Kimi 默认不写 `capabilities`，所以同一个模型在 Kimi 里可能不能收图。这是有意的“最小改动”还是产品缺口，需要确认。

## 8. 来源与证据等级

### 8.1 本地代码（tcamp，`main@4ab275c`）

- `src/models.rs:3-23`：常量、`CLAUDE_SELECTABLE_MODELS`、`CLAUDE_SLOTS`、context/max output 常量。
- `src/models.rs:32-62`：`EXPOSED_MODELS` 5 项、`catalog_model()`。
- `src/harness/pi.rs:30-44`：`MODEL_NAMES` / `MODEL_ALIASES` 各 5 项。
- `src/harness/pi.rs:270-291`：`model_fields()`，image 只对 `HAIKU_MODEL` 生效。
- `src/harness/pi.rs:115-124`：`exposed_ids()` / `managed_model_ids()` 从 `EXPOSED_MODELS` 派生。
- `src/harness/kimi.rs:18`：`MAX_OUTPUT_SIZE = 8192`。
- `src/harness/kimi.rs:27-53`：`ALIASES` 5 项。
- `src/harness/kimi.rs:163-177`：`merge_config()` 写 alias 字段。
- `src/doctor.rs:1617-1693`：`static_mapping()`，其中 `1655-1657` 硬编码“expected 5 mapped models”。
- `src/harness/preflight.rs:21-88`：按传入 id 查 `/v1/models/{id}`。
- `src/cli.rs:697-747`（Pi）、`904-941`（Kimi）：preflight 只传入选中的模型。
- `src/cli.rs:502-577`（Claude）：preflight 传 `claude_model_ids()`。
- `tests/support/mod.rs:155-174`：`model_routes()` 4 个模型。
- `tests/config_pi.rs:78-154,279-283,448-457,491-500,750-817`：5 模型/Pi 测试。
- `tests/config_kimi.rs:94-160,502-543,920-990`：5 alias/Kimi 测试。
- `tests/doctor_network.rs:187-193,747-751`：network 模型路径。
- `tests/models.rs:26-49`：Claude 静态槽位测试。
- `tests/preflight.rs:48-87`：Claude preflight 精确路径。
- `tests/docs.rs:197-243`：README Config 表 3 harness 断言。
- `SPEC.md:291-341,613-677`：Pi/Kimi 5 模型契约与验收矩阵。
- `RELEASE-CHECKLIST.md:90-163`：v0.1.0 五模型历史证据。
- `CHANGELOG.md:24`：0.1.0 五模型历史条目。

### 8.2 tokencamp-pro（`60718c1`）

- `config/default.yaml:362-385`（HEAD）：v4.1-flash 条目；`family: deepseek/deepseek-v4-flash`、`context_window: 1048576`、`capabilities`、`upstream_model: deepseek-flash`、`released_on: 2026-09-10`、MIT、price/cost。
- `config/default.yaml:76-99`（HEAD）：v4-pro 条目，用于对照 family/upstream/定位。
- `crates/core/src/chat.rs:13-40`：`ChatRequest`，`extra` 是请求级 `#[serde(flatten)]`。
- `crates/core/src/chat.rs:57-69`：`Message` 没有 `reasoning_content` / `flatten`。
- `crates/core/src/chat.rs:90-101,166-175`：`ChatCompletion` / `Delta` 也没有 `reasoning_content`。
- `crates/gateway/src/routes/mod.rs:198`：`Json(request): Json<ChatRequest>` 是请求入口。
- `crates/gateway/src/routes/mod.rs:466-527`：`/v1/models` 列表与 `/v1/models/{id}` 检索。
- `crates/gateway/src/catalog.rs:105-112`：`available_models()` 只过滤过期。
- `crates/gateway/src/upstream/openai.rs:156-168`：`adapt_request()` 直接 clone 后序列化。
- `crates/gateway/src/catalog_service.rs:181`：`seed_from_config()` 只插缺失行。
- `README.md:190,229`：DB 是权威、YAML 只 seed；`catalog-reset` 是 destructive。
- `crates/gateway/src/config.rs:319-383`：`ModelConfig` 没有 max output 字段（只有 `context_window`），所以 max output 必须由 tcamp curated 表提供。

### 8.3 官方/第一方文档

- [DeepSeek Models & Pricing](https://api-docs.deepseek.com/quick_start/pricing)：`deepseek-flash` = `DeepSeek-V4.1-Flash-1017`；1M context；**MAX OUTPUT 384K**；thinking/non-thinking（默认 thinking）；Vision ✓；Tool Calls ✓；Json Output ✓；Anthropic API ✓；价格。
- [DeepSeek Your First API Call](https://api-docs.deepseek.com/quick_start/models)：`deepseek-flash` 是推荐模型名；旧名 `deepseek-v4-flash`、`deepseek-v4-flash-vision-exp` 仍接受但已退役，由 V4.1-Flash 服务。
- [DeepSeek Thinking Mode](https://api-docs.deepseek.com/guides/thinking_mode)：thinking 默认开启、默认 effort high；`thinking: {type: enabled/disabled}`；`reasoning_effort: low/high/max`；带 tools 时必须回传 `reasoning_content`，否则 400；thinking 下 temperature/penalties 无效。
- [DeepSeek Harness — Configure models](https://deepseek-harness.github.io/deepseek-harness/en/guide/providers)：`llm-pi-ai` custom provider；`api: openai-completions`；`baseURL`；`apiKeyEnv`；模型 `id`/`input`/`contextWindow`/`maxTokens`；`input` 省略或为空 = 继承，hand-declared 默认 `[text]`；`reasoningEfforts`；`compat.thinkingFormat: deepseek`；`agent-default-model`。
- [DeepSeek Harness — Plugin Config Catalog](https://deepseek-harness.github.io/deepseek-harness/en/reference/config-catalog)：`PiAiModelProfile` 字段（`id`、`name`、`contextWindow`、`maxTokens`、`input`、`reasoningEfforts`、`compat`）；`maxTokens` 会同时成为 per-request 默认；`DEFAULT_MAX_TOKENS = 128_000`；`DEFAULT_INPUT = [text]`。
- [DeepSeek Harness source `packages/llm/llm-pi-ai/src/catalog.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/llm/llm-pi-ai/src/catalog.ts)：`PiAiModelProfile` 注释与 `DEFAULT_CONTEXT_WINDOW = 1_000_000` / `DEFAULT_MAX_TOKENS = 128_000`。
- [DeepSeek Harness source `packages/llm/llm-pi-ai/src/config.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/llm/llm-pi-ai/src/config.ts)：`modelFields` schema（`contextWindow`、`maxTokens`、`input`、`reasoningEfforts`、`compat`）。
- Pi 1.5.0 本地文档 `@earendil-works/pi-coding-agent/docs/models-configuration.md`：`input` 默认 `["text"]`；`reasoning` 默认 `false`；`contextWindow` 默认 `128000`；`maxTokens` 默认 `16384`；`compat` 字段表；`samplingParams` 对 OpenAI-compatible API 生效。
- Pi 1.5.0 类型 `@earendil-works/pi-ai/dist/types.d.ts:653-670`：`Model` 的 `reasoning` / `contextWindow` / `maxTokens` / `input`。
- [Kimi Code CLI docs](https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/config-files.html) 与本地 `kimi 2.1.1`：`config.toml` 的 `[providers.<name>]` / `[models.<alias>]`；alias 字段 `provider`、`model`、`max_context_size`、`max_output_size`、`capabilities`、`display_name`。
- 仓库内既有研究：`research/kimi-code-adapter.md`（branch `research/kimi-code-adapter`）§1.3/§8.1 记录 `capabilities` 与 `max_output_size` 行为；`research/pi-adapter.md`（branch `research/pi-adapter`）§5.1/§8 记录 Pi 模型字段与 `/v1/models` 元数据缺口。

### 8.4 未验证 / 限制

- **真实 `/v1/models` 与真实 chat 未调用**：本机 `https://api.tokencamp.com` 连接超时；ticket 也明确要求用本地 catalog/源码。所有“会出现在 `/v1/models`”的结论来自 tokencamp-pro 的 `available_models()` 源码，不是线上响应。
- **DeepSeek 原生 max output 393,216 未在 tcamp 实测**：来自 DeepSeek 文档的 384K；没有在 tokencamp 网关或 harness 上验证过 393216 的请求。tcamp 实际写 32768/8192，不依赖这个数。
- **thinking + tools 的 400 未在真实网关复现**：§2.4 是 DeepSeek 文档 + tokencamp-pro `Message` 类型/序列化路径的直接推断。实现前必须用真实请求验证；不要把它当成已复现的 bug。
- **dsh 版本/路径**：map #37 记录 dsh 当前 `0.2.2-rc.1`（2026-11-04），developer preview；本 ticket 不实现 dsh，dsh 的 profile 路径/版本地板以 map #37 和后续 dsh ticket 为准。
