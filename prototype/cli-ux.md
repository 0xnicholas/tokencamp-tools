# tcamp CLI 命令树与终端体验 — 原型

> **PROTOTYPE** · 为 [CLI 命令树与终端体验](https://github.com/0xnicholas/tokencamp-tools/issues/8) 的 HITL 讨论而做，不是规格，不进 `main`。
>
> 形态假设：CLI 的“界面”就是终端 transcript，所以这里用**三份结构不同的 transcript**代替 Web 变体。命令树与机器契约三份共用，只改信息层级、交互和默认动作。版本号、路径、模型名均为示意。
>
> 看完请在 ticket 里指一个变体（或指认“A 的日常 + B 的首次”这类组合），并回答第 8 节的问题。收敛后结论写回 ticket，本文件留在 `prototype/cli-ux` 分支。

## 要回答的问题

`tcamp` 的命令树和终端体验应该是什么样：首次运行怎么把人带进来，日常运行有多吵，失败时给什么下一步，哪些信息进 `--json`，退出码怎么分。

## 1. 已锁定，本原型不重开

- `tcamp <harness>` 收敛配置后启动；`--no-launch` 只收敛。harness：`claude` / `pi` / `kimi`。
- 登录/凭据按 [登录、key 存储与校验](https://github.com/0xnicholas/tokencamp-tools/issues/6)：默认免费 auth probe；`--key-stdin` / `--no-verify` / `--verify` / `--force`；401 不保存；掩码 `sk-…` + 后 4 位；一个 base URL 一把 key。
- 配置按 [配置所有权与安全策略](https://github.com/0xnicholas/tokencamp-tools/issues/7)：用户级、只增不改；Claude 用 tcamp-owned overlay + `--settings`；Pi/Kimi 只 upsert `tokencamp` provider/model；`--dry-run` / `--force` / `--yes` 语义已定；遮蔽检测不过不启动。
- 密钥只在启动时注入；Claude 用 `ANTHROPIC_AUTH_TOKEN` 并清空 `ANTHROPIC_API_KEY`。
- 不写 shell wrapper、不做项目级配置、不做浏览器登录、keyring、热轮换、Windows。
- 具体模型映射归 [模型映射与默认值](https://github.com/0xnicholas/tokencamp-tools/issues/11)；doctor 检查项归 [Doctor 与验证契约](https://github.com/0xnicholas/tokencamp-tools/issues/12)。本原型只定它们露在 CLI 上的**形状**。

## 2. 命令树（三变体共用）

```
tcamp <command> [flags] [-- <harness args>]

  tcamp claude [flags]        configure Claude Code, then exec claude
  tcamp pi [flags]            configure Pi, then exec pi
  tcamp kimi [flags]          configure Kimi Code, then exec kimi
  tcamp login [flags]         store and verify an API key
  tcamp logout [flags]        remove local credentials
  tcamp status [flags]        show the active connection and harness state
  tcamp doctor [flags]        check the local setup and connectivity
  tcamp uninstall [flags]     remove tcamp-written harness configuration
  tcamp help [command]
```

harness 子命令：`--no-launch`、`--dry-run`、`--model <id>`、`--base-url <url>`、`--force`、`--yes`、`--json`、`--no-color`。`--` 之后原样透传给 harness（见 Q7）。

`login`：`--key-stdin`、`--no-verify`、`--verify`、`--force`、`--base-url`、`--json`。
`logout`：`--all`、`--base-url`、`--json`。
`status` / `doctor`：`--json`；`doctor` 另有 `--network`、`--harness <name>`。
`uninstall`：`--harness <name>`（可重复）、`--dry-run`、`--yes`、`--force`、`--json`。
全局：`-h/--help`、`-V/--version`、`-v/--verbose`。

## 3. 共享契约（三变体一致）

### 3.1 输出通道

- `--json`：stdout 恰好一个 JSON 对象；进度/警告只走 stderr。
- 人类模式：进度、提示、确认走 stderr；结果（保存行、`status`、`--dry-run` diff）走 stdout，方便管道。
- 颜色只在 TTY；尊重 `NO_COLOR`、`--no-color`、`TERM=dumb`。不用光标把戏和转圈动画，CI 日志可读。
- `--json` 与“启动”互斥：`tcamp claude --json` 是用法错误（exit 2），因为 exec 之后 stdout 归 harness。要 JSON 就加 `--no-launch`。

### 3.2 退出码

| 码 | 含义 | 例 |
| --- | --- | --- |
| `0` | 成功；若启动，exec 之后退出码归 harness | 配置写入、`login` 成功、`status` |
| `1` | 运行期失败 | 网络不可达、401、5xx、写一半失败 |
| `2` | 需要用户先改点什么，重试才有意义 | 用法错误、没有 key、harness 未安装、配置冲突/损坏、非 TTY 缺确认 |

配置成功并通过 `exec` 启动后，tcamp 进程被 harness 替换：信号、退出码、Ctrl-C 都归 harness，tcamp 不再包装。tcamp 自己的失败只发生在 exec 之前。

### 3.3 `--json`

成功（`tcamp status --json` 为例）：

```json
{
  "ok": true,
  "base_url": "https://api.tokencamp.com",
  "key": { "masked": "sk-…9f2c", "source": "file", "verified_at": "2026-11-01T10:00:01Z" },
  "harnesses": {
    "claude": { "installed": "2.1.316", "configured": true, "target": "/Users/n/.config/tokencamp/harnesses/claude/settings.json" },
    "pi": { "installed": "1.4.0", "configured": false },
    "kimi": { "installed": null, "configured": false }
  }
}
```

失败：

```json
{
  "ok": false,
  "error": {
    "code": "authentication",
    "message": "API key is invalid or expired",
    "hint": "create or copy a key at https://tokencamp.com/keys",
    "request_id": "req_01JC..."
  }
}
```

`code` 有服务器值时沿用 API code（`authentication`、`insufficient_balance`…），本地错误用固定词：`missing_key`、`harness_not_found`、`config_conflict`、`config_unreadable`、`network`、`usage`。完整 key 永不出现在 JSON、日志、错误里。

### 3.4 错误文案

人类模式默认两到三行：`error:` 说原因，`hint:` 说下一步。服务器给了 `code` 就标在 error 行；`request_id` 默认不进人类输出，`--json` 和 `-v` 里有（见 Q6）。

```
error: API key is invalid or expired [authentication]
hint: create or copy a key at https://tokencamp.com/keys, then run `tcamp login` again
```

### 3.5 `--dry-run`

只读，不建目录、不拿锁、不写备份；diff 走 stdout，secret 打码；解析失败 exit 2，否则 exit 0。

```
$ tcamp claude --no-launch --dry-run
--- /Users/n/.config/tokencamp/harnesses/claude/settings.json (new)
+++ /Users/n/.config/tokencamp/harnesses/claude/settings.json (tcamp)
@@
+{
+  "$schema": "https://json.schemastore.org/claude-code-settings.json",
+  "_tcamp": { "version": 1, "harness": "claude" },
+  "env": {
+    "ANTHROPIC_BASE_URL": "https://api.tokencamp.com",
+    "ANTHROPIC_API_KEY": "",
+    "ANTHROPIC_MODEL": "claude-sonnet-4-5",
+    "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-opus-4-1",
+    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-4-5",
+    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "claude-haiku-4-5",
+    "ENABLE_TOOL_SEARCH": "false",
+    "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS": "1",
+    "MAX_THINKING_TOKENS": "0"
+  }
+}
no files written
```

（`ANTHROPIC_AUTH_TOKEN` 不在文件里，启动时注入；示例值由 [模型映射与默认值](https://github.com/0xnicholas/tokencamp-tools/issues/11) 最终决定。）

### 3.6 启动是 exec

配置成功后 `exec` harness，tcamp 让位。exec 前最多一行 `Launching <harness> ...` 到 stderr。这样 Ctrl-C、信号、退出码、`--` 透传参数都按 harness 的原生语义走。

### 3.7 `logout`（文案由 #6 定）

```
$ tcamp logout
Removed sk-…9f2c for https://api.tokencamp.com from ~/.config/tokencamp/credentials.toml
Running harnesses still hold the old key until restarted. To revoke it now: https://tokencamp.com/keys
```

### 3.8 402 / 余额不足

默认 auth probe 是免费的，查不到余额（#6 已定）。只有 `--verify`、`doctor --network`、或 harness 真实请求才会看到 402。三变体的框不同，内容一致：

```
$ tcamp claude --verify
Verifying key with https://api.tokencamp.com ... 402 insufficient_balance
warning: key is valid, but the account has no credit [insufficient_balance]
hint: top up at https://tokencamp.com/billing (self-hosted: check the workspace budget)
Configured claude: /Users/n/.config/tokencamp/harnesses/claude/settings.json
Launching claude ...
```

建议：402 只警告、不阻断配置和启动（账户状态 tcamp 修不了，harness 会再次报同一个错）；是否改成“启动前硬停”见 Q4。

## 4. 变体 A — Quiet Unix

**气质**：`gh` / `cargo`。成功少说话；没有变化就不打印配置行；失败两行，`error:` + `hint:`；不做向导。适合老手、脚本、CI。

```
$ tcamp
tcamp 0.1.0 — configure AI harnesses for tokencamp

Usage: tcamp <command> [flags]

Commands:
  claude     configure Claude Code, then launch it
  pi         configure Pi, then launch it
  kimi       configure Kimi Code, then launch it
  login      store and verify a tokencamp API key
  logout     remove stored credentials from this machine
  status     show the active connection
  doctor     check the local setup and connectivity
  uninstall  remove tcamp-written harness configuration

Detected: claude 2.1.316, pi 1.4.0
Get started: tcamp login, then tcamp claude
```

首次没有 key 就跑 `tcamp claude`：

```
$ tcamp claude
error: no API key for https://api.tokencamp.com [missing_key]
hint: run `tcamp login` (or set TOKENCAMP_API_KEY for this command)
# exit 2
```

登录（成功）：

```
$ tcamp login
API key: ********
Verifying key with https://api.tokencamp.com ... ok
Saved and verified sk-…9f2c (https://api.tokencamp.com)
```

登录（401，TTY 允许重试一次）：

```
$ tcamp login
API key: ********
Verifying key with https://api.tokencamp.com ... failed (401 authentication)
error: API key is invalid or expired [authentication]
hint: create or copy a key at https://tokencamp.com/keys, then run `tcamp login` again
Try another key? [y/N] n
# exit 1
```

第一次启动，发生了写入：

```
$ tcamp claude
Configured claude: /Users/n/.config/tokencamp/harnesses/claude/settings.json
Launching claude ...
# exec claude
```

第二次启动，没有变化：

```
$ tcamp claude
Launching claude ...
# exec claude
```

harness 未安装：

```
$ tcamp claude
error: `claude` not found on PATH [harness_not_found]
hint: install Claude Code, or write the config now with `tcamp claude --no-launch`
# exit 2
```

遮蔽冲突：

```
$ tcamp claude
error: ANTHROPIC_AUTH_TOKEN in /Users/n/.claude/settings.json would shadow the tcamp key [config_conflict]
hint: remove that entry, then retry; `tcamp doctor` lists every source checked
# exit 2
```

`status`：

```
$ tcamp status
base URL  https://api.tokencamp.com
key       sk-…9f2c (file, verified 2026-11-01T10:00:01Z)
claude    configured (overlay)
pi        not configured
kimi      not installed
```

`uninstall`：

```
$ tcamp uninstall
About to remove tcamp-written configuration:
  claude  overlay /Users/n/.config/tokencamp/harnesses/claude/settings.json
  pi      provider "tokencamp" in /Users/n/.pi/agent/models.json
  kimi    nothing to remove

Continue? [y/N] y
claude  removed (backup: /Users/n/.local/state/tokencamp/backups/claude-20261101T101500Z.json)
pi      removed
Credentials kept: /Users/n/.config/tokencamp/credentials.toml
Binary not removed: rm "$(command -v tcamp)"
```

## 5. 变体 B — Guided

**气质**：`flyctl` / `vercel`。首次运行是一段短向导：解释发生了什么、确认写入、然后启动；日常退回短行。适合第一次用、演示、给不熟 CLI 的人。

```
$ tcamp
tcamp 0.1.0

Claude Code 2.1.316 and Pi 1.4.0 are installed.
No tokencamp API key is stored yet.

Get started in two steps:
  1. tcamp login     paste and verify your API key
  2. tcamp claude    configure Claude Code and launch it

Run `tcamp status` any time to see what's set up.
```

首次没有 key 就跑 `tcamp claude`：

```
$ tcamp claude
Claude Code is installed (2.1.316).
No API key stored for https://api.tokencamp.com.

Let's store one first.

Paste your tokencamp API key (input is hidden): ********
Checking the key ... valid (sk-…9f2c)
Saved to /Users/n/.config/tokencamp/credentials.toml.

Configure Claude Code?
  tcamp writes its own overlay and leaves ~/.claude/settings.json untouched.
  → /Users/n/.config/tokencamp/harnesses/claude/settings.json
Write it? [Y/n] y
  ✓ overlay written

Launching claude ...
```

登录（成功）：

```
$ tcamp login
Paste your tokencamp API key (input is hidden; it starts with sk-).
API key: ********
Checking the key ... valid
Saved and verified sk-…9f2c for https://api.tokencamp.com.

Next: tcamp claude
```

登录（401）：

```
$ tcamp login
API key: ********
Checking the key ... rejected (401 authentication)

That key is invalid or expired.
Copy it again from https://tokencamp.com/keys — keys are shown only once,
so you may need to create a new one.

Try another key? [Y/n] y
API key: ********
Checking the key ... valid
Saved and verified sk-…9f2c.
# exit 0
```

日常启动，没有变化 / 有变化：

```
$ tcamp claude
✓ claude overlay up to date
→ launching claude
# exec claude
```

```
$ tcamp claude
✓ claude overlay updated: sonnet → claude-sonnet-4-5
→ launching claude
```

harness 未安装：

```
$ tcamp claude
Claude Code isn't installed, or isn't on PATH.

Install it first:
  https://docs.anthropic.com/en/docs/claude-code

Then run `tcamp claude` again.
To write the configuration now, run `tcamp claude --no-launch`.
# exit 2
```

遮蔽冲突：

```
$ tcamp claude
Claude Code already sends its own token:
  /Users/n/.claude/settings.json → env.ANTHROPIC_AUTH_TOKEN

tcamp can't inject its key while that entry exists — Claude Code would keep
using the other token. Nothing was written.

Remove that entry, then run `tcamp claude` again.
Run `tcamp doctor` to see every source it checks.
# exit 2
```

`status`：

```
$ tcamp status
Connection
  Base URL   https://api.tokencamp.com
  Key        sk-…9f2c (stored on this machine)
  Verified   2026-11-01 10:00 UTC

Harnesses
  Claude Code  configured (overlay)
  Pi           not configured
  Kimi Code    not installed

Next: tcamp claude
```

`uninstall`：

```
$ tcamp uninstall
tcamp will remove its own configuration. Credentials and the binary stay.

  Claude Code   remove overlay /Users/n/.config/tokencamp/harnesses/claude/settings.json
  Pi            remove provider "tokencamp" from /Users/n/.pi/agent/models.json
  Kimi Code     nothing to remove

Continue? [y/N] y
  ✓ Claude Code overlay removed (backup kept)
  ✓ Pi provider removed

Your API key is still stored at /Users/n/.config/tokencamp/credentials.toml.
To remove it: tcamp logout --all
To remove the binary: rm "$(command -v tcamp)"
```

## 6. 变体 C — State board

**气质**：`wrangler`。每次动作先摆出它要使用的状态，再动手；缺 key 就在原地补，不把人赶去另一条命令。信息密度最高，适合同时配多个 harness、需要看清状态的人。

```
$ tcamp
tcamp 0.1.0

  base URL   https://api.tokencamp.com
  key        missing
  claude     installed 2.1.316 · not configured
  pi         installed 1.4.0 · not configured
  kimi       not installed

  tcamp login    store an API key
  tcamp claude   configure and launch
```

首次没有 key 就跑 `tcamp claude`：

```
$ tcamp claude
  base URL   https://api.tokencamp.com
  key        missing
  claude     installed 2.1.316 · not configured

No API key found. Paste one now (input hidden), or Ctrl-C and run `tcamp login`.
API key: ********
  key        sk-…9f2c · verified

  config     overlay written (1 file)
Launching claude ...
```

登录（成功 / 401）：

```
$ tcamp login
  base URL   https://api.tokencamp.com
  key        missing

API key: ********
  key        sk-…9f2c · verified
  stored     /Users/n/.config/tokencamp/credentials.toml (0600)
```

```
$ tcamp login
  key        missing

API key: ********
  key        invalid · 401 authentication

Nothing was saved. Next:
  - copy the key again from https://tokencamp.com/keys
  - keys are shown once; create a new one if needed
  - retry: tcamp login

Try again? [y/N] n
# exit 1
```

日常启动：

```
$ tcamp claude
  base URL   https://api.tokencamp.com
  key        sk-…9f2c · verified 2h ago
  model      sonnet → claude-sonnet-4-5
  config     overlay up to date

Launching claude ...
```

harness 未安装：

```
$ tcamp claude
  claude     not installed

Nothing to configure or launch. Claude Code isn't on PATH.
  install      https://docs.anthropic.com/en/docs/claude-code
  config only  tcamp claude --no-launch   (writes the overlay without the binary)
# exit 2
```

遮蔽冲突：

```
$ tcamp claude
  claude     installed 2.1.316 · not configured
  conflict   /Users/n/.claude/settings.json sets env.ANTHROPIC_AUTH_TOKEN

Stopped before writing or launching: that token would shadow the tcamp key.
  fix        remove env.ANTHROPIC_AUTH_TOKEN from /Users/n/.claude/settings.json
  inspect    tcamp doctor
# exit 2
```

`status`：

```
$ tcamp status
tcamp · active connection
  base URL   https://api.tokencamp.com
  key        sk-…9f2c · file · verified 2026-11-01T10:00:01Z
  stored     /Users/n/.config/tokencamp/credentials.toml

harnesses
  claude     installed 2.1.316 · overlay configured · sonnet → claude-sonnet-4-5
  pi         installed 1.4.0 · provider configured · auth.json clean
  kimi       not installed · not configured

state        /Users/n/.local/state/tokencamp (3 backups)
```

`uninstall`：

```
$ tcamp uninstall
tcamp · uninstall
  claude     remove overlay  /Users/n/.config/tokencamp/harnesses/claude/settings.json
  pi         remove provider "tokencamp"  /Users/n/.pi/agent/models.json
  kimi       nothing to remove

  credentials   kept  /Users/n/.config/tokencamp/credentials.toml
  binary        kept  /usr/local/bin/tcamp

Continue? [y/N] y
  claude     removed · backup /Users/n/.local/state/tokencamp/backups/claude-20261101T101500Z.json
  pi         removed
```

## 7. 对照

| 场景 | A Quiet Unix | B Guided | C State board |
| --- | --- | --- | --- |
| 裸跑 `tcamp` | 帮助 + 检测到的 harness | 两步欢迎 | 状态板 |
| 首次无 key 跑 `tcamp claude` | 报错 + hint，exit 2 | 向导：登录 → 确认写入 → 启动 | 板 + 内联输入 → 继续 |
| 日常启动（无变化） | 一行 `Launching` | `✓ up to date` + `→ launching` | 四行板 + `Launching` |
| 写配置前确认 | 只在冲突/覆盖时问 | 首次写入也问 | 不另问，向导内已给出 |
| 401 | error/hint + 重试一次 | 解释 + 重试一次 | 板 + 解释 + 重试一次 |
| harness 未安装 | error + 两条 hint | 安装链接 + `--no-launch` | 板 + 安装/config-only 两条路 |
| 遮蔽冲突 | error + hint | 解释 + 未写入声明 | 板里的 `conflict` 行 + fix |
| `status` | 六行键值 | 分组小节 | 板 |
| `uninstall` | 计划 + 确认 + 备份路径 | 计划 + 确认 + 后续提示 | 板 + 计划 + 确认 |
| 适合 | 老手、CI、脚本 | 首次用户、演示 | 多 harness、可观测性 |
| 主要代价 | 首次要读两条命令 | 日常多一行、文案更长 | 日常最吵；每次多一块板 |

## 8. 需要你拍板

**Q1 选哪个变体？** 推荐 **A 的日常 + B 的首次**：默认安静；仅当 TTY 且没有 key 时，`tcamp <harness>` 走一次 B 的引导（问“现在登录吗 / 写配置吗”），之后回到 A。C 的板只用于 `status` / `doctor`。备选：纯 A（最省事、首次最硬）、纯 B（最友好、日常最啰嗦）、纯 C（观测性最好、最吵）。

**Q2 CLI 输出用什么语言？** 推荐 **English**（命令、错误、`code` 可搜索，文档仍中文）；备选：中文文案（与文档一致），或中文句子 + 英文标识符。

**Q3 无 key 时 `tcamp <harness>` 默认做什么？** 推荐：TTY 内联提示一次（B/C），非 TTY 直接报错 exit 2 并提示 `tcamp login --key-stdin` 或 `TOKENCAMP_API_KEY`。纯 A 的“报错让人自己跑 login”只保留给 `--no-launch` 和 CI 路径。

**Q4 402 / 余额不足怎么处理？** 推荐：不新增付费预检（#6 已定 probe 免费）；只有 `--verify` / `doctor --network` / harness 真实请求会暴露 402；暴露时警告 + 给充值链接，不阻断配置和启动。备选：`--verify` 遇到 402 就停止启动（更安全，但账户状态不是 tcamp 能修的）。

**Q5 退出码用 0/1/2 + exec 透传？** 推荐采纳：`2` = 用户先改点东西（缺 key、缺 harness、冲突、用法），`1` = 运行期失败（网络、401、5xx），启动后归 harness。与 #6 的 login 矩阵一致（401=1，损坏=2）。

**Q6 错误里的 `code` / `request_id` 默认露多少？** 推荐：人类输出带 `code`；`request_id` 只进 `--json` 和 `-v`（默认不刷屏，`hint` 里提示用 `-v` 取）。备选：两者都默认打印，方便用户直接贴给支持。

**Q7 允许 `--` 透传 harness 参数吗？** 推荐允许：`tcamp claude -- --resume` 原样交给 claude；tcamp 不解析 `--` 后面的内容。没有它，`tcamp claude --resume` 这类原生参数就没法用。

**Q8 harness 未安装时 `--no-launch` 还写配置吗？** 推荐写（provisioning 场景有用），`doctor` 标 warn；`tcamp <harness>` 不带 `--no-launch` 时仍 exit 2。

**Q9 `status` / `uninstall` 的默认范围？** 推荐：`status` 显示三个 harness 各自的配置状态（读 manifest + 目标文件）；`uninstall` 默认处理全部 harness，先打印计划、TTY 确认、`--yes` 跳过、保留凭据、提示删二进制的命令。`--harness` 可只处理一个。

**Q10 `--model <id>` 的作用域？** 推荐只影响本次启动（映射到 harness 原生 model 参数 / 本次 env），不改写配置；具体槽位映射归 [模型映射与默认值](https://github.com/0xnicholas/tokencamp-tools/issues/11)。

**Q11 `tcamp login` 在覆盖已有 key 时的确认**已由 #6 定（TTY 确认，非 TTY `--force`），本 ticket 只需决定文案，见 Q1 选定后的变体措辞。

## 9. 收敛后怎么落

- 结论写进 [CLI 命令树与终端体验](https://github.com/0xnicholas/tokencamp-tools/issues/8) 的 resolution comment，并在 map 的 Decisions-so-far 加一行。
- 选中的 transcript 变成 spec 里的命令/输出契约；未选中的留在本分支，不进 `main`。
- 若选混合，spec 要分别写“TTY 首次 / 非 TTY / 日常”三条路径，并复用第 3 节的共享契约。
- 本 ticket 不落 doctor 的检查项（#12）、模型映射（#11）、打包（#9）、术语表（#10）。
