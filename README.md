# tcamp

`tcamp` is the first Tool in the tokencamp-tools Toolkit. It is a single
binary that configures a coding agent Harness to use tokencamp and then
Launches it. v0.1 supports Claude Code, Pi, and Kimi Code.

English | [简体中文](./README.zh-CN.md)

The CLI prints English output. The design lives in [SPEC.md](./SPEC.md) and
the terminology lives in [GLOSSARY.md](./GLOSSARY.md).

## Install

`tcamp` runs on macOS 11 or later (arm64 and x86_64) and on Linux (x86_64 and
arm64, static musl builds). Windows is not supported.

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | sh
```

Pin a specific release with `TCAMP_VERSION` or the `--version` option:

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | TCAMP_VERSION=v0.1.0 sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | sh -s -- --version v0.1.0
```

To install somewhere other than `$HOME/.local/bin`, set `TCAMP_INSTALL_DIR`
for the installer:

```sh
curl -fsSL https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | TCAMP_INSTALL_DIR="$HOME/bin" sh
```

`install.sh` downloads the matching archive and `SHA256SUMS`, verifies the
SHA-256 checksum before touching an existing install, and installs to
`${TCAMP_INSTALL_DIR:-$HOME/.local/bin}`. If that directory is not in `PATH`,
the installer prints the exact `export PATH="..."` line to add; it does not
edit your shell startup files. It never uses `sudo`, never configures a
Harness, and does not remove quarantine automatically.

To verify a download manually:

```sh
curl -fsSLO https://github.com/0xnicholas/tokencamp-tools/releases/download/v0.1.0/tcamp-0.1.0-aarch64-apple-darwin.tar.gz
curl -fsSLO https://github.com/0xnicholas/tokencamp-tools/releases/download/v0.1.0/SHA256SUMS
grep 'tcamp-0.1.0-aarch64-apple-darwin.tar.gz$' SHA256SUMS | sha256sum -c -       # Linux
grep 'tcamp-0.1.0-aarch64-apple-darwin.tar.gz$' SHA256SUMS | shasum -a 256 -c -   # macOS
```

Every release contains four archives, one `.sha256` file per archive, a merged
`SHA256SUMS`, and `install.sh`.

v0.1 releases are not signed or notarized. A binary downloaded through a
browser can therefore be blocked by Gatekeeper. Right-click the binary, choose
**Open**, and confirm; or remove the quarantine attribute explicitly:

```sh
xattr -d com.apple.quarantine "$(command -v tcamp)"
```

Downloads made with `curl` and installs made with `install.sh` do not carry the
quarantine attribute, so `install.sh` does not remove quarantine. The command
above is only for a binary you downloaded through a browser.

## Quickstart

```sh
tcamp login
tcamp claude
tcamp pi
tcamp kimi
```

`tcamp login` stores an existing API key as a Credential. On a TTY it prompts
without echoing the key and runs a free Auth probe; in scripts, use
`--key-stdin` or set `TOKENCAMP_API_KEY` for the login process. Then
`tcamp claude`, `tcamp pi`, and `tcamp kimi` Configure the selected Harness
and Launch it.

On the first run of a Harness, when stdin and stdout are a TTY and no API key
is stored, `tcamp` guides you through hidden key entry, the free Auth probe,
and a `Write it? [Y/n]` confirmation before it writes the Config target and
Launches the Harness. A non-TTY run with no key exits 2 with a `missing_key`
error and a hint to use `tcamp login --key-stdin` or `TOKENCAMP_API_KEY`.

`--no-launch` only Configures; it does not Launch the Harness and works even
when the Harness binary is not installed. `--dry-run` prints the pending diff
without writing. `--json` is for non-Launch commands; use it with
`--no-launch`. CLI output stays in English.

## Configuration

`tcamp` writes only user-level Config targets. It does not write shell startup
files, does not generate a launcher script, and does not manage project-level
configuration in v1.

| Harness | Config target | Launch |
| --- | --- | --- |
| Claude Code | Overlay at `$XDG_CONFIG_HOME/tokencamp/harnesses/claude/settings.json` (fallback `~/.config/tokencamp/harnesses/claude/settings.json`) | `claude --settings <overlay>` |
| Pi | `$PI_CODING_AGENT_DIR/models.json` (fallback `~/.pi/agent/models.json`) | `pi --provider tokencamp --model <id>` |
| Kimi Code | `$KIMI_CODE_HOME/config.toml` (fallback `~/.kimi-code/config.toml`) | `kimi -m <alias>` |

### Claude Code

`tcamp claude` owns the whole Overlay file and launches Claude Code with
`--settings <overlay>`. It never modifies `$CLAUDE_CONFIG_DIR/settings.json`
(the default is `~/.claude/settings.json`). The Overlay contains the Base URL
and the Model mapping for the Claude Code Slots; `ANTHROPIC_AUTH_TOKEN` is
injected at Launch and is absent from the file. The Overlay carries a `_tcamp`
marker used to recognize ownership.

### Pi

`tcamp pi` upserts `providers.tokencamp` in `models.json` and exposes five
Catalog models. The Provider entry uses `apiKey: "$TOKENCAMP_API_KEY"`, so no
API key value is stored in the file. Existing unknown fields, comments, and
formatting are preserved. Defaults are written only when `defaultProvider` or
`defaultModel` is missing or empty.

### Kimi Code

`tcamp kimi` upserts `[providers.tokencamp]` in `config.toml` with
`type = "openai"`, `base_url = "<Base URL>/v1"`, and
`api_key_env = "TOKENCAMP_API_KEY"`, plus `[models."tokencamp/*"]` aliases.
Existing unknown fields and the existing `default_model` value are preserved.

### Ownership, Drift, and Conflict

The Ownership manifest is stored at
`$XDG_CONFIG_HOME/tokencamp/managed.toml` with mode `0600`. It records each
Config target, the Managed entries `tcamp` created or Adopted, the last-written
hash, model ids, defaults, and backup paths. It never stores an API key.

Before changing a Config target, `tcamp` writes a timestamped backup under
`$XDG_STATE_HOME/tokencamp/backups/<harness>/` (fallback
`~/.local/state/tokencamp/backups/<harness>/`), mode `0600`, keeping the latest
three. When a user edits a Managed entry, `tcamp` reports Drift, warns, and
converges the Managed entry again. When a target already contains an entry that
`tcamp` does not own, that is a Conflict: the command exits 2 and explains the
fix. `--force` backs up the target and Adopts the conflicting entry; it does
not skip parsing, validation, atomic writes, or backups. `--yes` only answers
TTY confirmation prompts and cannot replace `--force`.

### Credentials and models

The Credential file is
`$XDG_CONFIG_HOME/tokencamp/credentials.toml` (fallback
`~/.config/tokencamp/credentials.toml`), mode `0600`, in a `0700` directory.
Each Base URL maps to one Connection. Login makes it the Active connection.
No API key is ever written into a Config target.
`TOKENCAMP_API_KEY` overrides the stored Credential for the current process.
`--model` and `TOKENCAMP_MODEL` override the main model for one Launch only;
the static Model mapping in the Config target is left unchanged.

## Troubleshooting

### No API key

Run `tcamp login`, set `TOKENCAMP_API_KEY`, or pipe a key with
`tcamp login --key-stdin`. A non-TTY Harness command without a key exits 2 and
points at these options.

### 401 Unauthorized

The API key is invalid or expired. For the official cloud, create or replace a
key at <https://tokencamp.com/keys>. For a self-hosted Base URL, use its Portal.
Replace the stored Credential with `tcamp login --force`, then restart any
running Harness so it picks up the new key. `tcamp logout` only removes local
state and does not revoke the key at the server.

### 402 Payment Required

The account or workspace has no remaining budget. For the official cloud,
check billing and usage at <https://tokencamp.com>; for a self-hosted Base URL,
check the workspace budget. A 402 does not block Configure or Launch, and it is
visible only through `--verify`, `tcamp doctor --network`, or a real Harness
request.

### Network problems

Run `tcamp doctor --network` to check reachability and the Auth probe. Requests
use HTTPS; plain HTTP is accepted only for loopback and private networks.
Timeouts and 5xx responses are reported with a hint to retry or use
`--no-verify` when storing a key.

### Harness missing or too old

The version floors are Claude Code `2.1.304`, Pi `1.4.0`, and Kimi Code
`2.1.1`. A missing Harness makes a normal Launch exit 2; `--no-launch` still
writes the Config target. A version below the floor produces a warning, but
Launch continues. Install or upgrade the Harness with its own installer.

### Corrupt configuration

If a Config target is invalid, `tcamp` prints the path and parse location and
exits 2 without writing. Fix the file, restore a backup from the Ownership
manifest, or run `tcamp doctor` to locate the problem. `--force` does not
bypass parsing or validation.

### Claude Code 404 or anthropic deployment

Claude Code talks to `/v1/messages`, which requires a Deployment with
`provider_type: anthropic`. Without that deployment, `tcamp claude` is
config-only: it still configures the Overlay, warns that the capability is
unverified, and does not block Launch. Run `tcamp doctor --harness claude` for
the default warning, or `tcamp doctor --verify --harness claude` to send one
`/v1/messages` request. A `model_not_found` 404 points at the missing anthropic
deployment. Use `tcamp doctor --verify --harness pi` to validate the key over
chat completions.

## Security

The Credential lives in `credentials.toml` with mode `0600`; the directory is
`0700`; writes are atomic and refuse symlinks. API keys are masked in output,
and the full key never appears in logs, errors, diffs, JSON, or backups.

The key is injected into the Harness environment at Launch time only; API key
injection is never written into a Config target. A running Harness keeps the
key it received until it restarts. Revocation through the tokencamp Portal
takes effect immediately for new requests. Environment variables are visible
to other processes of the same user, which is an accepted v1 boundary and not
a defense against a malicious same-user process.

Release archives are downloaded over HTTPS, verified against `SHA256SUMS`, and
published with build provenance and immutable release assets. v0.1 releases are
not signed or notarized. `tcamp` performs no telemetry, analytics, or update
checks.

## Known limitations

- Codex is not supported in v1; only its design work exists.
- Claude live Smoke deferred; Claude Code is config-only unless your
  tokencamp deployment includes `provider_type: anthropic`.
- Windows is not supported.
- There is no OS keyring support, no hot rotation while a Harness is running,
  and no project-level configuration.
- The Model mapping is a static curated table; new models arrive with a new
  `tcamp` release.
- In Claude Code, thinking is disabled, prompt cache behavior is degraded, and
  tool search is disabled.

## Uninstall

```sh
tcamp uninstall
```

`tcamp uninstall` removes only the Managed entries that `tcamp` created and
that still match the last-written hash. Entries you modified are kept with a
warning; `--force` removes them. Use `--dry-run` to print the plan, `--yes` for
non-interactive confirmation, and `--harness <name>` to limit the scope.

Uninstall keeps your Credential, backups, and the `tcamp` binary. It prints the
paths it kept and the exact command to remove the binary:

```sh
rm "$(command -v tcamp)"
```

Use `tcamp logout --all` to remove the Credential. `install.sh` has no
`--uninstall` option, and v1 does not provide an updater. Delete backups
manually if you no longer need them.

## License

Apache-2.0. See [LICENSE](./LICENSE) and [SECURITY.md](./SECURITY.md).
