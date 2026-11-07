# tcamp v1 Release Checklist

Acceptance tier: v1 release (Tier T1 automated matrix + Tier T2 manual smoke, compatibility, and documentation review).

Environment for the local run: macOS 13.7.8 (Ventura), Intel x86_64, rustc/cargo 1.98.1, stable toolchain plus Rust 1.85.0. CI: `ubuntu-latest` x86_64 and `macos-15` arm64. The SPEC matrix totals 121 rows: 107 T1 rows are in the T1 table below and 14 T2 rows (SMOKE 3, COMPAT 4, DOC 7) are in the T2 table. T2 is manual and requires real Harness binaries and a real tokencamp API key; it cannot be completed by the deterministic suite.

Date of this run: 2026-11-07. Worktree branch: `ticket/30-acceptance`. Integration base: `integration/v1` at `b3e0853`. The checklist is unsigned; the release manager signs after T2 is complete and no open P0/P1 remains.

Result vocabulary: `pass` means the runnable local portion passed; `skipped` means the row requires a tag/release or an unavailable prerequisite and was not executed; `blocked` means a required prerequisite is missing; `fail` means an executed check failed. CI-only portions are called out in Notes.

## Local T1 run

| Command | Result | Measurement / note |
| --- | --- | --- |
| `cargo fmt --all -- --check` | pass | 0 diffs |
| `cargo clippy --all-targets --locked -- -D warnings` | pass | 0 warnings |
| `cargo test --locked` | pass | 368 passed, 0 failed, 1 ignored (the release-perf test, run separately below) |
| `cargo +1.85.0 check --all-targets --locked` | pass | MSRV 1.85.0 check completed |
| `cargo build --release --locked` | pass | `target/release/tcamp` 4,716,304 B |
| `sh scripts/package.sh 0.1.0 x86_64-apple-darwin target/release/tcamp /tmp/tcamp-dist-final` | pass | archive 2,255,574 B; entries `tcamp`, `LICENSE`; `.sha256` plus two-space `SHA256SUMS` |
| `sh scripts/check-release-gates.sh target/release/tcamp /tmp/tcamp-dist-final/tcamp-0.1.0-x86_64-apple-darwin.tar.gz` | pass | p95 `--version` 15.2 ms, `--help` 13.8 ms, `status` 12.9 ms, `doctor` 13.3 ms (limits 100/100/500/500) |
| `cargo test --locked --test perf -- --ignored` | pass | 1 passed; p95 `--version` 6.0 ms, `--help` 5.3 ms, `status` 6.3 ms, `doctor` 6.3 ms |
| `sh scripts/install-smoke.sh` | pass | `install-smoke: ok`; tampered checksum refused, existing install preserved, shellcheck and `sh -n` clean |

CI for the integration tip (`integration/v1` `b3e0853`, workflow run [42370138121](https://github.com/0xnicholas/tokencamp-tools/actions/runs/42370138121)): all five jobs succeeded.

| CI job | Result | Duration |
| --- | --- | --- |
| `test (ubuntu-latest)` | pass | 1m12s |
| `test (macos-15)` | pass | 1m54s |
| `msrv (1.85)` | pass | 39s |
| `install-smoke (ubuntu-latest)` | pass | 40s |
| `install-smoke (macos-15)` | pass | 1m0s |

The current `integration/v1` workflow uses `macos-15` (arm64), not `macos-14`; `b3e0853` replaced the label. The earlier run on `1dde819` was cancelled while the macOS jobs were queued, so it is not used as evidence. The draft PR is #31 (`integration/v1` -> `main`). This checklist is a docs-only change on `ticket/30-acceptance`; the CI result above is for the code tip that this branch merges, not for a separate run of the checklist commit.

## T1 matrix

| ID | Tier | Area | Evidence | Result | Notes |
| --- | --- | --- | --- | --- | --- |
| AUTH-01 | T1 | Login | `tests/auth.rs` | pass | 0700 dir, 0600 file, JSON ok, masked key. |
| AUTH-02 | T1 | Login | `tests/auth.rs` | pass | 200 probe writes verified_at. |
| AUTH-03 | T1 | Login | `tests/auth.rs` | pass | 401 does not save; self-hosted hint; TTY retry in auth_tty.rs. |
| AUTH-04 | T1 | Login | `tests/auth.rs` | pass | 402/403/429 save with warning; 5xx/timeout/unexpected 404 do not save. |
| AUTH-05 | T1 | Login | `tests/auth.rs` | pass | --no-verify makes zero requests. |
| AUTH-06 | T1 | Login | `tests/auth.rs` | pass | One minimal chat after probe: model auto, max_tokens 1, stream false. |
| AUTH-07 | T1 | Login UX | `tests/auth_tty.rs` | pass | TTY hidden input/retry; non-TTY missing key exit 2; env source. |
| AUTH-08 | T1 | Credential priority | `tests/auth.rs` | pass | Env overrides file and is not persisted. |
| AUTH-09 | T1 | Overwrite | `tests/auth.rs`, `tests/auth_tty.rs` | pass | Non-TTY needs --force; force refreshes created_at and clears verified_at; TTY yes/no. |
| AUTH-10 | T1 | Key shape | `tests/auth.rs` | pass | Shape warning only; empty/control chars exit 2; no --key flag. |
| AUTH-11 | T1 | Corrupt/symlink | `tests/auth.rs`, `tests/doctor.rs` | pass | Corrupt/version/symlink fail; 0644 doctor fail. |
| AUTH-12 | T1 | Atomic/concurrency | `tests/auth.rs` | pass | Two-process atomic login; injected rename failure preserves original. |
| AUTH-13 | T1 | Logout | `tests/auth.rs` | pass | Active/named/all removal; no auto-switch; harness config untouched; restart warning. Portal-revocation wording is documented, not asserted. |
| AUTH-14 | T1 | Status | `tests/status.rs` | pass | Status is local only; active URL, masked key/source, verified_at, path, harness snapshot. |
| AUTH-15 | T1 | Redaction | `tests/redaction.rs` | pass | Sentinel key absent from all output, JSON, diff, and backups. |
| AUTH-16 | T1 | Portal hint | `tests/auth.rs`, `src/credentials/mod.rs` | pass | Self-hosted hint does not guess /keys; official-cloud portal hint is a src unit test. |
| AUTH-17 | T1 | Paths | `tests/auth.rs` | pass | XDG and ~/.config fallback; 0700/0600. |
| AUTH-18 | T1 | HTTP policy | `tests/url.rs` | pass | Public HTTP exit 2; loopback/private HTTP allowed with warning; HTTPS allowed. |
| URL-01 | T1 | Normalize | `tests/url.rs` | pass | Strips /v1 and trailing slash. |
| URL-02 | T1 | Precedence | `tests/url.rs` | pass | --base-url > env > active > default. |
| URL-03 | T1 | Active | `tests/url.rs` | pass | One key per base URL; login sets active; logout active clears without switching. |
| URL-04 | T1 | Harness base | `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs` | pass | Claude root; Pi/Kimi root+/v1; invalid URL exit 2. |
| CFG-C-01 | T1 | Paths/permissions | `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs` | pass | SPEC names tests/config_common.rs, which is absent. Adapter suites cover 0700/0600, existing mode, state/backup paths. |
| CFG-C-02 | T1 | Atomic write | `tests/config_claude.rs` | pass | SPEC path absent; Claude engine test injects rename failure and checks temp cleanup. |
| CFG-C-03 | T1 | Backup/rollback | `tests/config_claude.rs` | pass | SPEC path absent; timestamped 0600 backups, keep 3, manifest rollback, rollback-failure exit 2 with both paths. |
| CFG-C-04 | T1 | Lock | `tests/config_claude.rs` | pass | SPEC path absent; flock timeout, post-lock hash reread, one retry, persistent conflict exit 1. |
| CFG-C-05 | T1 | Symlink | `tests/config_claude.rs` | pass | SPEC path absent; symlink target write preserves link; broken/non-regular rejected. |
| CFG-C-06 | T1 | Parse failure | `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs` | pass | SPEC path absent; JSON/JSONC/TOML parse failures print path/location and write nothing. |
| CFG-C-07 | T1 | Merge scope | `tests/config_pi.rs`, `tests/config_kimi.rs` | pass | SPEC path absent; JSONC comments/trailing commas and TOML comments/table order preserved. |
| CFG-C-08 | T1 | Secret boundary | `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs`, `tests/redaction.rs` | pass | SPEC path absent; no key in targets; literal-key conflict backup 0600; redaction suite. |
| CFG-C-09 | T1 | Dry-run | `tests/cli.rs`, `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs` | pass | Dry-run creates no dirs/locks/backups; diff on stdout, secret masked; JSON changes[]. |
| CFG-C-10 | T1 | Force/yes | `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs`, `tests/uninstall.rs` | pass | --force adopts, --yes only answers TTY; non-TTY conflict exit 2. |
| CFG-C-11 | T1 | Drift/uninstall | `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs`, `tests/uninstall.rs` | pass | Drift warns and converges; uninstall deletes only unchanged managed entries; modified kept. |
| CFG-C-12 | T1 | Shadowing | `tests/shadowing.rs` | pass | Claude/Pi/Kimi shadowing; values never printed; --force cannot bypass. |
| CFG-C-13 | T1 | Manifest missing | `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs`, `tests/uninstall.rs` | pass | Missing manifest: Claude marker cleanup; Pi/Kimi require --force/manual; schema mismatch exit 2. |
| CFG-C-14 | T1 | Blast radius | `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs`, `tests/uninstall.rs` | pass | No shell profile/wrapper/project config; uninstall aggregates failures exit 1. |
| CFG-CL-01 | T1 | Overlay | `tests/config_claude.rs` | pass | Overlay path, strict JSON, key order, idempotence, backup. |
| CFG-CL-02 | T1 | Env | `tests/config_claude.rs` | pass | Env keys and constants checked; no auth token in file. |
| CFG-CL-03 | T1 | Launch | `tests/config_claude.rs` | pass | --settings overlay; user settings untouched; CLAUDE_CONFIG_DIR shadow check. |
| CFG-CL-04 | T1 | Ownership | `tests/config_claude.rs` | pass | Unowned/version mismatch conflict; force backup+adopt; uninstall only unchanged. |
| CFG-CL-05 | T1 | Shadowing | `tests/shadowing.rs` | pass | Readable token/apiKeyHelper blocks; unreadable managed warns; values not printed. |
| CFG-CL-06 | T1 | Binary/version | `tests/config_claude.rs`, `tests/doctor.rs` | pass | Fake claude 2.1.303/2.1.304; missing binary exit 2 vs --no-launch success; doctor warn. |
| CFG-CL-07 | T1 | Messages/deferred | `tests/config_claude.rs`, `tests/doctor.rs`, `tests/doctor_network.rs` | pass | Deferred /v1/messages warning; verify one messages request; 404 deployment hint; 403/429 warn. |
| CFG-CL-08 | T1 | Models/flags | `tests/config_claude.rs` | pass | --model only main slot; invalid exit 2; no modelPicker/custom/small-fast keys. |
| CFG-PI-01 | T1 | File/path | `tests/config_pi.rs` | pass | PI_CODING_AGENT_DIR/fallback; $schema+providers; no project file. |
| CFG-PI-02 | T1 | Provider | `tests/config_pi.rs` | pass | Provider fields and compat flags exact. |
| CFG-PI-03 | T1 | Models/defaults | `tests/config_pi.rs` | pass | Five models, context/maxTokens, no cost; default only if missing/empty. |
| CFG-PI-04 | T1 | JSONC preserve | `tests/config_pi.rs` | pass | Comments/trailing commas/order preserved; unrepresentable JSONC aborts. |
| CFG-PI-05 | T1 | Ownership | `tests/config_pi.rs` | pass | Unowned conflict; force backup+adopt; manifest ownership/hash; drift; uninstall. |
| CFG-PI-06 | T1 | Shadowing | `tests/shadowing.rs` | pass | auth.json tokencamp/corrupt blocks; values untouched. |
| CFG-PI-07 | T1 | Binary/version | `tests/config_pi.rs`, `tests/doctor.rs` | pass | Fake pi 1.3.x/1.4.0; missing binary; doctor warn; launch continues below floor. |
| CFG-PI-08 | T1 | Models/tool loop | `tests/config_pi.rs`, `tests/mock_pi.rs` | pass | Five models/aliases; openai-completions; two-round tool loop in mock_pi. |
| CFG-KIMI-01 | T1 | File/provider | `tests/config_kimi.rs` | pass | KIMI_CODE_HOME/fallback; provider type/base_url/api_key_env. |
| CFG-KIMI-02 | T1 | Models | `tests/config_kimi.rs` | pass | Five aliases, canonical model, context 1048576/262144, max_output 8192; default only if missing. |
| CFG-KIMI-03 | T1 | TOML preserve | `tests/config_kimi.rs` | pass | TOML comments/blank lines/table order preserved; no marker comments. |
| CFG-KIMI-04 | T1 | Literal key | `tests/config_kimi.rs` | pass | Owned literal key backed up/removed; unowned conflict until force; never printed. |
| CFG-KIMI-05 | T1 | Ownership | `tests/config_kimi.rs` | pass | Unowned conflict; force adopt; manifest; drift; uninstall. |
| CFG-KIMI-06 | T1 | Binary/version | `tests/config_kimi.rs`, `tests/doctor.rs` | pass | Fake kimi 2.1.0/2.1.1; missing binary; doctor warn; launch uses -m alias. |
| CFG-KIMI-07 | T1 | Models/tool loop | `tests/config_kimi.rs`, `tests/mock_kimi.rs` | pass | Five aliases; type=openai; /v1/chat/completions; two-round tool loop in mock_kimi. |
| CLI-01 | T1 | Command tree | `tests/cli.rs` | pass | Command tree; bare run help+detected; no codex/opencode/env. |
| CLI-02 | T1 | Flags/passthrough | `tests/cli.rs` | pass | --json+launch conflict; -- passthrough; unknown flag hint. |
| CLI-03 | T1 | First run | `tests/cli_tty.rs`, `tests/config_tty.rs` | pass | TTY hidden key -> probe -> Write it? -> exec; non-TTY missing key exit 2; --no-launch no prompt. |
| CLI-04 | T1 | Daily/configured | `tests/cli.rs` | pass | Quiet when unchanged; Configured line on write; no repeat prompt. |
| CLI-05 | T1 | JSON | `tests/cli.rs`, `tests/doctor_json.rs`, `tests/auth.rs`, `tests/status.rs`, `tests/config_claude.rs` | pass | SPEC names tests/cli_json.rs, which is absent. JSON assertions are distributed across cli.rs, doctor_json.rs, auth.rs, status.rs, config_claude.rs. |
| CLI-06 | T1 | Streams/color | `tests/cli.rs` | pass | Prompts stderr, results stdout, color only TTY, NO_COLOR/--no-color/TERM=dumb. |
| CLI-07 | T1 | Exit codes | `tests/cli.rs` | pass | 0 success, 1 runtime/network/write, 2 usage/missing/conflict/unreadable; exec handoff. |
| CLI-08 | T1 | JSON shape | `tests/cli.rs`, `tests/doctor_json.rs`, `tests/auth.rs`, `tests/status.rs`, `tests/config_claude.rs` | pass | SPEC path absent; stable success/error shapes and dry-run changes[] asserted across the same distributed files; not every code has a dedicated case. |
| CLI-09 | T1 | Errors | `tests/cli.rs`, `tests/auth.rs`, `tests/config_claude.rs` | pass | 2-3 line error+hint; request_id only JSON/-v; no stack/body/key. |
| CLI-10 | T1 | Exec | `tests/cli.rs`, `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs` | pass | Exec after configure; Launching stderr; -- args forwarded; no wrapper. |
| CLI-11 | T1 | 402 | `tests/auth.rs`, `tests/doctor_network.rs` | pass | Verify-path 402 is covered by auth.rs and doctor 402 by doctor_network.rs; launch-time 402 warning has no direct test. |
| CLI-12 | T1 | Missing harness | `tests/cli.rs`, `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs`, `tests/doctor.rs` | pass | Missing harness exit 2 with install link; --no-launch writes and exits 0; doctor warn. |
| CLI-13 | T1 | Status/uninstall | `tests/cli.rs`, `tests/uninstall.rs`, `tests/uninstall_tty.rs`, `tests/status.rs` | pass | Status local; uninstall plans, TTY/--yes, --harness, --dry-run, --force; credentials/backups/binary kept; partial failure exit 1. |
| CLI-14 | T1 | Language/version/help | `tests/cli.rs`, `tests/docs.rs` | pass | English CLI/help; semver; quickstart; docs parity by tests/docs.rs. |
| DR-01 | T1 | Read-only | `tests/doctor.rs` | pass | Read-only board; no --fix; no writes/locks/prompts. |
| DR-02 | T1 | credential.file | `tests/doctor.rs` | pass | Missing+env warn; dir/file mode, corrupt, version, symlink. |
| DR-03 | T1 | credential.key/base_url | `tests/doctor.rs` | pass | Key missing fail/shape warn/env warn; HTTPS/private HTTP pass; public HTTP/invalid fail. |
| DR-04 | T1 | network.reachable/auth | `tests/doctor_network.rs` | pass | Reachable/auth mapping with/without key; no key skips. |
| DR-05 | T1 | network.verify | `tests/doctor_network.rs` | pass | --verify implies network; one minimal chat, 30s; status mapping. |
| DR-06 | T1 | Harness binary | `tests/doctor.rs` | pass | Unselected skip; missing warn; below floor warn; version probe fail; floors 2.1.304/1.4.0/2.1.1. |
| DR-07 | T1 | Config/ownership/backups | `tests/doctor.rs` | pass | Unconfigured skip; parse/unwritable/missing target/symlink fail; manifest/drift/backup warnings. |
| DR-08 | T1 | Shadowing/messages | `tests/doctor.rs`, `tests/shadowing.rs` | pass | Claude/Pi/Kimi shadowing; Claude messages default warn/verify statuses. |
| DR-09 | T1 | Models | `tests/doctor.rs`, `tests/doctor_network.rs` | pass | Offline static mapping complete; online model 200/404/network. |
| DR-10 | T1 | Exit/filter | `tests/doctor.rs` | pass | Exit 0/1; usage 2; harness filter only harness blocks; invalid/repeat 2. |
| DR-11 | T1 | JSON shape | `tests/doctor_json.rs` | pass | One object; ok==fail 0; key.verified null offline; checks[] fields; summary. |
| DR-12 | T1 | Preflight | `tests/preflight.rs` | pass | Preflight budget 2s; only relevant checks; 404 exit 2; network warn; --no-launch/--dry-run. |
| MOD-01 | T1 | Static table | `tests/models.rs` | pass | Static Claude slot mapping; Pi/Kimi five models; context/output table. |
| MOD-02 | T1 | Claude slots | `tests/config_claude.rs` | pass | Four ANTHROPIC_DEFAULT_*; no auto/preset/family; min window/32768 output. |
| MOD-03 | T1 | Pi fields | `tests/config_pi.rs` | pass | Pi provider/models fields; no cost; defaults only if empty. |
| MOD-04 | T1 | Kimi fields | `tests/config_kimi.rs` | pass | Kimi aliases/canonical; max_output 8192; defaults only if empty. |
| MOD-05 | T1 | Precedence | `tests/models.rs`, `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs` | pass | --model > env > default; empty env ignored; launch-only, config unchanged. |
| MOD-06 | T1 | Validation | `tests/models.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs`, `tests/preflight.rs` | pass | Claude canonical only; Pi/Kimi five; invalid exit 2; model 404 exit 2; network warn. |
| PKG-01 | T1 | Crate | `tests/packaging.rs` | pass | Single package; edition 2024, MSRV 1.85, Apache-2.0, bin tcamp, release profile, lockfile. |
| PKG-02 | T1 | Targets | `.github/workflows/release.yml`, `tests/release_workflow.rs` | pass | Local x86_64-apple-darwin built, packaged, and --version OK. Other three targets are tag-triggered release-only and were not run here; PR CI does not build them. |
| PKG-03 | T1 | Assets | `tests/packaging.rs` | pass | Local archive contains only tcamp+LICENSE, .sha256 and two-space SHA256SUMS; full four-archive set is release-time. |
| PKG-04 | T1 | install.sh | `tests/install_sh.rs` | pass | 21 installer tests: default dir, version pin, checksum, platform, atomic replace, PATH hint, no sudo/profile/quarantine/uninstall. |
| PKG-05 | T1 | install fail | `tests/install_sh.rs`, `scripts/install-smoke.sh`, `.github/workflows/ci.yml` | pass | install-smoke covers tampered checksum, existing install preservation, temp cleanup, shellcheck/sh -n. |
| PKG-06 | T1 | Release workflow | `.github/workflows/release.yml`, `tests/release_workflow.rs` | skipped | skipped: no v* tag pushed. Workflow contract (tag-only, verify, four targets, ancestor, CHANGELOG, prerelease, permissions) is checked by tests/release_workflow.rs; publish not executed. |
| PKG-07 | T1 | CI/supply | `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `tests/release_workflow.rs`, `tests/nfr.rs` | pass | Contract tests + CI run 42370138121 success on ubuntu-latest x86_64 and macos-15 arm64; MSRV, deny, Dependabot, minimal permissions, --locked, provenance. |
| PKG-08 | T1 | Docs/signing | `tests/docs.rs`, `tests/release_workflow.rs` | pass | No signing/notarization; Gatekeeper/xattr docs; PVR; no CONTRIBUTING/templates; uninstall binary hint; no updater. |
| NFR-01 | T1 | Startup latency | `tests/perf.rs` | pass | Release gates p95: --version 15.2ms, --help 13.8ms, status 12.9ms, doctor 13.3ms (limits 100/100/500/500). |
| NFR-02 | T1 | Binary size | `.github/workflows/release.yml`, `tests/packaging.rs` | pass | 4,716,304 B uncompressed (4.50 MiB <=15 MiB); 2,255,574 B tar.gz (2.15 MiB <=6 MiB). Host target only; other targets release-time. |
| NFR-03 | T1 | No telemetry | `tests/nfr.rs` | pass | Zero outbound connections for offline commands; dead proxy; static audit finds no telemetry/update/analytics. |
| NFR-04 | T1 | Redaction | `tests/redaction.rs` | pass | Sentinel key absent from stdout/stderr/JSON/diff/backups; mask sk-...last4. |
| NEG-01 | T1 | Codex | `tests/cli.rs` | pass | Codex exit 2, no config writes, README hint; doctor has no Codex check. |
| NEG-02 | T1 | Removed commands | `tests/cli.rs` | pass | OpenCode/env/--project/--fix/--lang rejected, no writes. |
| NEG-03 | T1 | No proxy/wrapper | `tests/config_claude.rs`, `tests/config_pi.rs`, `tests/config_kimi.rs`, `tests/nfr.rs` | pass | No wrapper/proxy/shell profile/project config/keyring/hot rotation; NFR source audit. |
| NEG-04 | T1 | Platform/pro | `tests/packaging.rs`, `tests/perf.rs`, `.github/workflows/release.yml` | pass | No Windows artifacts; install.sh exits 2; no tokencamp-pro coupling/GUI/telemetry. |

## T2 matrix (manual)

| ID | Date | OS/arch | Harness version | Key (masked) | Result | Request ID | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SMOKE-01 | 2026-11-07 | macOS 13.7.8 x86_64 | Pi 1.5.0 | — (no real key) | blocked | — | No real tokencamp API key is available in this environment (`TOKENCAMP_API_KEY` absent). Run in a clean HOME/XDG/PI_CODING_AGENT_DIR with Pi on PATH: `printf '%s' "$TOKENCAMP_API_KEY" \| tcamp login --key-stdin && tcamp pi --no-launch && tcamp pi -- -p 'Reply with exactly TCAMP_SMOKE_OK and nothing else.'` Expect exit 0, `TCAMP_SMOKE_OK` on stdout, no `error:` on stderr, and a single run under $0.01. |
| SMOKE-02 | 2026-11-07 | macOS 13.7.8 x86_64 | Kimi 2.1.1 | — (no real key) | blocked | — | No real tokencamp API key is available in this environment (`TOKENCAMP_API_KEY` absent). Run in a clean HOME/XDG/KIMI_CODE_HOME with Kimi on PATH: `printf '%s' "$TOKENCAMP_API_KEY" \| tcamp login --key-stdin && tcamp kimi --no-launch && tcamp kimi -- -p 'Reply with exactly TCAMP_SMOKE_OK and nothing else.'` Expect exit 0, `TCAMP_SMOKE_OK` on stdout, no `error:` on stderr, and a single run under $0.01. |
| SMOKE-03 | 2026-11-07 | — | Claude (not installed) | — | deferred | — | Deferred by SPEC: trigger is ops adding an anthropic deployment (`deepseek/deepseek-v4.1-flash` or `deepseek/deepseek-v4-pro`, base URL `https://api.deepseek.com/anthropic`); owner is the release manager/ops. Not blocking v1. When unblocked, run `tcamp claude --no-launch` then `tcamp claude -- -p 'Reply with exactly TCAMP_SMOKE_OK and nothing else.'`. |
| COMPAT-01 | 2026-11-07 | macOS 13.7.8 x86_64 | Claude not installed | — | blocked | — | `claude` is not installed. With real Claude 2.1.304 and 2.1.316, run `tcamp claude --no-launch` and `tcamp claude`; verify the overlay parses, `--settings` takes effect, user settings are untouched, and shadowing detection is correct. No live `/v1/messages` request is required. |
| COMPAT-02 | 2026-11-07 | macOS 13.7.8 x86_64 | Pi 1.5.0 | `sk-...cdef` (synthetic) | pass | — | Real Pi 1.5.0 in a temp HOME/PI_CODING_AGENT_DIR: `pi --list-models --offline` listed all five tokencamp models; `tcamp pi -- --version` printed 1.5.0; no network request. Floor 1.4.0 was not installed. |
| COMPAT-03 | 2026-11-07 | macOS 13.7.8 x86_64 | Kimi 2.1.1 | `sk-...cdef` (synthetic) | pass | — | Real Kimi 2.1.1 in a temp HOME/KIMI_CODE_HOME: `kimi doctor config` reported `config.toml` valid; `kimi provider list` showed `tokencamp type=openai models=5`; `tcamp kimi -- --version` printed 2.1.1; no network request. |
| COMPAT-04 | 2026-11-07 | macOS 13.7.8 x86_64; macOS arm64 + Linux x86_64 CI | n/a | n/a | pass | — | Local `cargo test` covers the generic config path, 0700/0600, symlink, and flock contracts on macOS x86_64; CI run 42370138121 is green on macos-15 arm64 and ubuntu-latest x86_64. Linux arm64 is not separately covered. |
| DOC-01 | 2026-11-07 | macOS 13.7.8 x86_64 | n/a | n/a | pass | n/a | README EN/ZH cover curl install, `TCAMP_VERSION`/`--version`, `SHA256SUMS`, supported platforms, `TCAMP_INSTALL_DIR`, PATH, and Gatekeeper right-click/`xattr`. |
| DOC-02 | 2026-11-07 | macOS 13.7.8 x86_64 | n/a | n/a | pass | n/a | Quickstart covers `tcamp login` -> `tcamp claude/pi/kimi`, first-run TTY prompt, non-TTY `missing_key`, and `--no-launch`. |
| DOC-03 | 2026-11-07 | macOS 13.7.8 x86_64 | n/a | n/a | pass | n/a | Configuration section covers target paths, Overlay, Ownership manifest, Drift/Conflict/`--force`, and no key in a target. |
| DOC-04 | 2026-11-07 | macOS 13.7.8 x86_64 | n/a | n/a | pass | n/a | Troubleshooting covers no key, 401, 402, network, missing/old Harness, corrupt config, and Claude 404/anthropic deployment. |
| DOC-05 | 2026-11-07 | macOS 13.7.8 x86_64 | n/a | n/a | pass | n/a | Security/limits cover 0600 credentials, launch-time injection, Portal revocation, uninstall keeping credentials, and Codex/Claude live Smoke/Windows/no keyring/hot rotation/project-level limitations; SECURITY.md points to Private Vulnerability Reporting. |
| DOC-06 | 2026-11-07 | macOS 13.7.8 x86_64 | n/a | n/a | pass | n/a | Uninstall section covers cleanup, keeping credentials/backups, `rm "$(command -v tcamp)"`, and no updater. |
| DOC-07 | 2026-11-07 | macOS 13.7.8 x86_64 | n/a | n/a | pass | n/a | README EN/ZH have eight aligned H2 sections; GLOSSARY terms are used, banned synonyms are absent (`tests/docs.rs`), and CLI output stays English. |

## Outstanding before sign-off

- The Pi and Kimi smoke rows are blocked on a real tokencamp API key; they must be run in a clean HOME on the target release machine.
- The Claude compatibility row is blocked because `claude` is not installed on this machine; it must be run with real Claude 2.1.304 and 2.1.316.
- The Claude smoke row is deferred by SPEC (ops anthropic deployment); it does not block v1.
- The three non-host release targets and the tag-triggered release workflow were not run: the other-target builds, the full four-archive set, the publish job, and the size checks on other targets remain release-time checks. The release workflow enforces them on a `v*` tag.
- PR #31 (`integration/v1` -> `main`) must be merged before tagging; the release tag must be cut from `main` because the release workflow checks that the tag is an ancestor of `origin/main`.
- No P0/P1 priority labels exist in this repository; the release manager must confirm manually that no open P0/P1 remains.

## Release sign-off

This checklist is unsigned. The release manager signs only after the blocked T2 rows are completed or explicitly accepted, the release-time checks above are green, and no open P0/P1 remains.

| Role | Name | Date | Tag |
| --- | --- | --- | --- |
| Release manager |  |  |  |
