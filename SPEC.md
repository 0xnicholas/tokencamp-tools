# tcamp v1 SPEC

本文件是 `tcamp` v1 的实现契约，由实现 effort 的第一步汇编，后续 ticket 以它为准。

- **Part I** 原样汇编自 [#17](https://github.com/0xnicholas/tokencamp-tools/issues/17)（v1 规格：产品、实现决策、测试决策、范围）。
- **Part II** 汇编自 [#13](https://github.com/0xnicholas/tokencamp-tools/issues/13) 的 resolution comment（验收清单、测试矩阵、smoke 流程、非功能门槛、traceability）。其中 map 流程相关的第 1 节（规格完成条件）与第 10 节（对 map 的影响）不属于实现契约，未收录。
- **测试矩阵**是 canonical 验收契约，按 #13 原样复制，共 121 行；实现时不得改写行内容。例外：AUTH-15、NFR-04、PKG-08 与 DOC-01..07 按 [#35](https://github.com/0xnicholas/tokencamp-tools/issues/35) 验收条目修订（见 Part II §4）。
- **术语**以仓库根 `GLOSSARY.md` 为准；平台词汇按 tokencamp-pro 的 `CONTEXT.md` 复用。
- **语言约定**：代码、标识符、commit message 用 English；内部设计文档中文；CLI 人类输出 English；README 英文 + `README.zh-CN.md`。
- **文档权威**：用户面向事实以 docs 站为唯一权威；`README.md` 与 `README.zh-CN.md` 只做入口、7 项白名单重复与 Security 摘要（见 Implementation Decisions §8）。

# Part I — v1 产品规格（#17）

## Problem Statement

今天，一个已经持有 tokencamp API key 的用户，想把 Claude Code、Pi 或 Kimi Code 接到 tokencamp，必须自己完成一整套容易出错的接线：

- 不同 Harness 对 Base URL 的语义不同：Claude Code 要 origin，Pi / Kimi Code 的 OpenAI-compatible 路径要带 `/v1`。
- 不同 Harness 的鉴权方式不同：tokencamp 只认 `Authorization: Bearer`；Claude Code 的 `ANTHROPIC_API_KEY` 会发 `x-api-key`；Pi 的 `auth.json`、Kimi Code 的 `api_key` 都可能遮蔽启动时注入的 API key。
- 每个 Harness 都有默认模型名（`claude-*` 等），不显式钉 Catalog model 就会 404。
- Claude Code 默认开启 tool search、experimental betas 和 thinking，而 tokencamp 的 `/v1/messages` 当前不建模这些 content block。
- Config target 格式各异：Claude Code 是严格 JSON，Pi 是带 `//` 与 trailing comma 的 JSONC，Kimi Code 是 TOML；手改容易破坏注释、键序或用户已有配置。
- 没有统一的“我现在连的是哪个 Base URL、API key 是否有效、哪个 Harness 已配置、怎么安全撤销”的视图。
- 把 API key 写进 Harness 配置会造成长期泄露风险；用户需要的是 Launch 时注入，而不是落盘。

结果是：用户要么放弃，要么复制一份过时或错误的配置，遇到 401 / 404 后无法判断是 API key、Base URL、Catalog model 还是 Harness 的问题。

## Solution

`tcamp`：一个 Rust 单二进制 CLI，按 Harness 分子命令。用户用已有 API key 登录一次，`tcamp claude` / `tcamp pi` / `tcamp kimi` 会按当前 Active connection 和静态 Model mapping 收敛该 Harness 的用户级 Config target，然后 Launch Harness。`--no-launch` 只 Configure；`--dry-run` 只看 diff；`tcamp uninstall` 安全撤销 tcamp 写入的 Managed entry。

用户可见结果：

- API key 只存在 `0600` 凭据文件中，或由 `TOKENCAMP_API_KEY` 提供；Harness 配置永远不落 API key，Launch 时注入。
- 只改 tcamp 拥有的 Managed entry：Claude Code 用独立 Overlay + `--settings`，Pi / Kimi Code 只 upsert `tokencamp` Provider entry 与 tcamp 创建的模型条目；不碰用户默认值、shell profile、wrapper、项目级配置。
- 默认免费 Auth probe 验证 API key；`doctor` 离线体检，`--network` 只做免费检查，`--verify` 才发一次最小真实请求。
- 静态 curated Model mapping，`--model` > `TOKENCAMP_MODEL` > 默认；不静默回退。
- CLI 人类输出 English，文档中文；`--json` 给脚本稳定形状，退出码稳定，API key 全程掩码。
- macOS 双架构 + Linux musl 双架构，curl 安装脚本 + GitHub Releases，校验和验证，v1 不签名不公证。

## User Stories

1. **安装**：作为 macOS / Linux 用户，我希望用一条 curl 命令安装到 `$HOME/.local/bin`，以便不需要 sudo 或包管理器。
2. **版本锁定**：作为 CI 用户，我希望用 `--version` / `TCAMP_VERSION` 锁定安装版本，以便重复构建。
3. **校验和**：作为安全敏感用户，我希望安装器验证 SHA-256，以便发现损坏或篡改。
4. **版本与帮助**：作为新用户，我希望 `tcamp --version` 和 `tcamp help` 给出可执行信息，以便确认安装成功。
5. **裸跑**：作为新用户，我希望直接运行 `tcamp` 看到帮助和检测到的 Harness，以便知道下一步。
6. **多平台**：作为 macOS / Linux 用户，我希望 Intel / Apple Silicon 和 x86 / arm Linux 都有产物，以便在现有机器上使用。
7. **首次登录**：作为持有 API key 的用户，我希望在 TTY 中隐藏输入 API key，以便不把它留在 shell 历史。
8. **stdin 登录**：作为 CI 用户，我希望 `tcamp login --key-stdin` 从 stdin 读 API key，以便非交互登录。
9. **环境变量**：作为自动化用户，我希望 `TOKENCAMP_API_KEY` 优先于凭据文件，以便临时覆盖。
10. **默认校验**：作为用户，我希望 Login 默认用免费 Auth probe 验证 API key，以便不产生费用。
11. **离线保存**：作为离线用户，我希望 `--no-verify` 保存 API key，以便稍后验证。
12. **深度校验**：作为用户，我希望 `--verify` 发一次最小真实 chat 请求，以便确认余额 / 模型 / 限流。
13. **401**：作为用户，我希望 401 时不保存 API key 并给出 Portal 链接，以便修复。
14. **402 / 403 / 429**：作为用户，我希望这些情况保存 API key 并警告，以便知道 key 有效但账户、权限或限流有问题。
15. **网络失败**：作为用户，我希望网络、5xx 或无法判定时不保存并提示 `--no-verify`，以便避免存坏 key。
16. **覆盖**：作为用户，我希望覆盖已有 API key 时在 TTY 确认，非 TTY 需要 `--force`，以便避免误覆盖。
17. **一 Base URL 一 API key**：作为自托管用户，我希望每个 Base URL 独立保存 API key，以便连接多个 tokencamp。
18. **规范化**：作为用户，我希望 `.../v1/` 和 `.../prefix/v1` 被规范化，以便不会拼出 `/v1/v1`。
19. **优先级**：作为用户，我希望 `--base-url` > `TOKENCAMP_BASE_URL` > Active connection > 默认，以便明确当前连接。
20. **HTTP 策略**：作为安全敏感用户，我希望公网明文 HTTP 被拒绝、内网 / loopback HTTP 允许并警告，以便不泄露 API key。
21. **Active connection**：作为用户，我希望 Login 成功把该 Base URL 设为 Active，Logout Active 时清空而不自动切换，以便连接状态可预测。
22. **Logout**：作为用户，我希望 `logout` / `--base-url` / `--all` 删除本机凭据，以便撤销本机访问。
23. **运行中提示**：作为用户，我希望 Logout 提醒运行中的 Harness 仍持旧 API key、Portal 吊销才立即生效，以便知道止损方式。
24. **Status**：作为用户，我希望 `status` 只做本地快照，显示 Active connection、掩码 API key、来源、验证时间、存储路径和 Harness 状态，以便快速排查。
25. **掩码**：作为用户，我希望所有输出只显示 `sk-…` 加后四位，以便不泄露 API key。
26. **权限**：作为安全敏感用户，我希望凭据目录 `0700`、文件 `0600`、原子写、拒绝 symlink，以便本机其他用户无法读取。
27. **损坏文件**：作为用户，我希望损坏或版本不支持的凭据文件报错且不覆盖，以便手工修复。
28. **Claude Overlay**：作为 Claude Code 用户，我希望 tcamp 写独立 Overlay 并用 `--settings` Launch，以便不改我的 `~/.claude/settings.json`。
29. **Claude 日常**：作为 Claude Code 用户，我希望 `tcamp claude` 配置后 Launch、`--no-launch` 只 Configure，以便融入日常。
30. **Claude key 边界**：作为 Claude Code 用户，我希望 API key 只在 Launch 时注入，以便不落盘。
31. **Claude 遮蔽**：作为 Claude Code 用户，我希望用户 / 项目 / managed settings 有 token 或 `apiKeyHelper` 时拒绝 Launch，以便不会用错凭据。
32. **Claude managed 不可读**：作为 Claude Code 用户，我希望 managed settings 不可读时警告并继续、Doctor 标红，以便不阻塞但可诊断。
33. **Claude 槽位**：作为 Claude Code 用户，我希望主模型和 opus / sonnet / haiku / fable Slot 都钉到 Catalog model，以便不会请求 `claude-*` 而 404。
34. **Claude 兼容开关**：作为 Claude Code 用户，我希望 tcamp 关闭 tool search、experimental betas 和 thinking，以便避开 tokencamp 未建模的 content block。
35. **Claude 上下文**：作为 Claude Code 用户，我希望 context 与 max output 使用保守静态值，以便不溢出。
36. **Claude 模型覆盖**：作为 Claude Code 用户，我希望 `--model` 只覆盖本次主模型，以便不改变配置。
37. **Claude 非法模型**：作为 Claude Code 用户，我希望 `auto`、`@preset/...`、`~family-latest` 被拒绝并给出可用 id，以便避免 404。
38. **Claude 未验证提示**：作为 Claude Code 用户，我希望 Launch 前看到 `/v1/messages` 能力未验证的提示，以便知道当前是 config-only。
39. **Claude Verify**：作为 Claude Code 用户，我希望 `doctor --verify --harness claude` 发一次 `/v1/messages`，以便在 ops 补 Anthropic deployment 后验证。
40. **Claude 版本**：作为 Claude Code 用户，我希望低于 `2.1.304` 只警告，以便不阻塞但知道风险。
41. **Pi models.json**：作为 Pi 用户，我希望 tcamp 写 `models.json` 的 `providers.tokencamp`，保留注释、trailing comma 和键序，以便不破坏我的文件。
42. **Pi Provider entry**：作为 Pi 用户，我希望 Provider entry 使用 `openai-completions`、Bearer 和 `$TOKENCAMP_API_KEY`，以便直接走 `/v1/chat/completions`。
43. **Pi 模型**：作为 Pi 用户，我希望看到 5 个精选 Catalog model，以便选择主、强、便宜、Kimi coding 和 auto。
44. **Pi 默认值**：作为 Pi 用户，我希望默认 Provider entry / 模型只在缺失或为空时补，以便不覆盖我的选择。
45. **Pi 遮蔽**：作为 Pi 用户，我希望 `auth.json` 有 `tokencamp` 或损坏时拒绝 Launch，以便不会用旧 key。
46. **Pi Launch**：作为 Pi 用户，我希望 `tcamp pi` 用 `--provider tokencamp --model <id>` Launch，以便本次选择明确。
47. **Pi tool loop**：作为 Pi 用户，我希望 chat tool loop 两轮能工作，以便正常使用工具。
48. **Pi Responses 边界**：作为 Pi 用户，我希望 tcamp 不使用 `openai-responses`，以便避免 `function_call_output` 400。
49. **Pi 未安装**：作为 Pi 用户，我希望 `--no-launch` 即使 `pi` 不在 PATH 也能写配置，以便预配置。
50. **Kimi config.toml**：作为 Kimi Code 用户，我希望 tcamp 用 `toml_edit` 只 upsert 自己的表，保留注释、空行和表顺序，以便不破坏配置。
51. **Kimi Provider entry**：作为 Kimi Code 用户，我希望 `type = "openai"`、`base_url = "<root>/v1"`、`api_key_env = "TOKENCAMP_API_KEY"`，以便 Bearer 加 chat completions。
52. **Kimi alias**：作为 Kimi Code 用户，我希望 5 个 `tokencamp/*` alias，以便用 `-m` 选择。
53. **Kimi max output**：作为 Kimi Code 用户，我希望每个模型 `max_output_size = 8192`，以便不触发 context 闸门。
54. **Kimi 字面量 key**：作为 Kimi Code 用户，我希望 tcamp 移除自己写过的字面量 `api_key` 并改用环境变量；unowned 字面量 key 视为 Conflict，以便不泄露。
55. **Kimi 默认值**：作为 Kimi Code 用户，我希望 `default_model` 只在缺失或为空时写，以便不覆盖我的默认。
56. **Kimi Launch**：作为 Kimi Code 用户，我希望 `tcamp kimi` 用 `-m <alias>` Launch，以便本次选择明确。
57. **Kimi tool loop**：作为 Kimi Code 用户，我希望 chat tool loop 工作，以便正常使用工具。
58. **原子写**：作为用户，我希望所有配置写入是原子、带备份、带锁、保留权限，以便失败不留下半个文件。
59. **symlink**：作为用户，我希望 symlink 被解析到目标并保留链接、broken / non-regular 被拒绝，以便不破坏我的布局。
60. **Conflict**：作为用户，我希望已有非 tcamp 条目时默认拒绝并给修法，`--force` 先备份再 Adopt，以便不静默覆盖。
61. **Drift**：作为用户，我希望手改受管字段时收到 Drift 警告但仍可收敛，以便知道配置被覆盖。
62. **Uninstall**：作为用户，我希望 Uninstall 只删 tcamp 创建且未被改过的条目，改过的保留并警告，`--force` 才删，以便安全撤销。
63. **Ownership manifest 丢失**：作为用户，我希望 Claude 可靠 `_tcamp` 标记清理，Pi / Kimi 不自动动并要求 `--force` 或手工，以便不误删。
64. **爆炸半径**：作为用户，我希望 tcamp 不写 shell profile、不生成 wrapper、不做项目级配置，以便影响面可控。
65. **dry-run**：作为用户，我希望 `--dry-run` 只读、打印打码 diff、不建目录 / 不拿锁 / 不写状态，以便预览。
66. **首次引导**：作为首次使用的 TTY 用户，我希望隐藏输入 API key、验证、确认写入路径后 Launch，以便一次完成。
67. **非 TTY**：作为 CI 用户，我希望缺 API key 时 exit 2 并提示 `login --key-stdin` 或环境变量，以便脚本化。
68. **日常安静**：作为日常用户，我希望配置无变化时只看到 `Launching ...`，有变化时看到 `Configured ...`，以便不刷屏。
69. **JSON**：作为脚本用户，我希望 `--json` 恰好一个对象，成功、失败、Doctor、dry-run 形状稳定，以便解析。
70. **退出码**：作为脚本用户，我希望 `0/1/2` 语义稳定，exec 后归 Harness，以便编排。
71. **透传**：作为用户，我希望 `--` 后参数原样传给 Harness、`--` 前未知 flag 报错，以便使用 Harness 原生参数。
72. **402**：作为用户，我希望 402 只警告不阻断 Configure / Launch，并给账单或预算下一步，以便不被误挡。
73. **英文 CLI**：作为用户，我希望 CLI / help 英文、文档中文，以便与 tokencamp 生态一致。
74. **Doctor 离线**：作为用户，我希望默认 Doctor 只读离线，给出 `pass/warn/fail/skip` 和下一步，以便不花钱。
75. **Doctor 网络**：作为用户，我希望 `--network` 只做免费 reachability 和 Auth probe，以便确认连通性。
76. **Doctor Verify**：作为用户，我希望 `--verify` 最多发一个最小真实请求，以便确认现在能发请求。
77. **Doctor JSON**：作为脚本用户，我希望 Doctor `--json` 有 `checks`、`warnings`、`summary` 和稳定退出码，以便 CI。
78. **Doctor 过滤**：作为用户，我希望 `--harness` 只过滤 Harness 块，Connection / Network 始终显示，以便聚焦。
79. **preflight**：作为用户，我希望 Launch 前只跑必要的本地和模型存在性检查、不跑 Auth probe 或付费检查，以便快且省钱。
80. **模型默认**：作为用户，我希望静态 curated 默认和 `--model` > `TOKENCAMP_MODEL` > 默认，以便可预测。
81. **模型校验**：作为用户，我希望 Launch 前用 `/v1/models/{id}` 检查存在性，404 时 exit 2、网络失败警告继续，以便不静默回退。
82. **无静默回退**：作为用户，我希望模型下架或改名时不自动换模型，以便我知道发生了什么。
83. **打包**：作为发布经理，我希望四个 target、固定资产名、校验和和 provenance，以便可重复发布。
84. **发布**：作为发布经理，我希望 tag `v*` 触发 verify / build / publish、CHANGELOG 缺失时失败、prerelease 正确，以便发布可控。
85. **不签名**：作为 macOS 用户，我希望 docs 说明 Gatekeeper 右键打开或 `xattr`、install.sh 不自动绕过，README 只保留 `not signed or notarized`，以便知情。
86. **卸载二进制**：作为用户，我希望 `tcamp uninstall` 清配置但保留凭据 / 备份 / 二进制，并提示 `rm "$(command -v tcamp)"`，以便分离配置与二进制。
87. **无遥测**：作为隐私敏感用户，我希望除显式网络操作外 0 出站请求，以便不泄露使用数据。
88. **Codex 负向**：作为用户，我希望 `tcamp codex` exit 2、不写任何 Codex 配置、指向 README，以便知道 Codex 不在 v1。
89. **旧命令负向**：作为用户，我希望 `opencode`、`env`、`--project`、`--fix`、`--lang` 不存在，以便范围清晰。
90. **不改 tokencamp-pro**：作为维护者，我希望 v1 纯客户端、不改 tokencamp-pro，以便范围可控。

## Implementation Decisions

### 1. 产品形态、命令树与输出契约

- `tcamp` 是 `tokencamp-tools` 的第一件 Tool：单二进制 Rust CLI，单 package（库 + 二进制入口），`edition = "2024"`、`rust-version = "1.85"`、`license = "Apache-2.0"`、初始版本 `0.1.0`；不与 tokencamp-pro 版本联动。
- 概念模块边界：`cli`（解析、输出、退出码）、`credentials`（Connection / Credential / 验证）、`config`（路径、锁、备份、Ownership manifest、原子写、diff）、`harness/{claude,pi,kimi}`（Config target、Desired state、格式合并、遮蔽检查、Launch 参数）、`doctor`（检查注册与报告）、`errors`（类型化错误到退出码 / JSON）。所有权、锁、备份、原子写、manifest 集中在 `config`，Adapter 不重复实现。
- 命令树：

| 命令 | 主要 flag |
| --- | --- |
| `tcamp claude\|pi\|kimi` | `--no-launch` `--dry-run` `--model <id>` `--base-url <url>` `--force` `--yes` `--json` `--no-color` |
| `tcamp login` | `--key-stdin` `--no-verify` `--verify` `--force` `--base-url` `--json` |
| `tcamp logout` | `--all` `--base-url` `--json` |
| `tcamp status` | `--json` |
| `tcamp doctor` | `--network` `--verify` `--harness <name>` `--json` |
| `tcamp uninstall` | `--harness <name>`（可重复） `--dry-run` `--yes` `--force` `--json` |
| `tcamp help [command]` | — |

- 全局：`-h/--help`、`-V/--version`、`-v/--verbose`。`tcamp` 裸跑打印 help 和检测到的 Harness，exit 0。
- `--json` 与 Launch 互斥：`tcamp claude --json` 是 usage error（exit 2），hint 指向 `--no-launch`。
- `--` 之后原样透传给 Harness；`--` 之前未知 flag 是 usage error（exit 2），hint 指向 `--`。
- 人类模式：进度、提示、确认走 stderr；结果（保存行、Status、dry-run diff）走 stdout。颜色只在 TTY，尊重 `NO_COLOR`、`--no-color`、`TERM=dumb`；无转圈或光标动画。
- `--json`：stdout 恰好一个 JSON 对象；警告进 `warnings[]`，stderr 只留致命 I/O。成功形状为 `{ok, base_url, key:{masked, source, verified, verified_at}, harnesses:{...}, warnings:[]}`；失败形状为 `{ok:false, error:{code, message, hint, request_id?}}`；`--dry-run --json` 返回 `changes:[{path, action:"create"|"update"|"none", diff}]`，diff 打码。本地固定 code：`missing_key`、`harness_not_found`、`config_conflict`、`config_unreadable`、`network`、`usage`。
- 退出码：`0` 成功；`1` 运行期失败（网络不可达、超时、401、5xx、写一半 I/O 失败）；`2` 用户可修的前置问题（用法、缺 API key、Harness 未安装且未带 `--no-launch`、Conflict、Config target 损坏、需要确认但无法交互）。Configure 成功后 `exec` Harness；信号、Ctrl-C、退出码都归 Harness。
- `tcamp codex` exit 2，不写任何 Codex 配置，错误信息说明 Codex 不在 v1 支持范围并指向 README。`opencode`、`env`、`--project`、`--fix`、`--lang` 不进入命令树。
- `status` 只做本地检查，不发网络请求；`doctor`、`uninstall` 使用 state board；日常 Configure / Launch 保持安静。
- CLI 人类输出与 help 用 English；文档中文；v1 不做 i18n 或 `--lang`。

### 2. Credential、Connection 与 Base URL

- v1 只用一个凭据文件：`$XDG_CONFIG_HOME/tokencamp/credentials.toml`，未设置时 `~/.config/tokencamp/credentials.toml`（macOS / Linux 同路径）。目录 `0700`，文件 `0600`，原子写（同目录临时文件 + `fsync` + `rename` + 父目录 `fsync`），拒绝跟随 symlink。
- 优先级：`TOKENCAMP_API_KEY`（当前进程）> 凭据文件。环境变量不落盘。
- Base URL 解析顺序：`--base-url` > `TOKENCAMP_BASE_URL` > Active connection > `https://api.tokencamp.com`。
- Base URL 规范化：trim 尾部 `/`；如果末尾是 `/v1` 则去掉；其他路径前缀保留。例：`https://host/v1/` → `https://host`；`https://host/prefix/v1` → `https://host/prefix`。Claude Code 的 `ANTHROPIC_BASE_URL` 用 API root；Pi / Kimi Code 的 `baseUrl` / `base_url` 用 `<root>/v1`。
- 一个 Base URL 对应一个 Credential；v1 不做命名 profile、account 或多 key。`tcamp login --base-url X` 验证成功后把 X 设为 Active connection；`logout` 当前 Active 时清空 Active，不自动切到别的 Connection。
- HTTP 策略：HTTPS 之外只允许 loopback、RFC1918、link-local 的 `http://`，打印警告；公网明文 HTTP 拒绝。
- `TOKENCAMP_API_KEY` 只覆盖当前调用的 API key，不改变 Active connection，不落盘。
- Login 输入优先级：`--key-stdin` 读取 stdin 并 trim；TTY 且无 `--key-stdin` 时隐藏输入；非 TTY 且无 `--key-stdin` 时，如果 `TOKENCAMP_API_KEY` 已设置则使用并打印来源，否则 usage error exit 2。TTY 下即使环境变量已设置也走隐藏输入，避免静默使用旧 key。不提供 `--key <字面量>`。
- Login 不创建、不吊销 API key；创建与吊销都在 Portal 操作。
- API key 形状不是 `sk-` + 32 字符时只警告，不硬拒；空值或控制字符硬拒。同一 Base URL 已有 key：TTY 确认覆盖；非 TTY 必须 `--force`，否则 exit 2；覆盖时更新 `created_at`、清空 `verified_at`。
- 默认验证：免费 Auth probe `GET <base>/v1/generation/<uuidv4>`，超时 10s。200 或 404 且 `error.code == "generation_not_found"` 视为有效；401 无效；其他 4xx（402/403/429 等，auth 已过）视为有效但警告；网络错误、超时、5xx、404 但 code 不是 `generation_not_found` 视为无法判定。
- `--no-verify`：跳过所有网络检查，保存，`verified_at` 留空，exit 0。
- `--verify`：额外发一次真实最小 chat：`POST <base>/v1/chat/completions`，`model: "auto"`、`max_tokens: 1`、一条 `user` 消息、`stream: false`、超时 30s。401 不保存；auth 已过但余额 / 模型 / 限流有问题的 4xx 仍保存并警告。
- 默认 probe 通过才写文件；不保存未验证的 key，除非显式 `--no-verify`。
- 失败矩阵：

| 结果 | 保存 | 退出码 | 输出 |
| --- | --- | --- | --- |
| 200 / 404 `generation_not_found` | 是 | 0 | 保存并验证，掩码 key 与 Base URL |
| 401 `authentication` | 否 | 1 | API key 无效或已过期；官方云给 `https://tokencamp.com/keys`；自托管只打印 Base URL，不猜 Portal 路径；TTY 允许重试一次 |
| 402 / 403 / 429（auth 已过） | 是 | 0 | 警告具体 code，提示余额、权限或限流 |
| 网络不可达 / 超时 / 5xx / 无法判定 | 否 | 1 | 无法连接；提示 `--no-verify` |
| 文件损坏 / schema 版本不支持 | 否 | 2 | 打印路径；不覆盖；提示修复或删除 |

- 错误输出带 `code` 与 `x-request-id`（有的话）；完整 API key 永不出现在任何输出。
- Logout：删除当前 Active Connection；`--base-url` 指定单个；`--all` 清空全部 Connection 与 Active。只动凭据文件，不碰 Harness 配置。输出提醒：运行中的 Harness 仍持有旧 key 直到重启；要立即失效，请在 Portal 吊销。
- Login 覆盖已有 key 时输出提醒：key 已替换；运行中的 Harness 需重启才会用新 key。
- Status：显示 Active Base URL、掩码 API key、来源（file / env）、`verified_at`、凭据路径，以及 claude / pi / kimi 的 installed / configured / target / conflict。支持 `--json`。
- 凭据文件 schema（`version = 1`）：

```toml
version = 1
active = "https://api.tokencamp.com"

[connections."https://api.tokencamp.com"]
api_key = "sk-..."
created_at = "2026-11-01T10:00:00Z"
verified_at = "2026-11-01T10:00:01Z"
```

- `version` 不认识或 TOML 损坏：报错、打印路径、不覆盖、exit 2。`verified_at` 只在验证通过时写入；`--no-verify` 时省略。不存 Portal 用户名、workspace 或其他派生字段。
- API key 输出分两层：`tcamp` 自己生成或输出的日志、错误、JSON、Status、diff 与 hint 永不包含完整 API key；变更前备份按原文件字节保存（`0600`），用户预置在 Config target 中的字面量 API key（README 称 `literal key`）可能存在于备份中，但 `tcamp` 从不打印其值。掩码格式为 `sk-…` + 后四位（key 太短则全掩）。

### 3. 配置所有权、写入引擎与 Uninstall

- v1 只写用户级、只增不改用户选择：
  - Claude Code：tcamp-owned Overlay `$XDG_CONFIG_HOME/tokencamp/harnesses/claude/settings.json`；Launch 用 `claude --settings <path>`；绝不改 `$CLAUDE_CONFIG_DIR/settings.json`（默认 `~/.claude/settings.json`）。
  - Pi：只 upsert `$PI_CODING_AGENT_DIR/models.json`（默认 `~/.pi/agent/models.json`）里的 `providers.tokencamp` 和 tcamp 创建的模型条目；不碰 `models-store.json`；`settings.json` 仅在 `defaultProvider` / `defaultModel` 缺失或为空时补默认值，不覆盖用户已有选择。
  - Kimi Code：只 upsert `$KIMI_CODE_HOME/config.toml`（默认 `~/.kimi-code/config.toml`）里的 `[providers.tokencamp]` 和 `[models."tokencamp/*"]`；`default_model` 仅在缺失或为空时写，不覆盖用户已有选择。
- 不写 shell profile、不生成 wrapper、不做项目级配置、不改任何用户默认值。`--project` 不在 v1。
- 每次 `tcamp <harness>` 先收敛 Desired state 再 Launch；收敛失败不 Launch。`--no-launch` 只 Configure。
- tcamp 状态目录：`$XDG_CONFIG_HOME/tokencamp/`（fallback `~/.config/tokencamp/`）；备份与锁：`$XDG_STATE_HOME/tokencamp/`（fallback `~/.local/state/tokencamp/`）。记录解析后的绝对路径；不把 `~` 展开后写进 Harness 配置值。
- 写入：同目录临时文件 → `fsync` → `rename` → `fsync` 父目录。缺失目录按 `0700` 创建；新文件 `0600`；已有文件保留 mode；备份 `0600`。
- symlink：解析最终目标，在目标目录原子写，保留链接；broken 或 non-regular 拒绝；tcamp 凭据与状态文件绝不跟随 symlink。
- 每个 Config target 一把 `flock`（状态目录，5s 超时）；拿到锁后重读并比对 hash；若读后被改，重试一次，仍冲突 exit 1。
- 修改已有 Config target 前写时间戳备份（状态目录、`0600`、每个 target 保留最近 3 份）；Ownership manifest 记录备份路径；不自动整文件回滚。
- 目标解析或结构校验失败：exit 2，打印路径与行列号，不写、不备份。`rename` 失败：原文件不动，临时文件清理，exit 1。配置写成功但 manifest 写失败：尝试从备份回滚；回滚失败 exit 2，打印两个路径。
- `--dry-run`：只读，不建目录、不拿锁、不写备份或状态；输出 unified diff，敏感值打码；解析失败 exit 2，否则 0。
- `--force`：只用于覆盖 tcamp 不拥有的东西和非 TTY 冲突确认；不跳过解析、校验、原子写、备份。`--yes`：只回答 TTY 交互确认，不能替代 `--force`；非 TTY 冲突无 `--force` exit 2。
- 密钥边界：Config target 只写非密接线（Base URL、Catalog model、Pi `apiKey: "$TOKENCAMP_API_KEY"`、Kimi `api_key_env`、Claude Overlay env 不含 token）。已有字面量 API key 视为 Conflict：先按原文件字节备份（`0600`，因此备份可能含该字面量 API key），要求 `--force`，绝不打印值；force 后替换为环境变量引用。
- Ownership manifest：`managed.toml`（`0600`，`version = 1`）与凭据同目录，按解析后的 Config target 路径记录：Harness、文件是否由 tcamp 创建、Provider entry 是否创建或被 Adopt、tcamp 创建的模型 id、受管对象的 last-written hash、备份路径、时间戳。不存敏感值。
- Claude Overlay 整文件所有，带 `_tcamp: {version, harness}` 标记；缺标记或 version 不匹配 → Conflict。manifest 只记录备份与版本。
- Pi / Kimi 使用键级所有权，不在共享配置里放标记键：
  - Pi 拥有 `providers.tokencamp` 的 `name` / `baseUrl` / `api` / `apiKey` 和 tcamp 创建的 `models[]` 条目；未知字段保留。
  - Kimi 拥有 `[providers.tokencamp]` 的 `type` / `base_url` / `api_key_env`（并移除 tcamp 写过的 `api_key`）和 `[models."tokencamp/*"]` 的已知字段；未知字段保留。
- owned：每次按 Active connection 与 Model mapping 重算、覆盖已知字段、保留未知；用户改过受管字段时警告 Drift，但仍覆盖。
- unowned 或 manifest 丢失：已存在的 `tokencamp` Provider entry 或 alias 是 Conflict；exit 2 给修法；`--force` 先备份再 Adopt，保留未知字段。
- Uninstall：只删 tcamp 创建且仍等于 last-written 的条目；用户改过的留着并警告；`--force` 才删。`--force` 接管的用户条目默认保留并警告，只有 `--force` Uninstall 才删。tcamp 创建的文件语义为空时删除；凭据、备份、二进制不动。
- manifest 丢失时：Claude Overlay 可靠 `_tcamp` 标记清理；Pi / Kimi 不自动动，要求 `--force` 或手工。
- Shadowing 检查：
  - Claude：检查用户 settings（跟随 `$CLAUDE_CONFIG_DIR`，fallback `~/.claude/settings.json`）、项目 shared / local（按 cwd / git root 的 `.claude/settings.json`、`.claude/settings.local.json`）、可读的 managed settings（macOS `/Library/Application Support/ClaudeCode/managed-settings.json`，Linux `/etc/claude-code/managed-settings.json`）。任一有非空 `env.ANTHROPIC_AUTH_TOKEN` 或 `apiKeyHelper` → 拒绝 Launch，打印来源路径与键名，不打印值；managed 不可读 → 警告并继续，Doctor 标红；`--force` 不能绕过。
  - Pi：只读 `$PI_CODING_AGENT_DIR/auth.json` 的顶层 provider 名，绝不读值；有 `tokencamp` → 拒绝并提示删条目或切换登录；文件损坏 → 拒绝。
  - Kimi：`[providers.tokencamp]` 有 owned 字面量 `api_key` → 备份并移除，改 `api_key_env`；unowned → Conflict / `--force`。值永不打印。
  - Shadowing 检查不因 `--force` 放行；所有读取只用于存在性或键名判断，不缓存、不落盘、不打印；`--dry-run` 同样。
- 合并规则：
  - Claude Overlay：严格 JSON（无注释、无 trailing comma），2 空格缩进，末尾换行，键顺序 `$schema` → `_tcamp` → `env`；每次从 Desired state 重新生成；相同不写；变化前备份；缺标记或 version 不匹配 → Conflict；不合并用户 settings。
  - Pi `models.json`：Pi 方言是 `//` 行注释 + trailing comma，无块注释。tcamp 按同一方言读写，保留注释、trailing comma、键序、空白、缩进；只 upsert `providers.tokencamp` 和 owned `models[]`；未知字段保留；新条目追加；缩进沿用文件，检测不到用 2 空格；新建文件写 `$schema` + `providers`，已有文件不强行补 `$schema`；无法保留注释 → abort exit 2，不“去注释重写”，`--force` 也不。
  - Kimi `config.toml`：`toml_edit`；只 upsert `[providers.tokencamp]` 和 `[models."tokencamp/*"]`；保留注释、空行、其他表顺序；新表追加末尾；已有表原地更新已知字段，未知字段保留；不写 `default_model` 的已有值；不加标记注释；写前做结构 / 引用校验，不调用 `kimi doctor config`；解析或校验失败 → abort。
  - 通用：只改 Managed entry；不格式化整文件、不排序、不删除用户内容；内容一致且 hash 匹配不写；保留换行风格，新文件 LF；`--dry-run` diff 打码。

### 4. Harness Adapter 契约

每个 Adapter 负责：Harness id、binary 名、版本地板、Config target 路径解析、从 Active connection + Model mapping + 本次覆盖生成 Desired state、格式合并规则、Shadowing 检查、Launch argv / env。Adapter 不实现锁、备份、manifest、原子写，这些由 `config` 统一完成。

#### Claude Code

- Config target：Overlay 文件；Launch `claude --settings <overlay>`；`CLAUDE_CONFIG_DIR` 只用于定位用户 settings 做 Shadowing 检查，不写它。
- Overlay `env` 写入：

| 键 | 值 |
| --- | --- |
| `ANTHROPIC_BASE_URL` | API root（不带 `/v1`） |
| `ANTHROPIC_API_KEY` | 空串，压掉 shell 或用户 settings 里的 `x-api-key` |
| `ANTHROPIC_AUTH_TOKEN` | 不写入文件；Launch 时由 tcamp 注入 |
| `ANTHROPIC_MODEL` | `zhipu/glm-5.3`，可被 `--model` / `TOKENCAMP_MODEL` 覆盖 |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | `deepseek/deepseek-v4-pro` |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | `zhipu/glm-5.3` |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | `zhipu/glm-5.3-flash` |
| `ANTHROPIC_DEFAULT_FABLE_MODEL` | `deepseek/deepseek-v4-pro` |
| `CLAUDE_CODE_MAX_CONTEXT_TOKENS` | `1048576`，或本次主模型 / Slot 中最小已知窗口；未知模型 `200000` |
| `CLAUDE_CODE_MAX_OUTPUT_TOKENS` | `32768` |
| `ENABLE_TOOL_SEARCH` | `false` |
| `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS` | `1` |
| `MAX_THINKING_TOKENS` | `0` |

- 不写 `ANTHROPIC_SMALL_FAST_MODEL`、`modelPicker`、`ANTHROPIC_CUSTOM_MODEL_OPTION`、`ANTHROPIC_DEFAULT_*_MODEL_SUPPORTED_CAPABILITIES`、`CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY`；不写 `~/.claude.json`。
- Launch 注入 `TOKENCAMP_API_KEY`，并映射为 `ANTHROPIC_AUTH_TOKEN`；`ANTHROPIC_API_KEY` 为空。Launch 前若检测到可读 Shadowing 来源则拒绝；managed settings 不可读只警告。
- 版本地板 `2.1.304`；低于地板只 `warn`，不阻止 Configure / Launch；`--version` 本身失败在 Doctor 记 `fail`。
- `/v1/messages` 当前只服务有 `provider_type: anthropic` deployment 的模型；默认环境没有该 deployment，因此 v1 是 config-only。`tcamp claude` Launch 前在 stderr 打一行“`/v1/messages` capability unverified; requires an anthropic deployment; run `tcamp doctor --harness claude`”，不阻断。Doctor 默认 `warn`；`doctor --verify --harness claude` 发一次 `/v1/messages`，404 `model_not_found` 给 deployment hint。
- `--model` 只覆盖本次主模型，opus / sonnet / haiku / fable Slot 不变；不写 Config target。Claude Code 只接受 canonical Catalog model id；`auto`、`@preset/...`、`~family-latest` 拒绝，exit 2 并列出可用 id。
- 已知限制写入 docs Security › Platform support and limitations：thinking 被关闭；prompt cache 在 message content block 上退化；tool search 关闭；Claude Code live Smoke deferred。

#### Pi

- Config target：`models.json`；新建文件含 `$schema` + `providers`。Provider entry：

| 字段 | 值 |
| --- | --- |
| `name` | `tokencamp` |
| `baseUrl` | `<root>/v1` |
| `api` | `openai-completions` |
| `apiKey` | `$TOKENCAMP_API_KEY` |
| `compat.maxTokensField` | `max_tokens` |
| `compat.supportsDeveloperRole` | `false` |
| `compat.supportsStore` | `false` |
| `compat.supportsReasoningEffort` | `false` |

- `models[]` 5 项：`zhipu/glm-5.3`、`deepseek/deepseek-v4-pro`、`zhipu/glm-5.3-flash`、`kimi/kimi-k2.7-code`、`tokencamp/auto`。每项含 `id`、`name`、`input`（默认 `["text"]`；`zhipu/glm-5.3-flash` 加 `"image"`）、`contextWindow`、`maxTokens`、必要的模型级 `compat`；不写 `cost`。
- `defaultProvider: "tokencamp"` / `defaultModel: "zhipu/glm-5.3"` 仅在缺失或为空时补；不覆盖已有选择。tcamp 写入的默认值记录进 manifest；Uninstall 只在仍等于 last-written 且用户未改时删除。
- Launch：`pi --provider tokencamp --model <id>`，并注入 `TOKENCAMP_API_KEY`。`--model` 只接受 5 个暴露模型或 alias；非法 exit 2 并列出 5 个。
- 不使用 `openai-responses`（tool loop 第二轮会因 `function_call_output` 400）；不默认使用 `anthropic-messages`；不写项目级 `models.json`；不写 `models-store.json`；不读写 `auth.json` 的值。
- Pi `auth.json` 顶层有 `tokencamp` 或文件损坏 → 拒绝 Launch；提示删条目或切换登录。不读值、不打印、不改文件。
- 版本地板 `1.4.0`；低于地板只 `warn`；未安装时 `--no-launch` 仍写配置，不带 `--no-launch` exit 2。
- tool loop 走 `/v1/chat/completions` 的 `assistant.tool_calls` + `role: "tool"` + `tool_call_id`；v1 以此作为可用性契约。

#### Kimi Code

- Config target：`config.toml`；Provider entry：

| 字段 | 值 |
| --- | --- |
| `type` | `openai` |
| `base_url` | `<root>/v1` |
| `api_key_env` | `TOKENCAMP_API_KEY` |

- 模型 alias 5 项：`tokencamp/glm-5.3`、`tokencamp/deepseek-v4-pro`、`tokencamp/glm-5.3-flash`、`tokencamp/kimi-k2.7-code`、`tokencamp/auto`。每项含 `provider = "tokencamp"`、`model`（canonical Catalog model id；auto 用 `tokencamp/auto`）、`max_context_size`、`max_output_size = 8192`、可选 `display_name`。
- `default_model` 仅在缺失或为空时写 `tokencamp/glm-5.3`；不覆盖已有值。tcamp 写入的默认值记录进 manifest；Uninstall 只在仍等于 last-written 且用户未改时删除。
- Launch：`kimi -m <alias>`，并注入 `TOKENCAMP_API_KEY`。`--model` 只接受 5 个 alias 或 canonical id；非法 exit 2 并列出 5 个。
- 不使用 `type = "anthropic"`（会发 `x-api-key`）；不使用 `openai_responses`；不写 `default_model` 的已有值；不加标记注释；不调用 `kimi provider remove`。
- 版本地板 `2.1.1`；低于地板只 `warn`；未安装时 `--no-launch` 仍写配置，不带 `--no-launch` exit 2。
- tool loop 走 `/v1/chat/completions`；`max_output_size = 8192` 避免 Kimi 默认 `min(131072, max_context_size)` 触发 tokencamp 的 context 闸门。

### 5. Model mapping 与覆盖

- v1 使用编译期静态 curated 表，随 `tcamp` 发版维护；不使用 `~family-latest` 运行时解析，不默认 `tokencamp/auto`。
- Claude Code Slot：

| Slot | 默认 Catalog model |
| --- | --- |
| `ANTHROPIC_MODEL` | `zhipu/glm-5.3` |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | `deepseek/deepseek-v4-pro` |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | `zhipu/glm-5.3` |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | `zhipu/glm-5.3-flash` |
| `ANTHROPIC_DEFAULT_FABLE_MODEL` | `deepseek/deepseek-v4-pro` |

- Pi 默认 `zhipu/glm-5.3`；Kimi Code 默认 `tokencamp/glm-5.3`。
- Pi / Kimi 暴露 5 个模型：`zhipu/glm-5.3`、`deepseek/deepseek-v4-pro`、`zhipu/glm-5.3-flash`、`kimi/kimi-k2.7-code`、`tokencamp/auto`；`tokencamp/auto` 是显式选项，不是默认。
- context / max output：

| Harness | context | max output |
| --- | --- | --- |
| Claude Code | `CLAUDE_CODE_MAX_CONTEXT_TOKENS = 1048576`，或主模型 / Slot 中最小已知窗口；未知 `200000` | `CLAUDE_CODE_MAX_OUTPUT_TOKENS = 32768` |
| Pi | glm / deepseek `1048576`；kimi `262144`；auto `200000` | 具体模型 `32768`；auto `8192` |
| Kimi Code | glm / deepseek `1048576`；kimi `262144`；auto `200000` | 全部 `8192` |

- 覆盖优先级：`--model` > `TOKENCAMP_MODEL` > 内置默认。只覆盖主 / 默认模型；Claude Code 的 opus / sonnet / haiku / fable Slot 保持不变。`--model` 只影响本次 Launch，不写 Config target。`TOKENCAMP_MODEL` 为空视同未设置。
- Claude Code 只接受 canonical Catalog model id；拒绝 `auto`、`@preset/...`、`~family-latest`；非法值 exit 2，hint 列出可用 id。
- Pi / Kimi 只接受 5 个暴露模型（canonical id 或 alias）；未暴露的 id exit 2，hint 列出 5 个。Kimi 的 canonical id 映射到对应 `tokencamp/<name>` alias；`tokencamp/auto` 直接使用。
- Drift 处理：不做运行时选择或自动回退；Configure / Launch 前尽力用公开 `GET /v1/models/{id}` 校验本次相关模型存在且未过期；网络不可达时警告并继续（不阻塞离线 Configure）；404 时 exit 2，hint 用 `--model` 或升级 `tcamp`。Doctor 重复检查。新模型不会自动进入默认值；`tcamp` 发版更新 curated 表。`/v1/models` 只用于存在性 / 过期检查，不用于选择模型；不依赖 `/feed/models`。

### 6. Doctor、Verify 与 preflight

- Doctor 是只读体检：检查 Connection、Harness 配置与可选网络连通性，输出 `pass / warn / fail / skip`；不写文件、不建目录、不拿锁、不交互、无 `--fix`。Status 仍是纯本地快照；`tcamp <harness>` 只跑 preflight 子集，不跑完整 Doctor。
- Doctor 退出码独立于全局 CLI：`0` 没有任何 `fail`（`warn` / `skip` 允许）；`1` 至少一个 `fail`；`2` 仅用法错误（未知 flag、非法或重复 `--harness`、`--json` 与其他选项冲突等）。`--json` 的 `ok` 与 `fail == 0` 同义。
- 检查项：
  - Connection：`credential.file`（路径、可读性、TOML、schema、权限、symlink）、`credential.key`（来源、非空、形状、env 覆盖）、`connection.base_url`（合法、规范化、HTTPS、内网 HTTP）。
  - Network：`network.reachable`（`GET <base>/v1/models`，仅 `--network`）、`network.auth`（免费 Auth probe，10s）、`network.verify`（仅 `--verify`，一次最小真实请求）。
  - Harness：`harness.<name>.binary`（存在、版本地板；未安装 `warn`，`--version` 失败 `fail`）、`harness.<name>.config`（解析、结构、可写性）、`harness.<name>.ownership`（manifest、所有权、Drift、备份）、`harness.<name>.shadowing`、`harness.<name>.models`（静态映射完整性；联网时 `/v1/models/{id}`）、`harness.<name>.backups`。
  - Claude 额外：`harness.claude.messages`（默认 `warn`；`--verify --harness claude` 时发一次 `/v1/messages`）。
- 状态语义：`pass` 健康；`warn` 可用但有降级、未验证或不影响本次运行的问题，不改变退出码；`fail` 会阻止 Configure / Launch 或让请求必然失败，退出码 1；`skip` 不适用（未选、未配置、未安装、未联网）。
- 无 API key 时，`credential.key` 记 `fail`，`network.reachable` / `network.auth` / `network.verify` 全部 `skip`。
- 各检查的状态映射：

| 检查 | pass | warn | fail |
| --- | --- | --- | --- |
| `credential.file` | 文件存在、可读、TOML 合法、`version = 1`、文件 `0600` | 文件不存在但 `TOKENCAMP_API_KEY` 可用；目录不是 `0700` | 不可读、损坏、版本不支持、文件不是 `0600`、symlink 或非普通文件 |
| `credential.key` | 从 env 或文件拿到非空 key | 形状不是 `sk-` + 32；env 覆盖了存量文件 key | 完全没有 key |
| `connection.base_url` | 合法、规范化；HTTPS；内网 / loopback HTTP | — | URL 非法；公网明文 HTTP |
| `network.reachable` | 2xx | — | 网络错误 / 超时、非 2xx |
| `network.auth` | 200，或 404 且 `generation_not_found` | auth 已过的其他 4xx | 401；404 但 code 不是 `generation_not_found`；5xx、网络错误、超时 |
| `network.verify` | 2xx | 403 / 429 | 401 / 402 / 400 / 404 / 5xx / 网络 / 超时 |
| `harness.<name>.binary` | 存在且版本 >= 地板 | 找不到；版本低于地板 | `--version` 执行失败或无法解析 |
| `harness.<name>.config` | 解析通过；目标或最近存在的父目录可写 | — | 解析 / 结构损坏；目标不可写；manifest 说已配置但目标缺失；broken symlink / 非普通文件 |
| `harness.<name>.ownership` | manifest 合法、所有权一致、无 Drift、备份存在 | Drift；备份缺失 / 不可读；Claude Overlay 有 `_tcamp` 但 manifest 缺失 | manifest schema / 版本不支持；Pi / Kimi 已有 `tokencamp` Provider entry 但 manifest 缺失；Claude Overlay 缺 `_tcamp`；unowned Conflict |
| `harness.<name>.shadowing` | 没有遮蔽来源 | managed settings 不可读；Kimi owned 字面量 `api_key`（下次 Configure 会移除） | Claude 可读 settings 有非空 `env.ANTHROPIC_AUTH_TOKEN` 或 `apiKeyHelper`；Pi `auth.json` 损坏或含 `tokencamp`；Kimi unowned 字面量 `api_key` |
| `harness.<name>.models` | 离线：映射完整且无禁用值；联网：所有映射 id 200 | — | 离线：映射缺槽 / 含 Claude 禁用值；联网：任一 id 404、网络错误、超时 |
| `harness.<name>.backups` | 都存在 | 任一缺失 / 不可读 | — |
| `harness.claude.messages` | `--verify --harness claude` 下 2xx | 默认未验证；需要 Anthropic deployment | `--verify --harness claude` 下 401 / 402 / 400 / 404 / 5xx / 网络 / 超时；404 `model_not_found` 给 deployment hint |
- 版本地板：Claude Code `2.1.304`、Pi `1.4.0`、Kimi Code `2.1.1`。低于地板只记 `warn`，不阻止 Configure / Launch；`--version` 执行失败或无法解析记 `fail`。版本检查只在 Doctor 做，preflight 不跑 `--version`。
- `doctor --network` 只做免费检查：`network.reachable` 与 `network.auth`；不隐式发真实 chat。
- `doctor --verify` 隐含 `--network`，整次最多发一个真实请求，不重试，30s 超时，`max_tokens: 1`、`stream: false`、一条 `user` 消息：
  - 默认（三个 Harness）或 `--harness pi|kimi`：`POST /v1/chat/completions`，`model: "zhipu/glm-5.3"`，检查 id `network.verify`。
  - `--harness claude`：`POST /v1/messages`，同样用 `zhipu/glm-5.3`，检查 id `harness.claude.messages`。
  - 三个 Harness 全选时只发 chat 请求；Claude 的 `.messages` 保持 `warn`，除非显式 `--harness claude`。
  - 2xx `pass`；403 / 429 `warn`；401 / 402 / 400 / 404 / 5xx / 网络 / 超时 `fail`。Claude 的 404 `model_not_found` hint 指向 `provider_type: anthropic` deployment 前置，并建议先用 `--harness pi` 验证 key。
- Doctor 人类输出是 state board，每个检查一行，`hint:` 另起一行缩进；`--json` 始终输出恰好一个对象：`{ok, base_url, key:{masked,source,verified,verified_at}, network, verify, harnesses:{...}, checks:[{id,scope,harness,status,message,hint,code,request_id}], warnings:[...], summary:{pass,warn,fail,skip}}`。`checks[].scope` 为 `connection|network|harness`；`harness` 为 `claude|pi|kimi|null`；`warnings` 是 `status == "warn"` 的 check id；`summary` 只统计实际输出的检查。完整 API key 永不出现在 JSON、日志、错误、diff 或 hint。
- `--harness <name>` 只过滤 Harness 块；Connection / Network 检查始终显示；只接受一个值，重复是用法错误 exit 2。
- preflight（`tcamp <harness>`）：
  1. 本地：`credential.key`、`connection.base_url`、binary 存在、Config target 解析 / ownership / Conflict、Shadowing。
  2. 模型：对本次 Launch 相关 id 做免费 `GET /v1/models/{id}`；Pi / Kimi 只查主 / 默认模型和 `--model` 覆盖；Claude 查主模型 + 四 Slot + 覆盖，去重；并行请求，总预算 2s；404 → exit 2；网络错误 / 超时 → `warn` 并继续。
  3. 收敛配置；失败不 Launch。
  4. `exec` Harness。
- preflight 不跑 Auth probe、不跑 `--verify`、不跑 `--version`；warn 打到 stderr，不摆 state board。失败按全局退出码：本地 / 模型 404 = `2`，网络 / 写入 = `1`。
- `--no-launch`：跳过 binary 与 Shadowing 的阻塞；Shadowing 仍检查并记 `warn`。Credential / Base URL / Config target 解析 / ownership / Conflict / 模型 404 仍然失败；网络错误只 `warn`。通过后写配置；即使 binary 不在 PATH 也写。
- `--dry-run`：不联网、不写、不拿锁、不建目录；只查 Credential / Base URL / Config target 解析 / Conflict，打印 unified diff；解析失败 exit 2，否则 0；不做 binary 检查。

### 7. CLI 首次运行、确认与错误体验

- v1 采用混合 UX：日常 A（Quiet Unix）+ 首次 B（Guided）+ `status` / `doctor` / `uninstall` 用 C（State board）。
- 首次引导触发条件：`tcamp <harness>`（不带 `--no-launch`）、stdin 与 stdout 是 TTY、有效 Base URL 没有 API key、没有 `--json`。引导内容：隐藏输入 API key → 免费 Auth probe 验证 → 若该 Harness 还没配置，打印 Config target 路径并问 `Write it? [Y/n]` → 写配置 → `exec` Launch。
- 非 TTY 同场景：`error: no API key ... [missing_key]`，exit 2；hint 指向 `tcamp login --key-stdin` 或 `TOKENCAMP_API_KEY`。
- `--no-launch` 不触发引导；缺 API key 就 error / hint，exit 2。
- 已有 API key：日常 A。配置无变化只打一行 `Launching <harness> ...`（stderr）；有写入打一行 `Configured <harness>: <path>`。
- 确认只发生在：首次引导里的首次写入、Login 覆盖已有 API key（TTY 确认 / 非 TTY `--force`）、Uninstall 计划。已配置的 Harness 不再问写入。
- `--yes` 只回答确认，不替代 `--force`；`--force` 不跳过解析、校验、原子写、备份。`--json` 不弹任何交互；需要确认时必须 `--yes` / `--force`，否则 exit 2。
- 错误文案：人类模式 2–3 行 `error: <原因> [<code>]`；`hint: <下一步>`。`request_id` 默认不打印：`--json` 放 `error.request_id`；`-v` 加一行 `request_id: ...`；有 request_id 时 hint 末尾提示 re-run with `-v`。不打印栈、原始响应体或完整 API key；`-v` 也不。
- 402 / 余额：不新增付费预检；只有 `--verify`、`doctor --network` 或 Harness 真实请求能看到 402。402 时 warning + 下一步：官方云打印 `https://tokencamp.com` 并提示检查 Portal 的 billing / usage；自托管提示检查 workspace budget。不阻断 Configure / Launch；`--verify --no-launch` 遇 402 写完配置后 exit 0 带 warning。
- Status / Uninstall：Status 只做本地检查；Uninstall 默认处理全部 Harness，先打印计划，TTY 确认，`--yes` 跳过；非 TTY 或 `--json` 无 `--yes` 时 exit 2；`--harness` 可只处理一个；`--dry-run` 只打印计划。保留凭据（打印路径与 `tcamp logout --all`）、不删二进制（打印 `rm "$(command -v tcamp)"`）、备份保留并打印路径；`--force` 才删用户改过的条目；部分失败汇总后 exit 1。
- `--model` 只影响本次 Launch（映射到 Harness 原生 model 参数或本次 env），不改写 Config target。

### 8. 打包、分发、发布与非功能门槛

- 单 package；release profile `lto = "thin"`、`strip = "symbols"`、`codegen-units = 1`；根 `.gitignore` 忽略 `/target`；提交 `Cargo.lock`。
- 依赖：`clap` 4（derive）、`serde` + `serde_json`、`toml_edit`、`jsonc-parser`、`reqwest`（`default-features = false`，`blocking` + `json` + `rustls-tls`）、`uuid`（v4）、`thiserror`、`dirs`、`fs4`、`anstream` + `anstyle`、`jiff`。dev：`tempfile`、`assert_cmd`、`predicates`。不引入 `tokio`、`anyhow`、`keyring`、`openssl`。
- Target：`aarch64-apple-darwin`、`x86_64-apple-darwin`（`MACOSX_DEPLOYMENT_TARGET=11.0`）、`x86_64-unknown-linux-musl`、`aarch64-unknown-linux-musl`。Linux 只发 musl 静态；macOS 双架构统一 deployment target 11.0。
- 版本：crate `0.1.0`；`tcamp --version` 输出 `tcamp 0.1.0`；tag `v0.1.0`；预发布 `v0.2.0-rc.1`；严格 semver；不把 git SHA 塞进 `--version`。
- 资产：`tcamp-<version>-<target>.tar.gz`（版本不带 `v`）；包内只有 `tcamp` 与 `LICENSE`，位于归档根，无外层目录；每个资产一个 `.sha256`；合并的 `SHA256SUMS`（`sha256sum` 标准格式，两个空格分隔）；只发 `.tar.gz`。
- `install.sh`：仓库根文件，每次发布原样上传为资产；规范命令 `curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | sh`；POSIX `sh`（`set -eu`，无 bashism）；`--version v0.1.0` 锁定版本。
- `install.sh` 环境变量：`TCAMP_INSTALL_DIR` 默认 `$HOME/.local/bin`（不存在则 `0755` 创建）；`TCAMP_VERSION` 默认空（latest），`--version` 优先；`TCAMP_RELEASES_URL` 默认 `https://github.com/0xnicholas/tokencamp-tools/releases`，给 CI 与镜像用，不写进 README。
- `install.sh` 平台检测：`uname -s`（`Darwin` / `Linux`）+ `uname -m`（`arm64` / `aarch64` → aarch64；`x86_64` / `amd64` → x86_64）；其他（Windows / MSYS、i686）exit 2 并列出支持范围。
- `install.sh` 流程：下载 tarball 与 `SHA256SUMS` 到 `mktemp -d`；用 `sha256sum` 或 `shasum -a 256` 校验（都缺则失败）；解包并确认 `tcamp` 存在；在目标目录写临时文件，`install -m 0755`，`mv` 原子替换；运行 `tcamp --version` 与请求版本比对。
- `install.sh` 失败语义：替换前任何失败（网络、缺工具、校验和不匹配、解包失败）不动已有安装，清理临时目录；校验和不匹配 exit 1，输出 `refusing to install`；不支持平台或用法错误 exit 2；已安装版本与新版相同仍重写；不做 sudo、不修改 shell profile、不自动去 quarantine、不写 Harness 配置；无 `--uninstall`；装完若目标目录不在 `PATH`，打印一行 `export PATH="<dir>:$PATH"`，不改文件。
- 发布工作流：`.github/workflows/release.yml`，仅 push tag `v*` 触发。`verify` job 跑 `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`（ubuntu）。`build` job 四 target 矩阵，`needs: verify`，`cargo build --release --locked --target <t>`，打包 + 校验和，上传 artifact。`publish` job `needs: build`，先确认 tag 是 `origin/main` 的祖先，再 `gh release create "$TAG" --verify-tag` 上传四个 tarball、四个 `.sha256`、`SHA256SUMS`、`install.sh`。
- Release notes 从 `CHANGELOG.md` 提取 `## [<version>]` 一节；缺失则发布失败，不自动生成。`CHANGELOG.md` 手工维护（Keep a Changelog）。tag 含 `-` → `--prerelease`。`permissions: contents: write`；provenance 需要时加 `id-token: write` + `attestations: write`；`concurrency` 保证同时只有一个发布；资产中不允许出现契约外的文件。
- 签名与公证：v1 不签名、不公证。curl 下载不带 quarantine，`install.sh` 安装不会触发 Gatekeeper；浏览器直接下 Release 的用户会遇到拦截。docs Start › Install 写明右键打开或 `xattr -d com.apple.quarantine`；README 只保留 `not signed or notarized`、`no updater, re-run the installer` 与 docs 链接；`install.sh` 不自动去 quarantine。
- 文档权威：用户面向事实以 docs 站（Start / CLI reference / Security and limitations）为唯一权威；`README.md` 与 `README.zh-CN.md` 只做入口、7 项白名单重复与 Security 摘要。白名单共 7 项：canonical install 命令、一条 `TCAMP_VERSION` pin 示例、OS 支持行、四行 quickstart、三行 Config target 表、`no updater, re-run the installer`、`not signed or notarized`。Security 摘要保留 `0600`/`0700`、原子写、Launch 注入、字面量 API key 备份 caveat、Portal 吊销与无 telemetry/更新检查。Troubleshooting、Known limitations、Uninstall 与 ownership 细节只在 docs。
- 仓库卫生：`README.md` 英文 + `README.zh-CN.md`，二者同节结构并遵守文档白名单；`SECURITY.md` 只指向 GitHub Private Vulnerability Reporting；v1 不加 `CONTRIBUTING.md`、issue 模板、PR 模板；不加 per-file license header；`Cargo.toml` 写 `license = "Apache-2.0"`。
- CI 与供应链：PR CI `fmt --check`、`clippy -D warnings`、`test`，矩阵 `ubuntu-latest` + `macos-14`；MSRV job；每周 `cargo-deny`（advisories + licenses）；Dependabot（cargo 与 actions，周更、分组）；Actions 用 major tag；每个 workflow 最小 `permissions`；发布构建 `--locked`；每个资产用 `actions/attest-build-provenance` 生成 provenance；开启 GitHub Release immutability；不发 crates.io / npm。
- `install-smoke`：ubuntu + macOS 本地构建 archive 与校验和，摆成 `releases/latest/download/` 结构，用 `TCAMP_RELEASES_URL` 指向本地，真实执行 `install.sh` 到临时 `TCAMP_INSTALL_DIR`，断言 `tcamp --version`；再加篡改校验和的负例，断言 exit 1 且目标目录无二进制；另跑 `shellcheck` 与 `sh -n`。
- 非功能门槛：release build 上 `tcamp --version` / `--help` p95 ≤ 100ms；离线 `status` / `doctor` p95 ≤ 500ms；未压缩二进制 ≤ 15 MiB/target；`tar.gz` ≤ 6 MiB/target。发布时硬门禁；PR 只记录延迟。除 Login、Doctor `--network` / `--verify`、preflight 模型检查外 0 出站请求；无更新检查、无分析、无遥测。

### 9. 安全与错误模型

- 类型化错误：每个错误映射到 `code`、人类 message、hint、退出码；服务器错误保留 `code` 与 `x-request-id`（有的话）。本地固定 code 见第 1 节。
- API key 边界：只存凭据文件或环境变量；只由 `tcamp <harness>` Launch 时注入；Harness 配置不含字面量 key；`tcamp` 自己生成的 stdout、stderr、JSON、diff、日志、错误与 hint 永不含完整 key；变更前备份按原文件字节保存（`0600`），可能含用户预置的字面量 API key，但 `tcamp` 不打印其值；掩码为 `sk-…` + 后四位。
- 同用户进程可见环境变量是 v1 接受的边界；不是防同用户恶意进程的机制。运行中的 Harness 持有启动时的 key；轮换后需重启；Portal 吊销对网关立即生效。
- 不写 `~/.claude.json`、不写 shell profile、不生成 wrapper、不做代理 / 网关 / 路由、不转发流量、不做项目级配置、不接 OS keyring。
- 版本风险：Claude Code / Pi / Kimi Code 迭代很快；Adapter 的版本地板与已知限制写进 Doctor、docs 和测试；模型目录可能漂移，静态 curated 表随 tcamp 发版更新。

## Testing Decisions

- 好测试只验证外部行为：用户可见的文件内容、文件权限、stdout / stderr 通道、退出码、`--json` 形状、发往 tokencamp 的 HTTP 请求、传给 Harness 的 argv / env、`exec` 后的退出码。不断言私有结构体、内部调用次数或实现细节；错误文案允许实现润色，但 `code`、hint 的存在、输出通道和退出码是契约。
- 主测试缝（唯一产品缝）：编译后的 `tcamp` 二进制。所有 CLI、Credential、Config、Harness Adapter、Doctor 行为都通过它测试；测试进程使用临时 `HOME`、`XDG_CONFIG_HOME`、`XDG_STATE_HOME`、`CLAUDE_CONFIG_DIR`、`PI_CODING_AGENT_DIR`、`KIMI_CODE_HOME`，fake `claude` / `pi` / `kimi` 可执行文件（记录 argv / env，返回指定版本或退出码），以及 loopback 上的本地 mock HTTP server。repo 目前没有代码，因此这是新建的最高缝；不在库内部另开测试缝。
- 分发缝：`install.sh` 是独立产物，在 shell 脚本边界测试：本地 archive + `SHA256SUMS` + `TCAMP_RELEASES_URL` + 临时 `TCAMP_INSTALL_DIR`，覆盖正例、篡改校验和、缺 sha 工具、不支持平台、已有安装不被破坏。该缝不复用到产品库。
- 矩阵与先例：#13 的 121 行测试矩阵是 canonical 验收契约；实现 effort 第一步汇编 `SPEC.md` 时原样复制该矩阵。repo 目前没有测试，矩阵本身是 prior art。T1 覆盖 AUTH、URL、CFG-COMMON、CFG-CLAUDE、CFG-PI、CFG-KIMI、CLI、DOCTOR、MOD、PKG、NFR、NEG；T2 覆盖 SMOKE、COMPAT、DOC；T3 可选 nightly，不设发布门槛。
- 关键测试组：
  - AUTH：Login 的有效 / 无效 / 过期 key、网络失败、402 / 403 / 429、覆盖确认、`--no-verify`、`--key-stdin`、env 优先级、Logout、Status、掩码、权限、损坏文件、并发写。
  - URL：规范化、优先级、Active 切换、Harness Base URL 形态、HTTP 策略。
  - CFG-COMMON：原子写、备份、锁、symlink、解析失败、合并范围、密钥边界、dry-run、`--force` / `--yes`、Drift、Uninstall、manifest 丢失、Shadowing、爆炸半径。
  - CFG-CLAUDE：Overlay 生成、`--settings` Launch、四 Slot、兼容开关、Shadowing、版本、`/v1/messages` 提示与 Verify。
  - CFG-PI：`models.json` JSONC 保留、Provider entry、5 模型、默认值、`auth.json` Shadowing、tool loop、版本。
  - CFG-KIMI：`config.toml` TOML 保留、Provider entry、5 alias、`max_output_size`、字面量 key、默认值、tool loop、版本。
  - CLI：命令树、首次引导、确认矩阵、JSON、退出码、`--` 透传、402、Status / Uninstall、英文输出。
  - DOCTOR：每个 check id 的 `pass/warn/fail/skip`、0/1/2 退出码、`--json` 形状、`--harness` 过滤、`--network` 免费边界、`--verify` 的单请求与错误映射、preflight 阻塞矩阵。
  - MOD：静态映射、Slot、context / max output、覆盖优先级、模型存在性校验、无静默回退。
  - PKG：四 target、资产命名、`install.sh` 正负例、release workflow、CHANGELOG / ancestor 失败、CI / 供应链。
  - NFR：启动延迟、二进制大小、无遥测、日志脱敏。
  - NEG：Codex / OpenCode / `env` / `--project` / `--fix` / `--lang` / Windows / 无 pro 改动。
- T1 使用 local mock HTTP server 断言 request line、headers、body、stream 形状；使用 fake Harness 断言 `--settings`、`--provider/--model`、`-m`、注入的 `TOKENCAMP_API_KEY` 与 `ANTHROPIC_AUTH_TOKEN`、`exec` 退出码。T2 使用真实 Harness + 真实 tokencamp API key，从干净临时 Home 开始，按 #13 的 SMOKE 流程执行 `tcamp login` → `tcamp <harness> --no-launch` → `tcamp <harness> -- -p '...'`，记录 Harness 版本、掩码 key、request id 和成本。
- 每个 T1 行必须在 `ubuntu-latest` + `macos-14` 全绿；T2 的 Pi / Kimi 真实 Smoke、真实 Harness 兼容和文档清单必须完成；没有 open 的 P0/P1；发布经理在 `RELEASE-CHECKLIST.md` 签字。Claude Code live Smoke 是 deferred：触发条件是 ops 在目标环境添加 `provider_type: anthropic` deployment（`deepseek/deepseek-v4.1-flash` 或 `deepseek/deepseek-v4-pro`，base URL `https://api.deepseek.com/anthropic`），owner 是发布经理 / ops；未补测不阻塞 v1。
- 验收分两层：规格完成 = 决策集完整、可执行、无未决设计问题、#13 矩阵原样可复制；实现发布 = T1 全绿、T2 完成、文档清单通过、无 P0/P1、签字。`SPEC.md` 由实现 effort 第一步汇编，不在本规格的交付物内。

## Out of Scope

- 修改 tokencamp-pro 的网关、计费、目录 API；v1 纯客户端。Codex 未来若支持，需要 tokencamp-pro 改 `/v1/responses`，作为独立 effort。
- 做代理、网关或路由：`tcamp` 只写配置，不转发流量；包括 Codex 的 Responses → Chat 本地 shim。
- 服务端 API key 管理（创建、吊销、额度）；用户在 Portal 操作。
- 浏览器 / 设备码登录、OAuth 拿 API key。
- 用量追踪、桌面 / 工具栏、全局 Provider entry 切换、GUI。
- 模型感知优化（tool search、系统提示词、按模型调参）。
- Windows、Homebrew、自动更新。
- 团队 / 组织级 profile 与共享配置；个人多账号 / 命名 profile；OS keyring / Keychain。
- 让直接运行 `claude` / `pi` / `kimi` 也带上 tokencamp 配置；v1 只支持 `tcamp <harness>` Launch。
- 运行中 Harness 的热轮换；v1 需重启，Portal 吊销是立即止损。
- OpenCode（用户暂缓）；未来恢复另开 effort。
- Codex v1 支持：`codex` 不进命令树；`tcamp codex` exit 2、不写配置、指向 README；Doctor 不检测 Codex。未来若做，需 tokencamp-pro 改协议子集，另案 ADR，作为独立 effort。
- Cursor、Cline、Aider 等其他 Harness。
- tokencamp-www 文档站页面改动。
- 工具包的后续 Tool 与品牌 / 商业策略。
- `tcamp env` 这类给任意 OpenAI / Anthropic 兼容客户端打印环境变量的通用逃生口。
- 项目级（repo 内）配置与 `--project`。
- `--fix` 自动修复、`--lang` / i18n、shell completion、man page。
- 发布到 crates.io / npm；Developer ID 签名 / 公证；Windows 产物。

## Further Notes

- Canonical 来源：map #1、已关闭的 child tickets #2–#16（#3 OpenCode 已移出范围）、#13 的验收契约与 121 行测试矩阵。本 issue 是这些决策的汇编，不新增设计取舍；实现时不需要再问设计问题。
- 领域词汇以仓库根 `GLOSSARY.md` 为准：Toolkit / Tool / tcamp、Harness / Adapter / Provider entry、Base URL / Connection / Active connection / Credential / API key、Login / Logout、Config target / Desired state / Managed entry / Overlay / Ownership manifest、Conflict / Adopt / Drift / Shadowing、Configure / Launch、Auth probe / Verify / Doctor / Smoke、Slot / Model mapping / Catalog model / Model reference。平台词汇按 tokencamp-pro 的 `CONTEXT.md` 原样复用。
- 已确认的平台事实：tokencamp 的 `/v1` 只认 `Authorization: Bearer`；OAuth token 不能调 `/v1`；`GET /v1/models` 公开只读，不能校验 key；`GET /v1/generation/{id}` 可用作免费 Auth probe；`/v1/messages` 只服务有 `provider_type: anthropic` deployment 的模型，当前生产 / 预发无可查部署、本地 dev 0 个 Anthropic deployment；`/v1/responses` 是 stateless 子集，拒绝 `include`（含空数组）与 `function_call` / `function_call_output` 回放。
- 已确认的 Harness 事实：Claude Code `2.1.316` 的 `ANTHROPIC_BASE_URL` 要 origin、`ANTHROPIC_AUTH_TOKEN` 发 Bearer、`ANTHROPIC_API_KEY` 发 `x-api-key`；自定义 Base URL 默认开启 tool search，未知模型默认 adaptive thinking；Pi `1.4.0` 的 `models.json` 是 JSONC，`openai-completions` 走 chat completions，`openai-responses` 的 tool loop 第二轮会 400；Kimi Code `2.1.1` 的 `type = "openai"` 走 Bearer + chat completions，`type = "anthropic"` 发 `x-api-key`，`max_output_size` 必须设小值避免 context 闸门。
- Deferred 项（均写明触发条件与 owner）：
  - Claude Code live Smoke：触发条件 = ops 在目标环境添加 `provider_type: anthropic` deployment；owner = 发布经理 / ops；未补测不阻塞 v1。
  - `SPEC.md` 汇编：实现 effort 第一步，原样复制本 issue 与 #13 矩阵；owner = 实现者。
  - T3 nightly 与真实 key 的额外 Smoke：可选，不设发布门槛；owner = 发布经理。
- 已知风险：Claude Code / Pi / Kimi Code 版本迭代快，版本地板和字段可能漂移；tokencamp 生产 / 预发 Base URL 仍标 provisional；模型目录与 deployment 可能变化；静态 curated 表需要随发版维护；macOS 无 Developer ID，v1 不签名不公证；CI 没有 Intel macOS runner，`x86_64-apple-darwin` 只做构建 + `--version` Smoke。
- 实现顺序建议：先按 #13 汇编 `SPEC.md`，再搭 Cargo 骨架与 `install.sh` / release workflow，然后实现 Credential 与 Config 引擎（原子写、manifest、合并、Shadowing），再实现三个 Adapter 与 Model mapping，接着 CLI / Doctor / preflight，最后按 121 行矩阵补 T1、执行 T2、写 README 英文与 `README.zh-CN.md`。
- 文档语言：代码、标识符、commit 用 English；内部设计文档中文；CLI 人类输出 English；README 英文 + `README.zh-CN.md`；docs 站为用户面向事实的唯一权威。


# Part II — v1 验收契约与测试矩阵（#13）

## 结论：v1 验收契约

本 ticket 把「v1 规格完成」与「实现发布」分开验收：

- **规格完成（本 map 关闭条件）**：决策集完整、可执行、无 TBD；测试矩阵是实现 effort 的发布契约，不在本 map 执行。
- **实现发布（后续 effort）**：T1 自动化全绿、T2 手动 smoke/兼容/文档通过、无 P0/P1，发布经理签字。

Canonical spec = map + 已关闭 ticket + 本 comment；`SPEC.md` 由实现 effort 的第一步按本 comment 的目录汇编，不另开 wayfinder ticket。
### 2. v1 发布完成条件

- T1 在 `ubuntu-latest` + `macos-14` 全绿。
- T2 的 Pi/Kimi 真实 smoke 与真实 harness 版本兼容性全部完成。
- README 英文 + `README.zh-CN.md` 作为 docs 入口通过 `tests/docs.rs`；docs 权威页（Start / CLI reference / Security and limitations）文档清单通过。
- 没有 open 的 P0/P1 缺陷。
- 由切 tag 的发布经理在 `RELEASE-CHECKLIST.md` 上签字，不设独立 QA 角色。

### 3. 验收清单

- [ ] **AUTH**：登录、凭据、logout、status、key 脱敏全部通过（AUTH-01..18）。
- [ ] **URL**：base URL 规范化、优先级、active 切换全部通过（URL-01..04）。
- [ ] **CFG-COMMON**：原子写、备份、锁、symlink、所有权、drift、shadowing、uninstall 全部通过（CFG-C-01..14）。
- [ ] **CFG-CLAUDE**：overlay、四槽位、启动、遮蔽检测、deferred smoke 提示通过（CFG-CL-01..08）。
- [ ] **CFG-PI**：models.json、JSONC 保留、provider/model 契约、tool loop 通过（CFG-PI-01..08）。
- [ ] **CFG-KIMI**：config.toml、TOML 保留、alias、max_output、tool loop 通过（CFG-KIMI-01..07）。
- [ ] **CLI**：命令树、首次引导、确认矩阵、JSON、退出码、uninstall、status 通过（CLI-01..14）。
- [ ] **DOCTOR**：所有 check id 的 pass/warn/fail/skip、退出码、JSON、preflight 通过（DR-01..12）。
- [ ] **MOD**：静态映射、槽位、context/max output、覆盖优先级、模型校验通过（MOD-01..06）。
- [ ] **PKG**：四 target、资产、install.sh、release workflow、CI/供应链通过（PKG-01..08）。
- [ ] **NFR**：启动延迟、二进制大小、无遥测、日志脱敏通过（NFR-01..04）。
- [ ] **NEG**：Codex/OpenCode/env/Windows/pro 改动等负向边界通过（NEG-01..04）。
- [ ] **SMOKE / COMPAT / DOC**：T2 手动项完成并签字（SMOKE-01..03、COMPAT-01..04、DOC-01..07；DOC 审查 README 入口 + docs 权威页）。

### 4. 测试矩阵说明

- **T1（PR + release 必过）**：确定性自动化测试；local mock HTTP、fake harness、临时 HOME/XDG；不依赖真实网络或真实 harness。
- **T2（release 必过）**：发布前手动；真实 harness + 真实 tokencamp key + 文档审查。
- **T3（可选）**：nightly 或不设门槛；失败不阻塞发布，但记录。

矩阵列：`ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate`。

证据约定：

- T1 evidence 写测试文件组（实现时落到 `tests/`）。
- T2 evidence 写进仓库根 `RELEASE-CHECKLIST.md`，列：`ID | Date | OS/arch | Harness version | Key (masked) | Result | Request ID | Notes`；DOC-01..07 的 README 入口部分另由 `tests/docs.rs` 机器断言。
- 矩阵 canonical 在本 comment；`SPEC.md` 汇编时原样复制。AUTH-15、NFR-04、PKG-08 与 DOC-01..07 由 [#35](https://github.com/0xnicholas/tokencamp-tools/issues/35) 在 #34 README docs 入口化后修订：AUTH-15 / NFR-04 对齐 §2 的“输出脱敏 + 备份保留原文件字节”两层 API key 规则；PKG-08 与 DOC 的审查对象为 README 入口 + docs 权威页；其余行保持 #13 原文。

### 5. 测试矩阵

#### AUTH — 登录、凭据、logout、status

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| AUTH-01 | T1 | Login | clean HOME/XDG；mock 返回 404 `generation_not_found`；`$KEY` 有效 | `printf '%s' "$KEY" \| tcamp login --key-stdin` | exit 0；保存；目录 0700、文件 0600；JSON `ok:true`；key 掩码 | `tests/auth.rs` | PR+rel |
| AUTH-02 | T1 | Login | mock probe 200 | 同上 | exit 0；保存；`verified_at` 写入 | `tests/auth.rs` | PR+rel |
| AUTH-03 | T1 | Login | mock 401 `authentication` | 同上 | 不保存；exit 1；提示 `<portal>/keys`；TTY 可重试一次；无完整 key | `tests/auth.rs` | PR+rel |
| AUTH-04 | T1 | Login | mock 402/403/429；mock 5xx/网络超时/意外 404 | 同上 | 402/403/429：保存、exit 0、warning code；5xx/网络/意外 404：不保存、exit 1、提示 `--no-verify` | `tests/auth.rs` | PR+rel |
| AUTH-05 | T1 | Login | `--no-verify` | `tcamp login --key-stdin --no-verify` | 0 网络请求；保存；`verified_at` 缺失；exit 0 | `tests/auth.rs` | PR+rel |
| AUTH-06 | T1 | Login | `--verify`；mock probe + chat | `tcamp login --key-stdin --verify` | 额外一次最小 chat：`model:"auto"`、`max_tokens:1`、`stream:false`；401 不保存；402 保存并 warning | `tests/auth.rs` | PR+rel |
| AUTH-07 | T1 | Login UX | TTY / 非 TTY；env 有/无 key | 分别运行 login | TTY 隐藏输入、401 重试一次；非 TTY 无 `--key-stdin`/env exit 2 `missing_key`；非 TTY env 打印来源；TTY env 仍提示输入 | `tests/auth_tty.rs` | PR+rel |
| AUTH-08 | T1 | Credential priority | `TOKENCAMP_API_KEY` + 文件 key 同时存在 | `status --json` | env 覆盖文件；env 不落盘；`key.source="env"` | `tests/auth.rs` | PR+rel |
| AUTH-09 | T1 | Overwrite | 同 base URL 已有 key | 非 TTY 无 `--force`；再 `--force`；TTY 确认 yes/no | 无 force exit 2；force 覆盖并更新 `created_at`、清空 `verified_at`；TTY yes 覆盖，no 不写且 exit 0 | `tests/auth.rs` | PR+rel |
| AUTH-10 | T1 | Key shape | 非 `sk-`+32；空值/控制字符；`--key` 字面量 | 分别 login | 形状异常只 warning 仍可保存；空/控制字符 exit 2；`--key` 不存在 | `tests/auth.rs` | PR+rel |
| AUTH-11 | T1 | Corrupt/symlink | 损坏 TOML；`version` 不支持；credential symlink；file 0644 | login / doctor | 损坏/版本 exit 2 打印路径不覆盖；symlink 拒绝；0644 doctor `fail` | `tests/auth.rs`、`tests/doctor.rs` | PR+rel |
| AUTH-12 | T1 | Atomic/concurrency | 并发 login；模拟 rename 失败 | 两进程同时写；注入 rename 失败 | 原子写；last-writer-wins 不损坏；rename 失败原文件不动、临时文件清理、exit 1 | `tests/auth.rs` | PR+rel |
| AUTH-13 | T1 | Logout | active connection；`--base-url` 指定；`--all` | `tcamp logout ...` | 删当前/指定/全部；active 清空，不自动切；不碰 harness 配置；提示运行中 harness 仍持旧 key、Portal 吊销立即生效 | `tests/auth.rs` | PR+rel |
| AUTH-14 | T1 | Status | 有 key、harness 已配置 | `tcamp status` / `--json` | 显示 active base URL、掩码 key、source、`verified_at`、存储路径、harness installed/configured/target/conflict；无完整 key | `tests/status.rs` | PR+rel |
| AUTH-15 | T1 | Redaction | 固定 sentinel key；覆盖所有命令/错误/diff/备份 | 全命令扫描 stdout/stderr/JSON/备份 | sentinel key 不出现在 stdout/stderr/JSON/diff；备份扫描只确认 `tcamp` 保存的 API key 不进入备份，用户预置的字面量 API key 可留在 `0600` 备份；`request_id` 只在 JSON 或 `-v`；无栈/原始响应体 | `tests/redaction.rs` | PR+rel |
| AUTH-16 | T1 | Portal hint | 官方云 401；自托管 401 | login | 官方云给 `https://tokencamp.com/keys`；自托管只打印 base URL，不猜路径 | `tests/auth.rs` | PR+rel |
| AUTH-17 | T1 | Paths | 自定义 `XDG_CONFIG_HOME`；未设置 | login | 凭据在 `$XDG_CONFIG_HOME/tokencamp/credentials.toml`；fallback `~/.config/tokencamp/`；0700/0600 | `tests/auth.rs` | PR+rel |
| AUTH-18 | T1 | HTTP policy | 公网 `http://`；loopback/RFC1918/link-local `http://`；https | login/status | 公网明文拒绝 exit 2；内网/本地 HTTP 允许并 warning；HTTPS 允许 | `tests/url.rs` | PR+rel |

#### URL — base URL 与 connection

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| URL-01 | T1 | Normalize | `https://host/v1/`、`https://host/prefix/v1`、尾部斜杠 | login/status | 存为不带 `/v1` 的 root：`https://host`、`https://host/prefix`；尾斜杠去掉 | `tests/url.rs` | PR+rel |
| URL-02 | T1 | Precedence | `--base-url`、`TOKENCAMP_BASE_URL`、active、默认同时存在 | login/status | `--base-url` > env > active > `https://api.tokencamp.com` | `tests/url.rs` | PR+rel |
| URL-03 | T1 | Active | 两个 base URL；login 成功；logout active | 切换 active | 一 base URL 一 key；login 成功设为 active；logout active 清空、不自动切；非 active logout 不影响 active | `tests/url.rs` | PR+rel |
| URL-04 | T1 | Harness base | Claude / Pi / Kimi configure | 检查写入值 | Claude `ANTHROPIC_BASE_URL` 用 root；Pi `baseUrl`/Kimi `base_url` 用 `<root>/v1`；非法 URL exit 2 | `tests/config_claude.rs`、`tests/config_pi.rs`、`tests/config_kimi.rs` | PR+rel |

#### CFG-COMMON — 配置写入通用契约

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CFG-C-01 | T1 | Paths/permissions | clean HOME/XDG；已有 0644 配置 | configure | 新目录 0700、新文件 0600；已有文件保留 mode；state/backup 在 `$XDG_STATE_HOME/tokencamp/`，fallback `~/.local/state/tokencamp/` | `tests/config_common.rs` | PR+rel |
| CFG-C-02 | T1 | Atomic write | 模拟 rename 失败 | configure | 原文件不动；临时文件清理；exit 1 | `tests/config_common.rs` | PR+rel |
| CFG-C-03 | T1 | Backup/rollback | 已有配置；模拟 manifest 写失败；模拟回滚失败 | configure | 改动前写时间戳备份（0600，保留 3 份）；manifest 记录；manifest 失败回滚；回滚失败 exit 2 并打印两个路径 | `tests/config_common.rs` | PR+rel |
| CFG-C-04 | T1 | Lock | 两进程同时 configure；持锁超过 5s | configure | flock 5s 超时；拿到锁后重读 hash；读后被改重试一次；仍冲突 exit 1 | `tests/config_common.rs` | PR+rel |
| CFG-C-05 | T1 | Symlink | 配置为 symlink；broken symlink；非普通文件 | configure | 解析最终目标、目标目录原子写、保留链接；broken/非普通拒绝 | `tests/config_common.rs` | PR+rel |
| CFG-C-06 | T1 | Parse failure | 损坏 JSON/JSONC/TOML；结构不合法 | configure / `--dry-run` | exit 2；打印路径+行列号；不写、不备份 | `tests/config_common.rs` | PR+rel |
| CFG-C-07 | T1 | Merge scope | 已有未知字段、注释、键序、缩进、换行风格 | configure | 只改受管条目；未知字段/注释/顺序保留；不格式化整文件、不排序、不删用户内容；新文件 LF | `tests/config_common.rs` | PR+rel |
| CFG-C-08 | T1 | Secret boundary | 已有字面量 key；正常 configure | configure/uninstall | 配置不落 key；Pi 用 `$TOKENCAMP_API_KEY`；Kimi 用 `api_key_env`；Claude overlay 不含 token；字面量 key 当冲突、备份 0600、需 `--force`、永不打印 | `tests/config_common.rs` | PR+rel |
| CFG-C-09 | T1 | Dry-run | clean/existing；`--dry-run`；`--dry-run --json` | `tcamp <harness> --dry-run` | 不建目录、不拿锁、不写备份/状态；stdout unified diff；stderr `no files written`；secret 打码；解析失败 exit 2，否则 0；JSON `changes[]` | `tests/cli.rs` | PR+rel |
| CFG-C-10 | T1 | Force/yes | unowned 条目；非 TTY 冲突；TTY 确认 | configure/uninstall | `--force` 只覆盖 unowned/确认，不跳过解析/校验/原子写/备份；`--yes` 只答 TTY 确认，不能替代 force；非 TTY 冲突无 force exit 2 | `tests/config_common.rs` | PR+rel |
| CFG-C-11 | T1 | Drift/uninstall | 用户改受管字段；tcamp 创建的条目未改；改过；`--force` | configure/uninstall | drift warning 但仍覆盖；uninstall 只删 tcamp 创建且仍等于 last-written 的条目；改过的留着并 warning；force 才删；文件语义为空时删除；凭据/备份/二进制不动 | `tests/config_common.rs` | PR+rel |
| CFG-C-12 | T1 | Shadowing | Claude 用户/项目/ managed token/apiKeyHelper；Pi auth.json；Kimi 字面量 key | configure/launch | Claude 可读冲突拒绝启动，managed 不可读 warning；Pi `tokencamp`/损坏拒绝；Kimi owned 字面量备份移除，unowned 冲突；`--force` 不能绕过；值永不打印 | `tests/shadowing.rs` | PR+rel |
| CFG-C-13 | T1 | Manifest missing | 删 `managed.toml`；Claude overlay 有 `_tcamp`；Pi/Kimi 有条目 | configure/uninstall | Claude 可靠 marker 清理；Pi/Kimi 不自动动，要求 `--force` 或手工；manifest schema/版本不支持 exit 2 | `tests/config_common.rs` | PR+rel |
| CFG-C-14 | T1 | Blast radius | 任意 configure/uninstall | 检查文件系统 | 不写 shell profile、不生成 wrapper、不做项目级；uninstall 跨 harness 尽力执行、汇总失败 exit 1；备份路径打印 | `tests/config_common.rs` | PR+rel |

#### CFG-CLAUDE — Claude Code adapter

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CFG-CL-01 | T1 | Overlay | clean HOME；已有 overlay | `tcamp claude --no-launch` | 写 `$XDG_CONFIG_HOME/tokencamp/harnesses/claude/settings.json`；严格 JSON、2 空格、末尾换行、键序 `$schema`→`_tcamp`→`env`；相同 desired state 不写；变化前备份 | `tests/config_claude.rs` | PR+rel |
| CFG-CL-02 | T1 | Env | configure | 检查 overlay `env` | `ANTHROPIC_BASE_URL`=root；`ANTHROPIC_AUTH_TOKEN` 不落文件；`ANTHROPIC_API_KEY=""`；`ANTHROPIC_MODEL` + 四个 `ANTHROPIC_DEFAULT_*_MODEL`；`ENABLE_TOOL_SEARCH=false`；`CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS=1`；`MAX_THINKING_TOKENS=0`；`CLAUDE_CODE_MAX_CONTEXT_TOKENS`=1048576 或已知更小窗口；`CLAUDE_CODE_MAX_OUTPUT_TOKENS=32768` | `tests/config_claude.rs` | PR+rel |
| CFG-CL-03 | T1 | Launch | 已配置；fake `claude` | `tcamp claude --no-launch` / launch | 启动传 `--settings <overlay>`；不修改 `$CLAUDE_CONFIG_DIR/settings.json`（默认 `~/.claude/settings.json`）；`CLAUDE_CONFIG_DIR` 用于遮蔽检查 | `tests/config_claude.rs` | PR+rel |
| CFG-CL-04 | T1 | Ownership | overlay 缺 `_tcamp`/版本不匹配；用户改过；uninstall | configure/uninstall | 冲突 exit 2；`--force` 先备份再接管；uninstall 只删未改的 overlay，改过留着 warning，force 才删 | `tests/config_claude.rs` | PR+rel |
| CFG-CL-05 | T1 | Shadowing | 用户/项目/local/managed settings 有 token 或 `apiKeyHelper`；managed 不可读 | launch | 可读冲突拒绝启动并打印路径+键名；managed 不可读 warning 并继续；`--force` 不能绕过；值不打印 | `tests/shadowing.rs` | PR+rel |
| CFG-CL-06 | T1 | Binary/version | fake `claude` 不在 PATH；版本 2.1.303 / 2.1.304 | launch / doctor | 未安装：不带 `--no-launch` exit 2 给安装链接；`--no-launch` 写配置 exit 0；doctor 未安装 warn；低于 2.1.304 doctor warn 但启动继续 | `tests/config_claude.rs`、`tests/doctor.rs` | PR+rel |
| CFG-CL-07 | T1 | Messages/deferred | 无 anthropic deployment；`doctor --verify --harness claude` | launch / doctor | `tcamp claude` exec 前 stderr 一行 `/v1/messages capability unverified; requires an anthropic deployment; run tcamp doctor --harness claude`，不阻断；doctor 默认 `warn`；verify 发一次 `/v1/messages`，404 `model_not_found` 给 deployment hint；401/402/400/404/5xx/网络 fail，403/429 warn | `tests/config_claude.rs`、`tests/doctor.rs` | PR+rel |
| CFG-CL-08 | T1 | Models/flags | `--model` canonical / `auto` / `@preset/...` / `~family-latest` | `tcamp claude --model ...` | 只覆盖主模型；opus/sonnet/haiku/fable 不变；配置不变；非法值 exit 2 并列出可用 id；不写 `modelPicker`、`ANTHROPIC_CUSTOM_MODEL_OPTION`、`ANTHROPIC_SMALL_FAST_MODEL`、capabilities | `tests/config_claude.rs` | PR+rel |

#### CFG-PI — Pi adapter

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CFG-PI-01 | T1 | File/path | clean HOME；`PI_CODING_AGENT_DIR` 设置/未设置 | `tcamp pi --no-launch` | 写 `$PI_CODING_AGENT_DIR/models.json`（fallback `~/.pi/agent/models.json`）；新文件含 `$schema` + `providers`；不写项目级 models.json | `tests/config_pi.rs` | PR+rel |
| CFG-PI-02 | T1 | Provider | configure | 检查 `providers.tokencamp` | `name:"tokencamp"`；`baseUrl:"<root>/v1"`；`api:"openai-completions"`；`apiKey:"$TOKENCAMP_API_KEY"`；`compat.maxTokensField:"max_tokens"`、`supportsDeveloperRole:false`、`supportsStore:false`、`supportsReasoningEffort:false` | `tests/config_pi.rs` | PR+rel |
| CFG-PI-03 | T1 | Models/defaults | clean；已有 defaultProvider/defaultModel | configure | 5 个模型 `zhipu/glm-5.3`、`deepseek/deepseek-v4-pro`、`zhipu/glm-5.3-flash`、`kimi/kimi-k2.7-code`、`tokencamp/auto`；含 id/name/input/contextWindow/maxTokens/compat；不写 cost；default 仅在缺失/为空时补 | `tests/config_pi.rs` | PR+rel |
| CFG-PI-04 | T1 | JSONC preserve | 已有 `//` 注释、trailing comma、未知字段、键序、空白 | configure | 保留注释/trailing comma/键序/空白/缩进；只 upsert；新条目追加；已有文件不强行补 `$schema`；无法保留注释 abort exit 2，`--force` 也不重写 | `tests/config_pi.rs` | PR+rel |
| CFG-PI-05 | T1 | Ownership | 已有非 tcamp `providers.tokencamp`；manifest 丢失；drift；uninstall | configure/uninstall | unowned 冲突 exit 2；`--force` 备份+adopt、保留未知字段；manifest 记录 ownership/model ids/hash/备份；drift warning 但覆盖；uninstall 语义同 CFG-C-11 | `tests/config_pi.rs` | PR+rel |
| CFG-PI-06 | T1 | Shadowing | `auth.json` 含 `tokencamp`；损坏；正常 | launch | 含 `tokencamp` 拒绝并提示删条目/切换登录；损坏拒绝；不读值、不打印、不改 `auth.json` | `tests/shadowing.rs` | PR+rel |
| CFG-PI-07 | T1 | Binary/version | fake `pi` 缺失；版本 1.3.x / 1.4.0 | launch / doctor | 未安装：不带 `--no-launch` exit 2；`--no-launch` 写配置 exit 0；doctor 未安装 warn；低于 1.4.0 warn，启动继续 | `tests/config_pi.rs`、`tests/doctor.rs` | PR+rel |
| CFG-PI-08 | T1 | Models/tool loop | 5 个暴露模型；local mock chat + tool loop | `tcamp pi --model ...`；mock 两轮 | 只接受 5 个模型/alias；非法 exit 2；写 `openai-completions` 不写 `openai-responses`；`assistant.tool_calls` + `role:"tool"` 两轮成功 | `tests/config_pi.rs`、`tests/mock_pi.rs` | PR+rel |

#### CFG-KIMI — Kimi Code adapter

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CFG-KIMI-01 | T1 | File/provider | clean HOME；`KIMI_CODE_HOME` 设置/未设置 | `tcamp kimi --no-launch` | 写 `$KIMI_CODE_HOME/config.toml`（fallback `~/.kimi-code/config.toml`）；`[providers.tokencamp]`：`type:"openai"`、`base_url:"<root>/v1"`、`api_key_env:"TOKENCAMP_API_KEY"` | `tests/config_kimi.rs` | PR+rel |
| CFG-KIMI-02 | T1 | Models | clean；已有 `default_model` | configure | 5 个 alias `tokencamp/glm-5.3`、`tokencamp/deepseek-v4-pro`、`tokencamp/glm-5.3-flash`、`tokencamp/kimi-k2.7-code`、`tokencamp/auto`；`provider:"tokencamp"`、canonical model、`max_context_size`、`max_output_size=8192`；default 仅在缺失/为空时写 | `tests/config_kimi.rs` | PR+rel |
| CFG-KIMI-03 | T1 | TOML preserve | 已有注释、空行、其他表、未知字段 | configure | `toml_edit` 保留注释/空行/表顺序；只 upsert；新表追加末尾；未知字段保留；不写 `default_model` 已有值；不加标记注释 | `tests/config_kimi.rs` | PR+rel |
| CFG-KIMI-04 | T1 | Literal key | owned/unowned `api_key` 字面量 | configure | owned：备份并移除，改 `api_key_env`；unowned：冲突 exit 2，`--force` 才接管；值永不打印 | `tests/config_kimi.rs` | PR+rel |
| CFG-KIMI-05 | T1 | Ownership | 非 tcamp provider/alias；manifest 丢失；drift；uninstall | configure/uninstall | unowned 冲突 exit 2；force 备份+adopt；manifest 记录；drift warning 但覆盖；uninstall 同 CFG-C-11 | `tests/config_kimi.rs` | PR+rel |
| CFG-KIMI-06 | T1 | Binary/version | fake `kimi` 缺失；版本 2.1.0 / 2.1.1 | launch / doctor | 未安装：不带 `--no-launch` exit 2；`--no-launch` 写配置 exit 0；doctor 未安装 warn；低于 2.1.1 warn，启动继续；launch 用 `-m <alias>` | `tests/config_kimi.rs`、`tests/doctor.rs` | PR+rel |
| CFG-KIMI-07 | T1 | Models/tool loop | 5 个 alias；local mock chat/tool loop | `tcamp kimi --model ...`；mock 两轮 | 只接受 5 个 alias/canonical；非法 exit 2；`type=openai` 走 `/v1/chat/completions`；tool loop 成功；不使用 `type=anthropic` | `tests/config_kimi.rs`、`tests/mock_kimi.rs` | PR+rel |

#### CLI — 命令树、交互、JSON、退出码

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CLI-01 | T1 | Command tree | clean | `tcamp`、`tcamp help`、`tcamp --help` | 含 claude/pi/kimi/login/logout/status/doctor/uninstall/help；无 codex/opencode/env；裸跑 help + 检测到的 harness，exit 0 | `tests/cli.rs` | PR+rel |
| CLI-02 | T1 | Flags/passthrough | 各命令 flags；`--` 前后参数 | `tcamp claude --json`；`tcamp pi -- --foo`；`tcamp pi --foo` | `--json` 与 launch 互斥 exit 2 hint `--no-launch`；`--` 后原样透传；`--` 前未知 flag exit 2 hint `--` | `tests/cli.rs` | PR+rel |
| CLI-03 | T1 | First run | TTY、无 key、无 `--json`；非 TTY；`--no-launch` | `tcamp <harness>` | TTY 隐藏输入→probe→`Write it? [Y/n]`→写→exec；非 TTY 无 key exit 2 `missing_key` + hint；`--no-launch` 不引导，缺 key exit 2 | `tests/cli_tty.rs` | PR+rel |
| CLI-04 | T1 | Daily/configured | 已有 key；配置无变化/有变化 | `tcamp <harness>` | 无变化 stderr `Launching <harness> ...`；有写入 `Configured <harness>: <path>`；已配置不再问写入；确认只发生在首次写入/login 覆盖/uninstall 计划 | `tests/cli.rs` | PR+rel |
| CLI-05 | T1 | JSON | `--json`；需要确认但无 `--yes/--force` | `tcamp <cmd> --json` | 不弹交互；需要确认时 exit 2；stdout 恰好一个 JSON 对象；warnings 在 `warnings[]`；stderr 只留致命 I/O | `tests/cli_json.rs` | PR+rel |
| CLI-06 | T1 | Streams/color | TTY / pipe；`NO_COLOR`/`--no-color`/`TERM=dumb` | 各命令 | 进度/提示/确认 stderr；结果 stdout；颜色只在 TTY；尊重禁用；无转圈/光标动画 | `tests/cli.rs` | PR+rel |
| CLI-07 | T1 | Exit codes | 成功；运行期失败；用法/缺 key/冲突/损坏；exec harness | 各命令 | 0 成功；1 网络/401/5xx/写失败；2 用法/缺 key/未安装/冲突/损坏/需确认不可交互；exec 后退出码/信号归 harness | `tests/cli.rs` | PR+rel |
| CLI-08 | T1 | JSON shape | 成功/失败/`--dry-run --json` | 各命令 | 成功 `{ok,base_url,key:{masked,source,verified,verified_at},harnesses,warnings}`；失败 `{ok:false,error:{code,message,hint,request_id?}}`；codes `missing_key`/`harness_not_found`/`config_conflict`/`config_unreadable`/`network`/`usage`；dry-run `changes[]` | `tests/cli_json.rs` | PR+rel |
| CLI-09 | T1 | Errors | 401/402/冲突/损坏/网络 | 各命令 | 人类模式 2–3 行 `error: <原因> [<code>]` + `hint:`；request_id 默认不打印，JSON/`-v` 才有；无栈/原始响应体/完整 key | `tests/cli.rs` | PR+rel |
| CLI-10 | T1 | Exec | fake harness；`--` 参数 | `tcamp <harness>` | 配置成功后 exec；stderr `Launching <harness> ...`；透传 `--` 后参数；不生成 wrapper | `tests/cli.rs` | PR+rel |
| CLI-11 | T1 | 402 | 运行期 402；`--verify --no-launch` 402 | launch/verify | 402 warning + 下一步（官方云账单/充值，自托管 workspace budget）；不阻断配置/启动；`--verify --no-launch` 写完配置 exit 0 带 warning | `tests/cli.rs` | PR+rel |
| CLI-12 | T1 | Missing harness | fake harness 不在 PATH；`--no-launch` | `tcamp <harness>` | 不带 `--no-launch` exit 2 给安装链接；`--no-launch` 即使缺失也写配置 exit 0；doctor 标 warn | `tests/cli.rs`、`tests/doctor.rs` | PR+rel |
| CLI-13 | T1 | Status/uninstall | 已配置多 harness；TTY/非 TTY；`--json`；`--harness`；`--dry-run`；`--force` | `tcamp status`；`tcamp uninstall ...` | status 只本地；uninstall 默认全部、先计划、TTY 确认、`--yes` 跳过；非 TTY/JSON 无 yes exit 2；`--harness` 过滤；dry-run 只打印；保留凭据/备份/二进制；force 才删改过条目；部分失败汇总 exit 1 | `tests/cli.rs` | PR+rel |
| CLI-14 | T1 | Language/version/help | `-V`；help；中文文档 | 各命令 | CLI/help 英文；`tcamp <semver>`；help 给 quickstart `tcamp login`→`tcamp <harness>`；文档中文、README 英文+zh-CN；无 i18n/`--lang` | `tests/cli.rs` | PR+rel |

#### DOCTOR — doctor 与 preflight

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DR-01 | T1 | Read-only | 任意状态；无 `--fix` | `tcamp doctor` | 只读；不写文件/不建目录/不拿锁/不交互；state board `pass/warn/fail/skip` + summary；无 `--fix` | `tests/doctor.rs` | PR+rel |
| DR-02 | T1 | credential.file | 文件缺失+env；目录非 0700；文件 0644；损坏/版本不支持；symlink | doctor | 缺失+env warn；目录权限 warn；文件权限/损坏/版本/symlink fail；密码不打印 | `tests/doctor.rs` | PR+rel |
| DR-03 | T1 | credential.key/base_url | 无 key；形状异常；env 覆盖；HTTPS；内网 HTTP；公网 HTTP；非法 URL | doctor | key 缺失 fail、形状 warn、env 覆盖 warn；base URL HTTPS/内网 HTTP pass、公网 HTTP/非法 fail | `tests/doctor.rs` | PR+rel |
| DR-04 | T1 | network.reachable/auth | 无 `--network`；有/无 key；mock 2xx/401/404 code/402/403/429/5xx/超时 | `doctor --network` | reachable skip/pass/fail；auth skip/pass/warn/fail 按 #12 表；无 key 全 skip | `tests/doctor_network.rs` | PR+rel |
| DR-05 | T1 | network.verify | `--verify`；mock 2xx/401/402/400/403/404/429/5xx/超时 | `doctor --verify` | 隐含 `--network`；整次最多一个真实请求、不重试、30s、`max_tokens:1`、`stream:false`、`model:"zhipu/glm-5.3"`；2xx pass；403/429 warn；401/402/400/404/5xx/网络 fail | `tests/doctor_network.rs` | PR+rel |
| DR-06 | T1 | Harness binary | 未选/未安装/低于地板/版本解析失败 | doctor | 未选 skip；未安装 warn；低于地板 warn；`--version` 失败/无法解析 fail；地板 Claude 2.1.304/Pi 1.4.0/Kimi 2.1.1 | `tests/doctor.rs` | PR+rel |
| DR-07 | T1 | Config/ownership/backups | 未配置；解析损坏；不可写；manifest 缺失/target 缺失/broken symlink；drift；备份缺失 | doctor | 未配置 skip；解析/不可写/target 缺失/symlink fail；manifest ok pass；drift/备份缺失/Claude marker 丢失 warn；manifest schema fail；unowned 冲突 fail | `tests/doctor.rs` | PR+rel |
| DR-08 | T1 | Shadowing/messages | Claude 可读冲突/managed 不可读；Pi auth；Kimi literal；Claude 默认/verify | doctor | Claude 冲突 fail、managed 不可读 warn（人类标红）；Pi `tokencamp`/损坏 fail；Kimi owned literal warn、unowned literal fail；Claude messages 默认 warn；verify 2xx pass、404 hint、401/402/400/404/5xx fail、403/429 warn | `tests/doctor.rs` | PR+rel |
| DR-09 | T1 | Models | 离线映射完整/缺槽/禁用值；联网全 200/任一 404/网络失败 | doctor / `--network` | 离线：完整且无禁用值 pass，缺槽/禁用值 fail；联网：全 200 pass，404/网络/超时 fail；检查 Claude 5 个去重 id、Pi/Kimi 5 个 id；`tokencamp/auto` 也查 | `tests/doctor.rs` | PR+rel |
| DR-10 | T1 | Exit/filter | fail 数 0/1；用法错误；`--harness` 合法/非法/重复 | doctor | 0 无 fail；1 有 fail；2 仅用法错误；`--harness` 只过滤 harness 块，connection/network 始终显示；非法/重复 exit 2 | `tests/doctor.rs` | PR+rel |
| DR-11 | T1 | JSON shape | 成功/失败报告；离网 | `doctor --json` | 恰好一个对象；`ok` 与 fail==0 同义；`key.verified` 离线 null；`checks[]` 含 id/scope/harness/status/message/hint/code/request_id；`warnings[]`；`summary` 只统计实际输出 | `tests/doctor_json.rs` | PR+rel |
| DR-12 | T1 | Preflight | 本地检查；模型 `/v1/models/{id}`；网络不可达；404；`--no-launch`；`--dry-run` | `tcamp <harness>` | preflight 只跑 credential/base URL/binary/config/ownership/conflict/shadowing + 本次相关模型存在性（并行，总预算 2s）；404 exit 2；网络 warn 继续；不跑 auth probe/verify/`--version`；`--no-launch` 跳过 binary/shadow 阻塞但 shadow 仍 warn；`--dry-run` 不联网/不写/不拿锁/不建目录，只查 credential/base URL/config/conflict，diff 后 exit 0/2 | `tests/preflight.rs` | PR+rel |

#### MOD — 模型映射

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| MOD-01 | T1 | Static table | 三个 harness configure | 检查写入值 | Claude：主 `zhipu/glm-5.3`、opus `deepseek/deepseek-v4-pro`、sonnet `zhipu/glm-5.3`、haiku `zhipu/glm-5.3-flash`、fable `deepseek/deepseek-v4-pro`；Pi/Kimi 5 个模型；context/max output 按 #11 表 | `tests/models.rs` | PR+rel |
| MOD-02 | T1 | Claude slots | Claude configure | 检查 `env` | 四个 `ANTHROPIC_DEFAULT_*_MODEL` 必写；不用 `auto`/`@preset`/`~family-latest`；`CLAUDE_CODE_MAX_CONTEXT_TOKENS` 取本次主模型+槽位最小已知窗口，未知 200000；输出 32768 | `tests/config_claude.rs` | PR+rel |
| MOD-03 | T1 | Pi fields | Pi configure | 检查 provider/models | provider/compat 按 CFG-PI-02；models 含 id/name/input/contextWindow/maxTokens/compat；不写 cost；default 仅缺失/为空 | `tests/config_pi.rs` | PR+rel |
| MOD-04 | T1 | Kimi fields | Kimi configure | 检查 provider/models | alias 5 个；canonical model；`max_output_size=8192`；default 仅缺失/为空 | `tests/config_kimi.rs` | PR+rel |
| MOD-05 | T1 | Precedence | `--model`、`TOKENCAMP_MODEL`、默认同时存在；env 空串 | launch | `--model` > env > 默认；只覆盖主/默认，不逐槽位；env 空视同未设置；`--model` 只影响本次启动，不改配置 | `tests/models.rs` | PR+rel |
| MOD-06 | T1 | Validation | Claude canonical / auto / preset / family；Pi/Kimi 5 个/未暴露；`/v1/models/{id}` 404/网络失败 | launch/preflight | Claude 只接受 canonical，非法 exit 2 hint 可用 id；Pi/Kimi 只接受 5 个，非法 exit 2 hint；404 exit 2；网络 warn 继续；不静默回退 | `tests/models.rs` | PR+rel |

#### PKG — 打包、分发、发布

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PKG-01 | T1 | Crate | 仓库 | `cargo metadata`；检查 `Cargo.toml` | 单 package；`edition=2024`、`rust-version=1.85`、`license=Apache-2.0`、`[[bin]] name="tcamp"`；release `lto="thin"`、`strip="symbols"`、`codegen-units=1`；`.gitignore` 忽略 `/target`；提交 `Cargo.lock` | `tests/packaging.rs` | PR+rel |
| PKG-02 | T1 | Targets | CI matrix | 构建四个 target | `aarch64-apple-darwin`、`x86_64-apple-darwin`（`MACOSX_DEPLOYMENT_TARGET=11.0`）、`x86_64-unknown-linux-musl`、`aarch64-unknown-linux-musl`；Linux 只发 musl；各 target `tcamp --version` | `.github/workflows/*.yml` | PR+rel |
| PKG-03 | T1 | Assets | release build | 检查归档 | `tcamp-<version>-<target>.tar.gz`；根目录只有 `tcamp` 与 `LICENSE`；每个 `.sha256`；合并 `SHA256SUMS`（两空格）；只有 `.tar.gz` | `tests/packaging.rs` | PR+rel |
| PKG-04 | T1 | install.sh | 本地归档+校验和 | 执行 install.sh | POSIX `sh`；`TCAMP_INSTALL_DIR` 默认 `$HOME/.local/bin`；`TCAMP_VERSION`/`--version`；`TCAMP_RELEASES_URL`；平台检测；checksum；解包确认；原子替换；`--version` 比对；PATH hint；无 sudo/profile/quarantine/uninstall | `tests/install_sh.rs` | PR+rel |
| PKG-05 | T1 | install fail | 篡改 checksum；不支持平台；缺 sha 工具；已有安装 | install.sh | checksum 不匹配 exit 1 `refusing to install`；不支持/用法 exit 2；替换前失败不动已有安装；临时目录清理；`install-smoke` 正/负例；`shellcheck` + `sh -n` | `.github/workflows/*.yml`、`tests/install_sh.rs` | PR+rel |
| PKG-06 | T1 | Release workflow | push `v*` tag；缺 CHANGELOG 节；tag 非 main 祖先 | release.yml | `verify` fmt/clippy/test；`build` 四 target；`publish` 先 ancestor check；CHANGELOG 缺失失败；prerelease；`contents:write`；concurrency；资产齐全 | `.github/workflows/release.yml` | PR+rel |
| PKG-07 | T1 | CI/supply | PR / 周任务 | CI | PR 矩阵 `ubuntu-latest`+`macos-14`；MSRV job；`cargo-deny` 周更；Dependabot cargo/actions；最小 permissions；`--locked`；provenance；immutability；不发 crates.io/npm | `.github/workflows/*.yml` | PR+rel |
| PKG-08 | T1 | Docs/signing | 发布前 | 检查文档 | v1 不签名/不公证；`README.md` + `README.zh-CN.md` 作为 docs 入口：6 个 H2 中英文对齐、无 H3、无 Troubleshooting/Known limitations/Uninstall、只重复 7 项白名单（Implementation Decisions §8）与 Security 摘要，并按要求链 docs 三页（Install/Quickstart→Start，Configuration→Reference，Security→Security）；Gatekeeper、卸载、已知限制、ownership 细节只在 docs，`install.sh` 不自动去 quarantine；`SECURITY.md` 指向 Private Vulnerability Reporting；无 CONTRIBUTING/issue/PR 模板/逐文件 license header；无 updater | `tests/docs.rs`、`tests/release_workflow.rs`（SECURITY / CONTRIBUTING / license header） | PR+rel |

#### NFR — 非功能

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| NFR-01 | T1 | Startup latency | release build；warm FS；20 次 | `tcamp --version`/`--help`；离线 `status`/`doctor` | p95：`--version`/`--help` ≤100ms；`status`/`doctor` ≤500ms；release 硬门禁，PR 只记录 | `tests/perf.rs` | rel |
| NFR-02 | T1 | Binary size | release artifacts | 测量 | 未压缩 ≤15 MiB/target；`tar.gz` ≤6 MiB/target；release 硬门禁 | `.github/workflows/release.yml` | rel |
| NFR-03 | T1 | No telemetry | 离线命令；dead proxy；静态审计 | 运行命令 | 除 login/doctor `--network`/`--verify`/preflight 模型检查外，0 出站请求；无更新检查/分析；dead proxy 下离线命令正常 | `tests/nfr.rs` | PR+rel |
| NFR-04 | T1 | Redaction | sentinel key；全命令/JSON/diff/备份 | 扫描输出 | 完整 key 不出现在 stdout/stderr/JSON/diff；`tcamp` 生成的日志/错误/hint 同样不含 key；变更前备份按原文件字节保存（`0600`），可含用户预置的字面量 API key，但不含 `tcamp` 保存的 API key；掩码 `sk-…` + 后 4 位；request_id 仅 JSON/`-v` | `tests/redaction.rs` | PR+rel |

#### NEG — 负向与范围边界

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| NEG-01 | T1 | Codex | clean HOME | `tcamp codex`；`doctor` | exit 2；不写任何 Codex 配置；错误提示 Codex 不在 v1 支持范围并指向 README；doctor 无 Codex 检查 | `tests/cli.rs` | PR+rel |
| NEG-02 | T1 | Removed commands | clean HOME | `tcamp opencode`；`tcamp env`；`--project`；`--fix`；`--lang` | OpenCode 不在命令树/exit 2 且不写配置；`tcamp env` 不存在；`--project`/`--fix`/`--lang` 不存在 | `tests/cli.rs` | PR+rel |
| NEG-03 | T1 | No proxy/wrapper | 任意 configure/launch | 检查文件/进程 | 不生成 shell wrapper；不转发流量；不改默认 provider/shell profile；不写项目级配置；不接 keyring；不支持热轮换 | `tests/config_common.rs` | PR+rel |
| NEG-04 | T1 | Platform/pro | 安装器/构建 | 检查 | Windows 无产物、install.sh exit 2；不修改 tokencamp-pro；无 GUI/用量面板/遥测 | `tests/packaging.rs` | PR+rel |

#### SMOKE — T2 真实 smoke

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SMOKE-01 | T2 | Pi | 干净临时 HOME/XDG/PI_CODING_AGENT_DIR；真实 key；Pi 1.4.0/current | login → `tcamp pi --no-launch` → `tcamp pi -- -p 'Reply with exactly TCAMP_SMOKE_OK and nothing else.'` | exit 0；stdout 含 `TCAMP_SMOKE_OK`；stderr 无 `error:`；记录版本/掩码 key/request_id；单次 <$0.01 | `RELEASE-CHECKLIST.md#SMOKE-01` | rel |
| SMOKE-02 | T2 | Kimi | 干净临时 HOME/XDG/KIMI_CODE_HOME；真实 key；Kimi 2.1.1/current | login → `tcamp kimi --no-launch` → `tcamp kimi -- -p 'Reply with exactly TCAMP_SMOKE_OK and nothing else.'` | exit 0；stdout 含 `TCAMP_SMOKE_OK`；stderr 无 `error:`；记录版本/掩码 key/request_id；单次 <$0.01 | `RELEASE-CHECKLIST.md#SMOKE-02` | rel |
| SMOKE-03 | T2 | Claude (deferred) | 无 anthropic deployment；若 ops 补了 `deepseek/deepseek-v4.1-flash` 或 `deepseek/deepseek-v4-pro` | 同流程 `claude -p` | 默认记录 `deferred`，触发条件“ops 添加 anthropic deployment”，owner 发布经理/ops；补测时 exit 0、stdout 含 `TCAMP_SMOKE_OK`、单次 <$0.01；未补测不算失败 | `RELEASE-CHECKLIST.md#SMOKE-03` | rel |

#### COMPAT — T2 真实 harness 版本兼容

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| COMPAT-01 | T2 | Claude | 真实 Claude 2.1.304 + 2.1.316；无 live request | `tcamp claude --no-launch`；launch | overlay 被真实 Claude 解析；`--settings` 生效；用户 settings 未被改；遮蔽检测正确；不要求 `/v1/messages` 成功 | `RELEASE-CHECKLIST.md#COMPAT-01` | rel |
| COMPAT-02 | T2 | Pi | 真实 Pi 1.4.0 + current | `tcamp pi --no-launch`；launch | models.json 被真实 Pi 解析；provider/model 可选；不写坏 JSONC | `RELEASE-CHECKLIST.md#COMPAT-02` | rel |
| COMPAT-03 | T2 | Kimi | 真实 Kimi 2.1.1 + current | `tcamp kimi --no-launch`；launch | config.toml 被真实 Kimi 解析；alias 可选；`max_output_size` 生效 | `RELEASE-CHECKLIST.md#COMPAT-03` | rel |
| COMPAT-04 | T2 | OS/paths | macOS + Linux（至少一个 x86_64 和一个 arm64） | 检查路径/权限/symlink/flock | 路径变量、0700/0600、symlink、flock 行为符合 CFG-C-01..05 | `RELEASE-CHECKLIST.md#COMPAT-04` | rel |

#### DOC — 文档验收

| ID | Tier | Area | Precondition / Fixture | Action | Expected | Evidence | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DOC-01 | T2 | Install | README EN/zh-CN + docs Start › Install | 审查 | README 只保留 canonical install 命令、一条 `TCAMP_VERSION` pin、OS 行、`not signed or notarized`、`no updater, re-run the installer` 并链 docs；docs 覆盖 curl 安装、版本锁定、校验和、支持平台、`TCAMP_INSTALL_DIR`、PATH、Gatekeeper 说明 | `tests/docs.rs`、`RELEASE-CHECKLIST.md#DOC-01` | rel |
| DOC-02 | T2 | Quickstart | README EN/zh-CN + docs Start › Log in and Configure and launch | 审查 | README 保留四行 quickstart 与一句 summary 并链 docs；docs 覆盖 `tcamp login` → `tcamp claude/pi/kimi`、首次引导/非 TTY、`--no-launch`；README 不重复这些细节 | `tests/docs.rs`、`RELEASE-CHECKLIST.md#DOC-02` | rel |
| DOC-03 | T2 | Configuration | README EN/zh-CN + docs Reference › Config targets | 审查 | README 保留一句 + 三行 Config target 表并链 docs；docs 覆盖目标路径、overlay、ownership、managed.toml、drift/冲突/`--force`、不落 key；README 不重复 ownership 细节 | `tests/docs.rs`、`RELEASE-CHECKLIST.md#DOC-03` | rel |
| DOC-04 | T2 | Troubleshooting | docs Security › Troubleshooting；README EN/zh-CN 无该章节 | 审查 | docs 覆盖无 key、401、402、网络、harness 未安装/版本过低、配置损坏、Claude 404/anthropic deployment；README 不保留 Troubleshooting | `tests/docs.rs`、`RELEASE-CHECKLIST.md#DOC-04` | rel |
| DOC-05 | T2 | Security/limits | README EN/zh-CN + docs Security + SECURITY.md | 审查 | README 保留安全摘要（`0600`/`0700`、原子写、拒绝 symlink、Launch 注入、字面量 API key（README 称 `literal key`）备份 caveat、运行中的 Harness 保留旧 key 直到重启、Portal 吊销、无 telemetry/更新检查）并链 docs 与 `SECURITY.md`；docs 覆盖凭据路径/`0600`、key 只在启动注入、Portal 吊销、uninstall 不删凭据、Known limitations（Codex、Claude live smoke、Windows、无 keyring/热轮换/项目级）；README 不保留 Known limitations | `tests/docs.rs`、`RELEASE-CHECKLIST.md#DOC-05` | rel |
| DOC-06 | T2 | Uninstall | docs Reference › Uninstall；README EN/zh-CN 无该章节 | 审查 | docs 覆盖 `tcamp uninstall` 清理配置、保留凭据/备份、提示 `rm "$(command -v tcamp)"`、不提供 updater；README 只保留白名单中的 `no updater, re-run the installer` 句，不保留卸载细节 | `tests/docs.rs`、`RELEASE-CHECKLIST.md#DOC-06` | rel |
| DOC-07 | T2 | Terminology/parity | README EN/zh-CN + docs + SPEC.md | 审查 | 全部使用 `GLOSSARY.md` 术语，不用禁用同义词；README 无 H3、语言切换与 `v0.1` / `[SPEC.md]` / `[GLOSSARY.md]` 链接存在，英文/中文 6 个 H2 对齐、docs 链接与白名单事实一致；docs 为唯一权威，README 只做入口、白名单重复与 Security 摘要；CLI 输出英文 | `tests/docs.rs`、`RELEASE-CHECKLIST.md#DOC-07` | rel |

### 6. 真实 smoke 操作定义（T2）

每个 harness 从**干净 home** 开始：

1. 创建临时 `HOME`、`XDG_CONFIG_HOME`、`XDG_STATE_HOME`、`CLAUDE_CONFIG_DIR`/`PI_CODING_AGENT_DIR`/`KIMI_CODE_HOME`；`PATH` 含 `tcamp` 与对应 harness。
2. `printf '%s' "$TOKENCAMP_API_KEY" | tcamp login --key-stdin`。
3. `tcamp <harness> --no-launch`。
4. `tcamp <harness> -- -p 'Reply with exactly TCAMP_SMOKE_OK and nothing else.'`（Pi 用 `pi -p`；Kimi 用 `kimi -p`；Claude 解除阻塞后用 `claude -p`）。
5. 断言 exit 0、stdout 含 `TCAMP_SMOKE_OK`、stderr 无 `error:`；在 `RELEASE-CHECKLIST.md` 记录 harness 版本、掩码 key、request_id（有的话）、成本。
6. Claude 默认 `deferred`；触发条件 = ops 在目标环境添加 `provider_type: anthropic` deployment（`deepseek/deepseek-v4.1-flash` 或 `deepseek/deepseek-v4-pro`，base URL `https://api.deepseek.com/anthropic`）；owner = 发布经理/ops。未补测不阻塞 v1。
7. 成本上限：每个 harness 单次 <$0.01，整个 v1 验收 <$0.03；用默认主模型 `zhipu/glm-5.3`，不用 `auto`/强模型。

### 7. 非功能门槛执行方式

- **二进制大小**：release workflow 按 target 硬门禁，超过 15 MiB（未压缩）或 6 MiB（tar.gz）直接失败。
- **启动延迟**：只在 release build 上测（20 次、warm FS、p95），发布时硬门禁；PR CI 记录但不因 runner 噪声失败。
- **无遥测 / 日志脱敏**：T1 硬门禁，每次 PR 跑。
- **文档**：T2 人工清单，发布时硬门禁。

### 8. Ticket → Matrix IDs

| Ticket | Matrix IDs |
| --- | --- |
| [Claude Code 适配器契约](https://github.com/0xnicholas/tokencamp-tools/issues/2) | CFG-CL-02、CFG-CL-08、MOD-02、DOC-04 |
| [OpenCode 适配器契约](https://github.com/0xnicholas/tokencamp-tools/issues/3) | NEG-02 |
| [Kimi Code 适配器契约](https://github.com/0xnicholas/tokencamp-tools/issues/4) | CFG-KIMI-01..07、MOD-04、SMOKE-02、COMPAT-03 |
| [Codex 兼容性与 tool loop 可行性](https://github.com/0xnicholas/tokencamp-tools/issues/5) | NEG-01、NEG-03、DOC-05 |
| [登录、key 存储与校验](https://github.com/0xnicholas/tokencamp-tools/issues/6) | AUTH-01..18、URL-01..04、CFG-C-08、CFG-C-12、CLI-05、CLI-13、NFR-04、DOC-05 |
| [配置所有权与安全策略](https://github.com/0xnicholas/tokencamp-tools/issues/7) | CFG-C-01..14、CFG-CL-01..05、CFG-PI-01..05、CFG-KIMI-01..05、CLI-13、DOC-03 |
| [CLI 命令树与终端体验](https://github.com/0xnicholas/tokencamp-tools/issues/8) | CLI-01..14、CFG-CL-06、CFG-PI-07、CFG-KIMI-06、NEG-02、DOC-02 |
| [打包、分发与发布](https://github.com/0xnicholas/tokencamp-tools/issues/9) | PKG-01..08、NFR-02、DOC-01、DOC-05、NEG-04 |
| [领域词汇与适配器术语](https://github.com/0xnicholas/tokencamp-tools/issues/10) | DOC-03、DOC-05、DOC-07、CLI-14 |
| [模型映射与默认值](https://github.com/0xnicholas/tokencamp-tools/issues/11) | MOD-01..06、CFG-CL-02、CFG-PI-03、CFG-KIMI-02、DOC-04、DOC-05 |
| [Doctor 与验证契约](https://github.com/0xnicholas/tokencamp-tools/issues/12) | DR-01..12、CFG-C-12、CFG-CL-05、CFG-CL-07、CFG-PI-06、CFG-KIMI-04 |
| [Pi 适配器契约](https://github.com/0xnicholas/tokencamp-tools/issues/14) | CFG-PI-01..08、MOD-03、SMOKE-01、COMPAT-02 |
| [确认 Anthropic deployment（Claude Code 前置）](https://github.com/0xnicholas/tokencamp-tools/issues/15) | CFG-CL-06、CFG-CL-07、SMOKE-03、COMPAT-01、DOC-04、DOC-05 |
| [Codex 的 v1 支持设计](https://github.com/0xnicholas/tokencamp-tools/issues/16) | NEG-01、DOC-05 |
| [Tokencamp 工具包 · 第一件工具：tcamp harness 配置器](https://github.com/0xnicholas/tokencamp-tools/issues/1) | NFR-03、NEG-03、NEG-04、DOC-06 |

### 9. Deferred / 明确不做

- **Claude live smoke**：deferred；触发条件 = ops 添加 anthropic deployment；owner = 发布经理/ops。未补测不阻塞 v1。
- **`SPEC.md`**：由实现 effort 第一步汇编；不是 wayfinder ticket，不阻塞 map 关闭。
- **T3 nightly**：可选，不设发布门槛。
- 其余边界沿用 map 的 **Out of scope**：不改 tokencamp-pro、不做代理/网关、不接 keyring、不做项目级配置、不支持 Windows/OpenCode/Codex、不做 GUI/用量/遥测。
- `RELEASE-CHECKLIST.md` 由实现 effort 创建，列：`ID | Date | OS/arch | Harness version | Key (masked) | Result | Request ID | Notes`。

