# dsh Adapter 契约

Status: verified against `@deepseek-ai/dsh` `0.2.2-rc.1` on macOS (Node `v26.2.0`) in an isolated `DSH_HOME`; unverified items are marked. Ticket: #38. Map: #37.

## Summary

The adapter shape is viable: tcamp can own a dedicated `tokencamp` profile, write one `cordis.patch.yml`, set the default model through `agent-default-model`, and launch the Web UI with `dsh --profile tokencamp`. The key integration seam is dsh's `llm-pi-ai` custom-provider config using `api: openai-completions` and `apiKeyEnv`.

Two facts change the implementation plan:

1. `--from-default-profile` is destructive and normally boots the app. `dsh --profile tokencamp --from-default-profile web --dump-config` creates a fresh profile and exits without booting (verified). tcamp must only use this when the profile is absent or `--force` is given.
2. dsh's credential store wins over the launch environment. A stored `TOKENCAMP_API_KEY` under `$DSH_HOME/.credentials.yaml` shadows the key tcamp injects. This needs a preflight shadowing check or a dedicated credential-ref name.

Open risk: DeepSeek thinking + tools may require replaying `reasoning_content`; tokencamp-pro currently does not carry that field. Tracked in #44.

## 1. Profile lifecycle

- `$DSH_HOME` defaults to `~/.dsh`; the environment variable overrides it. The dedicated profile lives at `$DSH_HOME/profiles/tokencamp/`.
- Create it without booting:

  ```sh
  DSH_HOME=... dsh --profile tokencamp --from-default-profile web --dump-config
  ```

  Verified: exit 0, no server, and these files are written:
  - `package.json` — `dsh.profile.bundles` = `@deepseek-ai/dsh-base`, `@deepseek-ai/dsh-web-app`
  - `cordis.patch.yml` — `[]` (the user patch layer)
  - `cordis.yml` — empty root entry list
  - `pnpm-workspace.yaml` — `packages: [.]`, `nodeLinker: hoisted`, `autoInstallPeers: false`

- `--from-default-profile` **replaces** an existing profile with a fresh template copy. Never run it on an existing profile unless `--force` is intended; for a tcamp-managed profile, only rewrite `cordis.patch.yml`.
- Validate the composed profile without booting:

  ```sh
  DSH_HOME=... dsh --profile tokencamp --dump-config
  ```

  A malformed patch or invalid provider config fails nonzero. This is a good preflight check.
- `cordis.patch.yml` is a top-level YAML array of loader patch entries. An entry targets an existing entry by `id` and supplies `config`; the entry config is replaced, not deep-merged.

## 2. Exact `cordis.patch.yml`

Verified to compose with `dsh --profile tokencamp --dump-config` (exit 0). Replace the example base URL with tcamp's normalized API root (`.../v1`).

```yaml
- id: llm-pi-ai
  config:
    providers:
      tokencamp:
        displayName: Tokencamp
        api: openai-completions
        baseURL: https://api.tokencamp.com/v1
        apiKeyEnv: TOKENCAMP_API_KEY
        compat:
          supportsStore: false
          supportsDeveloperRole: false
          maxTokensField: max_tokens
        models:
          - id: deepseek/deepseek-v4.1-flash
            name: DeepSeek V4.1 Flash
            contextWindow: 1048576
            maxTokens: 32768
            input: [text, image]
            reasoningEfforts:
              off:
              high: high
              max: max
            compat:
              thinkingFormat: deepseek
              requiresReasoningContentOnAssistantMessages: true
          - id: deepseek/deepseek-v4-pro
            name: DeepSeek V4 Pro
            contextWindow: 1048576
            maxTokens: 32768
            input: [text]
            reasoningEfforts:
              off:
              high: high
              max: max
            compat:
              thinkingFormat: deepseek
              requiresReasoningContentOnAssistantMessages: true
          - id: zhipu/glm-5.3
            name: GLM-5.3
            contextWindow: 1048576
            maxTokens: 32768
            input: [text]
          - id: zhipu/glm-5.3-flash
            name: GLM-5.3 Flash
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
- id: agent-default-model
  config:
    provider: tokencamp
    model: deepseek/deepseek-v4.1-flash
```

Field notes:

- `providers` is the whole `llm-pi-ai` config surface. The route key (`tokencamp`) is the provider id shown to the model picker and used by `agent-default-model`.
- A route pi-ai does not ship needs `api`, `baseURL`, and a non-empty `models` list. Its `models` list replaces the catalog wholesale; we spell out capacity and input because there is no installed catalog entry to inherit from.
- `baseURL` is passed to model requests unchanged. It must include the API version segment (`/v1`). Discovery would append `/models`, but tcamp does not rely on discovery.
- `apiKeyEnv` is a credential reference resolved per request. No key is stored in this file.
- `input` defaults to `[text]`; `deepseek/deepseek-v4.1-flash` and `zhipu/glm-5.3-flash` explicitly add `image`.
- `maxTokens` is tcamp's curated cap, not the native 384K. Pi/dsh use `32768`, matching the existing harness policy; `tokencamp/auto` keeps `8192`.
- `reasoningEfforts` declares the picker's Effort levels. `off` is valueless; `thinkingFormat: deepseek` makes `off` send `thinking: {type: disabled}` and other levels send `thinking: {type: enabled}` plus `reasoning_effort`. Only the DeepSeek models declare levels here; GLM/Kimi/auto levels are left unverified and should not be invented.
- `compat` at the route is the default for all models; model-level `compat` wins field by field. `supportsStore: false`, `supportsDeveloperRole: false`, and `maxTokensField: max_tokens` mirror the existing Pi adapter's tokencamp workarounds (tokencamp rejects the developer role and `max_completion_tokens`). `requiresReasoningContentOnAssistantMessages` follows DeepSeek's own pi-ai catalog entry; see the risk in §9.

## 3. Default model

`agent-default-model` is a normal composed entry; its live config can be overridden in the same patch:

```yaml
- id: agent-default-model
  config:
    provider: tokencamp
    model: deepseek/deepseek-v4.1-flash
```

Verified in `--dump-config`: the entry defaults to `provider: deepseek-official`, `model: deepseek-flash`; the patch replaces it. This is how tcamp makes v4.1-flash the default without driving the Web UI. A session that already logged a selection keeps that selection; this only affects fresh agents.

## 4. Credentials and precedence

- `apiKeyEnv` is resolved by the harness credential seam on every request. With no resolvable value the request fails with `MISSING_CREDENTIAL`; tcamp must ensure the variable is injected at launch.
- Credential precedence, highest first:
  1. `$DSH_HOME/.credentials.yaml` (`refs:` entry)
  2. the launch environment snapshot (`TOKENCAMP_API_KEY=... dsh`)
  3. `<invocation cwd>/.env`
  4. `$DSH_HOME/.env`
- **Shadowing risk:** a value saved in `.credentials.yaml` beats the launch environment. If the user has saved `TOKENCAMP_API_KEY`, tcamp's injected key is ignored. Preflight should inspect `.credentials.yaml` for `refs.TOKENCAMP_API_KEY` and warn/conflict, or tcamp should use a dedicated reference name such as `TCAMP_DSH_KEY` to make accidental collisions unlikely. This is a #40 decision; do not silently overwrite the file.
- The store is a versioned YAML file created with owner-only permissions; dsh refuses to load it if another user can read it. tcamp must never write the API key into it.
- Direct `dsh --profile tokencamp` without tcamp has no injected key; it only works if the user has a stored ref or exported the variable. This matches the existing "only `tcamp <harness>` launches with the key" boundary.

## 5. Model ids and picker

- Model ids containing `/` are preserved verbatim by dsh (verified in `--dump-config`); no translation is needed.
- `name` is the picker display name. The provider `displayName` labels the route.
- The picker shows the route's `models` list; the default selection is the `agent-default-model` entry.
- `tokencamp/auto` is an explicit model id; it is not a wildcard in dsh.

## 6. Version and Node detection

- `dsh --version` prints a plain version, e.g. `0.2.2-rc.1`.
- `node --version` prints `v26.2.0` in the probe; the package declares `engines.node >= 24.9.0`.
- Recommended policy for the experimental adapter: floor at the verified `0.2.2-rc.1`; warn on any other version until re-verified. Semver prerelease ordering makes a "tested ceiling" ambiguous; an exact verified version plus a warn is safer than silently accepting untested RCs.

## 7. Launch and passthrough

- Launch the dedicated profile with `dsh --profile tokencamp`. Because the profile bundles are `dsh-base` + `dsh-web-app`, this boots the Web UI.
- Launcher flags are consumed before app args. App args follow: `dsh --profile tokencamp --no-open`, `dsh --profile tokencamp --port 8080`. `tcamp dsh -- --no-open` should pass everything after `--` to the dsh launcher/app; `tcamp` must not reorder `--profile tokencamp`.
- The Web UI defaults to `127.0.0.1:3080` and opens a browser on a local launch. `--no-open` runs the server without opening; SSH launches print the host URL. tcamp should just exec and inherit stdio.
- `--dump-config` is a non-booting validation/export path; it is not a launch mode.

## 8. Storage and uninstall

Under `$DSH_HOME`:

- `profiles/tokencamp/` — tcamp-owned config (the only directory tcamp edits)
- `sessions/` — JSONL session persistence
- `storages/` — JSON storage
- `.credentials.yaml` — credential refs/records (never edit)
- `logs/` — startup diagnostics
- `.env`, `.anonymous-user-id`, attachments, and other runtime data

Uninstall must preserve everything outside `profiles/tokencamp/`, especially `.credentials.yaml`, `sessions/`, `storages/`, and `logs/`. For the profile itself, the safe default is to remove tcamp's two patch entries (or restore the pre-change `cordis.patch.yml`) and keep the generated profile skeleton. Deleting the whole profile directory requires `--force` and should refuse if it contains files tcamp did not generate. Back up `cordis.patch.yml` before changing it, following the existing backup policy.

## 9. Hot reload and risks

- Provider/profile settings are re-read per request; a `cordis.patch.yml` change takes effect on the next request without restarting. Profile bundle/package changes still need a restart. tcamp writes before launch, so this is mostly a doctor/manual-edit concern.
- **`reasoning_content` risk:** DeepSeek's own pi-ai catalog sets `requiresReasoningContentOnAssistantMessages: true` for `deepseek-flash` and `deepseek-v4-pro`. tokencamp-pro's `ChatRequest::Message` has no `reasoning_content` field, so the gateway may drop it on request replay and response capture. If so, the second round of a thinking + tool call can 400. Ticket #44 owns the verification and the fallback (force non-thinking / conditional GO / blocked). Do not declare thinking + tools verified until #44 and the real E2E pass.
- **Credential shadowing:** see §4. This is the most likely cause of "tcamp configured it but dsh still gets 401" in the field.
- **Preview churn:** dsh warns that compatibility-breaking changes will happen. The profile file shape and plugin config schema are not stable API. Re-verify on every dsh upgrade; do not treat `0.2.2-rc.1` config as forward-compatible.
- **Model metadata:** native v4.1-flash output is 384K, but tcamp writes 32K. This is intentional and consistent with Pi; if a user needs the native cap, that is a separate policy decision.
- **Vision:** v4.1-flash declares `image`; tokencamp upstream supports vision, but the dsh Web UI image path is not verified. Treat as a non-core E2E item.
- **Reasoning effort:** the `thinking` / `reasoning_effort` wire shape is DeepSeek-native; tokencamp's `/v1/chat/completions` forwarding is not verified. E2E should test at least `off` and `high`.

## 10. Sources

- dsh CLI README: <https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/cli/README.md>
- dsh provider guide: <https://deepseek-harness.github.io/deepseek-harness/en/guide/providers>
- `@deepseek-ai/dsh-llm-pi-ai` README (installed package): custom routes, `apiKeyEnv`, `models`, `compat`, hot reload, discovery
- `@deepseek-ai/dsh-credentials-local` README: precedence (stored file > launch env > project `.env` > home `.env`), `.credentials.yaml` shape
- `@deepseek-ai/dsh-agent-default-model` README: `provider` / `model` config and per-profile writes
- `@earendil-works/pi-ai` installed catalog data: `deepseek-flash` / `deepseek-v4-pro` compat flags, including `requiresReasoningContentOnAssistantMessages`
- Local probe: `dsh 0.2.2-rc.1`, Node `v26.2.0`, isolated `DSH_HOME`, `--from-default-profile ... --dump-config` and `--dump-config` exit 0
- tokencamp-pro `config/default.yaml`: `deepseek/deepseek-v4.1-flash` metadata and deployment
