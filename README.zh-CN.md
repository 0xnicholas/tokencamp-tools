# tcamp

`tcamp` 是 tokencamp-tools Toolkit 中的第一件 Tool：一个单二进制 CLI，按
Harness 把它配置到 tokencamp，然后 Launch 它。v0.1 支持 Claude Code、Pi
和 Kimi Code。

[English](./README.md) | 简体中文

CLI 输出保持英文。设计见 [SPEC.md](./SPEC.md)，术语见
[GLOSSARY.md](./GLOSSARY.md)。

## 安装

`tcamp` 支持 macOS 11 或更新版本（arm64 与 x86_64），以及 Linux（x86_64
与 arm64，静态 musl 构建）。Windows 不受支持。

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | sh
```

用 `TCAMP_VERSION` 或 `--version` 锁定版本：

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | TCAMP_VERSION=v0.1.0 sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | sh -s -- --version v0.1.0
```

安装到 `$HOME/.local/bin` 以外的目录时，为安装器设置
`TCAMP_INSTALL_DIR`：

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | TCAMP_INSTALL_DIR="$HOME/bin" sh
```

`install.sh` 会下载对应归档和 `SHA256SUMS`，在替换已有安装前校验 SHA-256
校验和，然后安装到 `${TCAMP_INSTALL_DIR:-$HOME/.local/bin}`。如果该目录不在
`PATH` 中，安装器会打印需要添加的 `export PATH="..."` 行；它不会修改 shell
启动文件，不使用 `sudo`，不配置任何 Harness，也不会自动移除 quarantine。

手动检查下载：

```sh
curl -fsSLO https://github.com/0xnicholas/tokencamp-tools/releases/download/v0.1.0/tcamp-0.1.0-aarch64-apple-darwin.tar.gz
curl -fsSLO https://github.com/0xnicholas/tokencamp-tools/releases/download/v0.1.0/SHA256SUMS
grep 'tcamp-0.1.0-aarch64-apple-darwin.tar.gz$' SHA256SUMS | sha256sum -c -       # Linux
grep 'tcamp-0.1.0-aarch64-apple-darwin.tar.gz$' SHA256SUMS | shasum -a 256 -c -   # macOS
```

每次发布包含四个归档、每个归档一个 `.sha256`、合并的 `SHA256SUMS`，以及
`install.sh`。

v0.1 发布不签名、不公证。通过浏览器下载的二进制可能被 Gatekeeper 拦截。
请右键点击二进制并选择“打开”确认；或者显式移除 quarantine 属性：

```sh
xattr -d com.apple.quarantine "$(command -v tcamp)"
```

通过 `curl` 下载、再由 `install.sh` 安装的二进制不带 quarantine 属性，因此
`install.sh` 不会自动移除 quarantine。上面的命令只适用于浏览器下载的场景。

## 快速开始

```sh
tcamp login
tcamp claude
tcamp pi
tcamp kimi
```

`tcamp login` 把已有 API key 保存为 Credential。在 TTY 中它会隐藏输入并运行
免费的 Auth probe；在脚本中使用 `--key-stdin`，或为 Login 进程设置
`TOKENCAMP_API_KEY`。随后 `tcamp claude`、`tcamp pi`、`tcamp kimi` 会
Configure 选中的 Harness 并 Launch 它。

首次运行某个 Harness 时，如果 stdin 与 stdout 都是 TTY 且没有存过 API key，
`tcamp` 会引导你隐藏输入 key、运行免费 Auth probe，并在写入 Config target
前询问 `Write it? [Y/n]`，确认后 Launch Harness。非 TTY 且没有 key 时以
exit 2 结束，报 `missing_key`，并提示使用 `tcamp login --key-stdin` 或
`TOKENCAMP_API_KEY`。

`--no-launch` 只 Configure，不 Launch Harness；即使 Harness 二进制未安装也能
写入 Config target。`--dry-run` 只打印待写入的 diff。`--json` 用于非 Launch
命令，请与 `--no-launch` 搭配。CLI 输出保持英文。

## 配置

`tcamp` 只写用户级 Config target。它不写 shell 启动文件、不生成中间启动脚本，
v1 也不管理项目级配置。

| Harness | Config target | Launch |
| --- | --- | --- |
| Claude Code | Overlay：`$XDG_CONFIG_HOME/tokencamp/harnesses/claude/settings.json`（回退 `~/.config/tokencamp/harnesses/claude/settings.json`） | `claude --settings <overlay>` |
| Pi | `$PI_CODING_AGENT_DIR/models.json`（回退 `~/.pi/agent/models.json`） | `pi --provider tokencamp --model <id>` |
| Kimi Code | `$KIMI_CODE_HOME/config.toml`（回退 `~/.kimi-code/config.toml`） | `kimi -m <alias>` |

### Claude Code

`tcamp claude` 拥有整个 Overlay 文件，并用 `--settings <overlay>` Launch
Claude Code。它绝不修改 `$CLAUDE_CONFIG_DIR/settings.json`（默认是
`~/.claude/settings.json`）。Overlay 包含 Base URL 与 Claude Code Slot 的
Model mapping；`ANTHROPIC_AUTH_TOKEN` 只在 Launch 时注入，不写进文件。
Overlay 带有用于识别所有权的 `_tcamp` 标记。

### Pi

`tcamp pi` 在 `models.json` 中 upsert `providers.tokencamp`，并暴露五个
Catalog model。Provider entry 使用 `apiKey: "$TOKENCAMP_API_KEY"`，因此文件
中不保存 API key 值。已有未知字段、注释和格式都会保留。只有
`defaultProvider` 或 `defaultModel` 缺失或为空时才补默认值。

### Kimi Code

`tcamp kimi` 在 `config.toml` 中 upsert `[providers.tokencamp]`，写入
`type = "openai"`、`base_url = "<Base URL>/v1"` 和
`api_key_env = "TOKENCAMP_API_KEY"`，并写入 `[models."tokencamp/*"]` 别名。
已有未知字段和已有的 `default_model` 值都会保留。

### 所有权、Drift 与 Conflict

Ownership manifest 保存在 `$XDG_CONFIG_HOME/tokencamp/managed.toml`，权限为
`0600`。它记录每个 Config target、`tcamp` 创建或 Adopt 的 Managed entry、
last-written hash、model id、默认值和备份路径，绝不保存 API key。

修改 Config target 前，`tcamp` 会在
`$XDG_STATE_HOME/tokencamp/backups/<harness>/`（回退
`~/.local/state/tokencamp/backups/<harness>/`）写入带时间戳的备份，权限
`0600`，每个 target 保留最近三份。用户修改 Managed entry 时，`tcamp` 报告
Drift、给出警告，并再次收敛 Managed entry。目标位置已有 `tcamp` 不拥有的
条目时，这是 Conflict：命令以 exit 2 结束并说明修法。`--force` 会先备份再
Adopt 冲突条目；它不跳过解析、校验、原子写或备份。`--yes` 只回答 TTY 确认，
不能替代 `--force`。

### 凭据与模型

Credential 文件是 `$XDG_CONFIG_HOME/tokencamp/credentials.toml`（回退
`~/.config/tokencamp/credentials.toml`），权限 `0600`，目录权限 `0700`。
每个 Base URL 对应一个 Connection，Login 会把该 Connection 设为
Active connection。`tcamp` 不写入 API key 到任何 Config target。
`TOKENCAMP_API_KEY` 在当前进程中覆盖已保存的 Credential。`--model` 和
`TOKENCAMP_MODEL` 只覆盖本次 Launch 的主模型，不修改 Config target 中的
静态 Model mapping。

## 故障排查

### 没有 API key

运行 `tcamp login`、设置 `TOKENCAMP_API_KEY`，或用
`tcamp login --key-stdin` 从标准输入传入 key。非 TTY 下运行 Harness 命令且
没有 key 时，以 exit 2 结束并提示这些选项。

### 401 Unauthorized

API key 无效或已过期。官方云请在 <https://tokencamp.com/keys> 创建或替换
key；自托管 Base URL 请使用其 Portal。用 `tcamp login --force` 替换已保存的
Credential，然后重启正在运行的 Harness 以使用新 key。`tcamp logout` 只删除
本地状态，不会在服务端吊销 key。

### 402 Payment Required

账户或 workspace 的预算已用尽。官方云请在 <https://tokencamp.com> 检查账单
与用量；自托管 Base URL 请检查 workspace budget。402 不阻断 Configure 或
Launch，只有 `--verify`、`tcamp doctor --network` 或 Harness 真实请求才能看到。

### 网络问题

运行 `tcamp doctor --network` 检查连通性和 Auth probe。请求使用 HTTPS；
明文 HTTP 只允许 loopback 和私有网络。超时和 5xx 响应会给出重试提示；保存
key 时也可以使用 `--no-verify`。

### Harness 未安装或版本过低

版本地板是 Claude Code `2.1.304`、Pi `1.4.0`、Kimi Code `2.1.1`。Harness
未安装时，普通 Launch 以 exit 2 结束；`--no-launch` 仍会写入 Config target。
版本低于地板只产生警告，Launch 继续。请用 Harness 自己的安装方式安装或升级。

### 配置损坏

Config target 无效时，`tcamp` 打印路径和解析位置，以 exit 2 结束且不写入。
请修复文件、从 Ownership manifest 记录的位置恢复备份，或运行 `tcamp doctor`
定位问题。`--force` 不会绕过解析或校验。

### Claude Code 404 或 anthropic deployment

Claude Code 使用 `/v1/messages`，该端点要求 `provider_type: anthropic` 的
Deployment。没有该 Deployment 时，`tcamp claude` 是 config-only：它仍会配置
Overlay，警告该能力未验证，但不阻断 Launch。运行
`tcamp doctor --harness claude` 查看默认警告，或运行
`tcamp doctor --verify --harness claude` 发送一次 `/v1/messages` 请求。
`model_not_found` 404 指向缺失的 anthropic deployment。可以用
`tcamp doctor --verify --harness pi` 通过 chat completions 验证 key。

## 安全

Credential 保存在 `credentials.toml`，权限 `0600`；目录权限 `0700`；写入是
原子的，并拒绝 symlink。输出中的 API key 会打码，完整 key 永远不会出现在
日志、错误、diff、JSON 或备份中。

API key 只在 Launch 时注入 Harness 环境，注入内容不写入 Config target。
运行中的 Harness 会保留启动时收到的 key，直到重启。通过 tokencamp Portal
吊销 key 会立即对新请求生效。环境变量对同一用户的其他进程可见，这是 v1
接受的边界，不用于防御同用户的恶意进程。

发布归档通过 HTTPS 下载，并用 `SHA256SUMS` 校验；发布带有 build provenance
和不可变 release 资产。v0.1 发布不签名、不公证。`tcamp` 不做遥测、分析或
更新检查。

## 已知限制

- v1 不支持 Codex，只有设计工作。
- Claude live Smoke 延后；除非 tokencamp deployment 包含
  `provider_type: anthropic`，否则 Claude Code 是 config-only。
- 不支持 Windows。
- 没有 OS keyring 支持，不支持运行中热轮换，不支持项目级配置。
- Model mapping 是静态 curated 表；新模型随新的 `tcamp` 发布提供。
- Claude Code 中 thinking 被关闭，prompt cache 行为退化，tool search 被关闭。

## 卸载

```sh
tcamp uninstall
```

`tcamp uninstall` 只删除 `tcamp` 创建且仍等于 last-written hash 的 Managed
entry。你修改过的条目会保留并给出警告；`--force` 才会删除。`--dry-run`
只打印计划，`--yes` 用于非交互确认，`--harness <name>` 限定范围。

卸载会保留 Credential、备份和 `tcamp` 二进制，并打印保留路径与删除二进制的
确切命令：

```sh
rm "$(command -v tcamp)"
```

用 `tcamp logout --all` 删除 Credential。`install.sh` 没有 `--uninstall`，
v1 也不提供 updater。不再需要的备份请手动删除。

## 许可证

Apache-2.0。见 [LICENSE](./LICENSE) 与 [SECURITY.md](./SECURITY.md)。
