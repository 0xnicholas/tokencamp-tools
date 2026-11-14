# tcamp

`tcamp` is the first Tool in the tokencamp-tools Toolkit. It is a single
binary that configures a Harness to use tokencamp and then Launches it. v0.1
supports Claude Code, Pi, and Kimi Code.

English | [简体中文](./README.zh-CN.md)

The CLI prints English output. The design lives in [SPEC.md](./SPEC.md) and
the terminology lives in [GLOSSARY.md](./GLOSSARY.md).

## Documentation

- [Start](https://docs.tokencamp.com/tools/tcamp)
- [CLI reference](https://docs.tokencamp.com/tools/tcamp/reference)
- [Security and limitations](https://docs.tokencamp.com/tools/tcamp/security)

## Install

`tcamp` runs on macOS 11 or later (arm64 and x86_64) and on Linux (x86_64 and
arm64, static musl builds). Windows is not supported.

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | sh
```

Pin v0.1.0 with `TCAMP_VERSION`:

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | TCAMP_VERSION=v0.1.0 sh
```

v0.1 releases are not signed or notarized.
There is no updater; re-run the installer to upgrade.

See [Install](https://docs.tokencamp.com/tools/tcamp) for details.

## Quickstart

```sh
tcamp login
tcamp claude
tcamp pi
tcamp kimi
```

`tcamp login` stores an existing key. `tcamp claude`, `tcamp pi`, and
`tcamp kimi` Configure and Launch their Harness.

See [Log in and Configure](https://docs.tokencamp.com/tools/tcamp) for details.

## Configuration

`tcamp` writes only user-level Config targets.

| Harness | Config target | Launch |
| --- | --- | --- |
| Claude Code | Overlay at `$XDG_CONFIG_HOME/tokencamp/harnesses/claude/settings.json` (fallback `~/.config/tokencamp/harnesses/claude/settings.json`) | `claude --settings <overlay>` |
| Pi | `$PI_CODING_AGENT_DIR/models.json` and `settings.json` (fallback `~/.pi/agent/models.json` and `~/.pi/agent/settings.json`), with defaults in `defaultProvider` and `defaultModel` | `pi --provider tokencamp --model <id>` |
| Kimi Code | `$KIMI_CODE_HOME/config.toml` (fallback `~/.kimi-code/config.toml`) | `kimi -m <alias>` |

See [Reference](https://docs.tokencamp.com/tools/tcamp/reference) for details.

## Security

- The Credential is stored with mode `0600` in a `0700` directory; writes are
  atomic and refuse symlinks. A literal key already present in a Config
  target is kept in the `0600` pre-change backup.
- The API key is injected into the Harness environment at Launch only; it is
  never written into a Config target.
- A running Harness keeps the key it received until it restarts. Revocation
  through the tokencamp Portal takes effect immediately for new requests.
- `tcamp` performs no telemetry, analytics, or update checks.

See [Security and limitations](https://docs.tokencamp.com/tools/tcamp/security)
and [SECURITY.md](./SECURITY.md) for details.

## License

Apache-2.0. See [LICENSE](./LICENSE) and [SECURITY.md](./SECURITY.md).
