# tcamp

`tcamp` 是 tokencamp-tools Toolkit 中的第一件 Tool：一个单二进制 CLI，按
Harness 把它配置到 tokencamp，然后 Launch 它。v0.1 支持 Claude Code、Pi
和 Kimi Code。

[English](./README.md) | 简体中文

CLI 输出保持英文。设计见 [SPEC.md](./SPEC.md)，术语见
[GLOSSARY.md](./GLOSSARY.md)。

## 文档

- [开始](https://docs.tokencamp.com/tools/tcamp)
- [CLI 参考](https://docs.tokencamp.com/tools/tcamp/reference)
- [安全与限制](https://docs.tokencamp.com/tools/tcamp/security)

## 安装

`tcamp` 支持 macOS 11 或更新版本（arm64 与 x86_64），以及 Linux（x86_64
与 arm64，静态 musl 构建）。Windows 不受支持。

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | sh
```

用 `TCAMP_VERSION` 锁定 v0.1.0：

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | TCAMP_VERSION=v0.1.0 sh
```

v0.1 发布不签名、不公证。没有 updater；升级时重新运行安装器。

安装详情见 [Install](https://docs.tokencamp.com/tools/tcamp)。

## 快速开始

```sh
tcamp login
tcamp claude
tcamp pi
tcamp kimi
```

`tcamp login` 保存已有的 key。`tcamp claude`、`tcamp pi`、`tcamp kimi` 会
Configure 并 Launch 对应的 Harness。

登录与配置详情见 [Log in and Configure](https://docs.tokencamp.com/tools/tcamp)。

## 配置

`tcamp` 只写用户级 Config target。

| Harness | Config target | Launch |
| --- | --- | --- |
| Claude Code | Overlay：`$XDG_CONFIG_HOME/tokencamp/harnesses/claude/settings.json`（回退 `~/.config/tokencamp/harnesses/claude/settings.json`） | `claude --settings <overlay>` |
| Pi | `$PI_CODING_AGENT_DIR/models.json` 与 `settings.json`（回退 `~/.pi/agent/models.json` 与 `~/.pi/agent/settings.json`），默认值写入 `defaultProvider` 与 `defaultModel` | `pi --provider tokencamp --model <id>` |
| Kimi Code | `$KIMI_CODE_HOME/config.toml`（回退 `~/.kimi-code/config.toml`） | `kimi -m <alias>` |

详见 [Reference](https://docs.tokencamp.com/tools/tcamp/reference)。

## 安全

- Credential 以 `0600` 保存在 `0700` 目录中；写入是原子的，并拒绝 symlink。
  Config target 中原本存在的 literal key 会保留在 `0600` 的变更前备份中。
- API key 只在 Launch 时注入 Harness 环境，不写入任何 Config target。
- 运行中的 Harness 会保留启动时收到的 key，直到重启。通过 tokencamp Portal
  吊销 key 会立即对新请求生效。
- `tcamp` 不做遥测、分析或更新检查。

详见 [安全与限制](https://docs.tokencamp.com/tools/tcamp/security) 与
[SECURITY.md](./SECURITY.md)。

## 许可证

Apache-2.0。见 [LICENSE](./LICENSE) 与 [SECURITY.md](./SECURITY.md)。
