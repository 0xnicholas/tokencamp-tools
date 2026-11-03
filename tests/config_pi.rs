mod support;

use std::fs;
use std::path::PathBuf;
use std::process::Output;

use serde_json::Value;
use support::{MockResponse, MockServer, TestEnv, model_server};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";

fn login(env: &TestEnv, base_url: &str) {
    env.login(base_url, KEY);
}

fn run(env: &TestEnv, args: &[&str]) -> Output {
    env.command().args(args).output().expect("run tcamp")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn models_path(env: &TestEnv) -> PathBuf {
    env.pi_models_path()
}

fn settings_path(env: &TestEnv) -> PathBuf {
    env.pi_settings_path()
}

fn models_text(env: &TestEnv) -> String {
    fs::read_to_string(models_path(env)).expect("read models.json")
}

fn models_value(env: &TestEnv) -> Value {
    jsonc_parser::parse_to_serde_value(&models_text(env), &Default::default())
        .expect("models.json is valid JSONC")
}

fn settings_value(env: &TestEnv) -> Value {
    serde_json::from_str(&fs::read_to_string(settings_path(env)).expect("read settings.json"))
        .expect("settings.json is strict JSON")
}

fn provider(models: &Value) -> &Value {
    &models["providers"]["tokencamp"]
}

fn model<'a>(provider: &'a Value, id: &str) -> &'a Value {
    provider["models"]
        .as_array()
        .expect("models array")
        .iter()
        .find(|model| model["id"] == id)
        .unwrap_or_else(|| panic!("missing model {id}"))
}

fn backup_files(env: &TestEnv) -> Vec<PathBuf> {
    let directory = env.xdg_state.join("tokencamp/backups/pi");
    let mut paths: Vec<_> = match fs::read_dir(&directory) {
        Ok(entries) => entries
            .map(|entry| entry.expect("read backup entry").path())
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("read backup directory: {error}"),
    };
    paths.sort();
    paths
}

#[test]
fn configure_writes_models_json_with_the_provider_and_five_models() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains(&format!("Configured pi: {}", models_path(&env).display())),
        "stdout: {}",
        stdout(&output)
    );

    let models = models_value(&env);
    assert_eq!(
        models["$schema"],
        "https://pi.dev/schemas/models.schema.json"
    );
    let provider = provider(&models);
    assert_eq!(provider["name"], "tokencamp");
    assert_eq!(provider["baseUrl"], format!("{}/v1", server.url("")));
    assert_eq!(provider["api"], "openai-completions");
    assert_eq!(provider["apiKey"], "$TOKENCAMP_API_KEY");
    assert_eq!(provider["compat"]["maxTokensField"], "max_tokens");
    assert_eq!(provider["compat"]["supportsDeveloperRole"], false);
    assert_eq!(provider["compat"]["supportsStore"], false);
    assert_eq!(provider["compat"]["supportsReasoningEffort"], false);

    let models_list = provider["models"].as_array().expect("models array");
    let ids: Vec<&str> = models_list
        .iter()
        .map(|model| model["id"].as_str().expect("model id"))
        .collect();
    assert_eq!(
        ids,
        vec![
            "zhipu/glm-5.3",
            "deepseek/deepseek-v4-pro",
            "zhipu/glm-5.3-flash",
            "kimi/kimi-k2.7-code",
            "tokencamp/auto",
        ]
    );

    for (id, context, max) in [
        ("zhipu/glm-5.3", 1_048_576, 32768),
        ("deepseek/deepseek-v4-pro", 1_048_576, 32768),
        ("zhipu/glm-5.3-flash", 1_048_576, 32768),
        ("kimi/kimi-k2.7-code", 262_144, 32768),
        ("tokencamp/auto", 200_000, 8192),
    ] {
        let entry = model(provider, id);
        assert_eq!(entry["contextWindow"], context, "context for {id}");
        assert_eq!(entry["maxTokens"], max, "max tokens for {id}");
        assert!(
            entry["name"].as_str().is_some_and(|name| !name.is_empty()),
            "name for {id}"
        );
        assert_eq!(entry["compat"]["maxTokensField"], "max_tokens", "{id}");
        assert_eq!(entry["compat"]["supportsDeveloperRole"], false, "{id}");
        assert_eq!(entry["compat"]["supportsStore"], false, "{id}");
        assert_eq!(entry["compat"]["supportsReasoningEffort"], false, "{id}");
        assert!(
            entry.get("cost").is_none(),
            "cost must not be written for {id}"
        );
    }

    assert_eq!(
        model(provider, "zhipu/glm-5.3")["input"],
        serde_json::json!(["text"])
    );
    assert_eq!(
        model(provider, "zhipu/glm-5.3-flash")["input"],
        serde_json::json!(["text", "image"])
    );

    let settings = settings_value(&env);
    assert_eq!(settings["defaultProvider"], "tokencamp");
    assert_eq!(settings["defaultModel"], "zhipu/glm-5.3");

    assert!(!env.pi_agent.join("models-store.json").exists());
    assert!(!env.pi_auth_path().exists());
    assert!(!env.root.join(".pi").exists());
    assert!(!models_text(&env).contains(KEY));
}

#[test]
fn existing_defaults_are_preserved_and_empty_defaults_are_filled() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    fs::write(
        settings_path(&env),
        r#"{"defaultProvider":"other","defaultModel":"other/model","theme":"dark"}"#,
    )
    .expect("write settings");

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let settings = settings_value(&env);
    assert_eq!(settings["defaultProvider"], "other");
    assert_eq!(settings["defaultModel"], "other/model");
    assert_eq!(settings["theme"], "dark");

    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    fs::write(
        settings_path(&env),
        r#"{"defaultProvider":"","defaultModel":"","theme":"dark"}"#,
    )
    .expect("write settings");

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let settings = settings_value(&env);
    assert_eq!(settings["defaultProvider"], "tokencamp");
    assert_eq!(settings["defaultModel"], "zhipu/glm-5.3");
    assert_eq!(settings["theme"], "dark");
}

#[test]
fn configure_preserves_jsonc_comments_trailing_commas_and_unknown_fields() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let existing = r#"// top comment
{
  // providers comment
  "providers": {
    "other": { "keep": true }, // keep other
    "tokencamp": {
      // managed by tcamp from here on
      "name": "tokencamp",
      "baseUrl": "https://old.example/v1",
      "api": "openai-completions",
      "apiKey": "$TOKENCAMP_API_KEY",
      "custom": "keep-me",
      "compat": {
        "maxTokensField": "max_tokens",
        "supportsDeveloperRole": false,
        "supportsStore": false,
        "supportsReasoningEffort": false,
        "customCompat": "keep-me",
      },
      "models": [
        {
          "id": "zhipu/glm-5.3",
          "name": "user name",
          "custom": "keep-me",
          "compat": {
            "maxTokensField": "max_tokens",
            "supportsDeveloperRole": false,
            "supportsStore": false,
            "supportsReasoningEffort": false,
          },
        },
        { "id": "user/extra", "name": "extra" },
      ],
    },
  },
}
"#;
    fs::write(models_path(&env), existing).expect("write models.json");

    let output = run(&env, &["pi", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = models_text(&env);
    for marker in [
        "// top comment",
        "// providers comment",
        "// keep other",
        "// managed by tcamp from here on",
        "\"custom\": \"keep-me\"",
        "\"customCompat\": \"keep-me\"",
        "\"user/extra\"",
    ] {
        assert!(text.contains(marker), "missing {marker:?} in:\n{text}");
    }
    assert!(
        !text.contains("\"$schema\""),
        "an existing file must not gain $schema:\n{text}"
    );
    assert!(text.contains("    \"other\": { \"keep\": true },"));
    assert!(
        !text.contains("\"name\": \"user name\""),
        "managed model fields must be converged:\n{text}"
    );

    let models = models_value(&env);
    let provider = provider(&models);
    assert_eq!(provider["baseUrl"], format!("{}/v1", server.url("")));
    assert_eq!(provider["custom"], "keep-me");
    assert_eq!(provider["compat"]["customCompat"], "keep-me");
    assert_eq!(model(provider, "zhipu/glm-5.3")["custom"], "keep-me");
    assert_eq!(model(provider, "user/extra")["name"], "extra");
    assert_eq!(
        provider["models"].as_array().expect("models").len(),
        6,
        "the user model must be preserved alongside the five managed models"
    );
}

#[test]
fn corrupt_jsonc_is_rejected_with_a_location_and_never_rewritten() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let corrupt = "{\n  \"providers\": {\n";
    fs::write(models_path(&env), corrupt).expect("write corrupt models.json");

    for args in [
        vec!["pi", "--no-launch"],
        vec!["pi", "--no-launch", "--force"],
        vec!["pi", "--no-launch", "--dry-run"],
    ] {
        let output = run(&env, &args);
        assert_eq!(output.status.code(), Some(2), "args: {args:?}");
        let message = stderr(&output);
        assert!(
            message.contains("config_unreadable"),
            "args: {args:?}: {message}"
        );
        assert!(
            message.contains(&models_path(&env).display().to_string()),
            "args: {args:?}: {message}"
        );
        assert!(message.contains("line "), "args: {args:?}: {message}");
        assert!(message.contains("column "), "args: {args:?}: {message}");
        assert_eq!(
            fs::read_to_string(models_path(&env)).expect("read models"),
            corrupt
        );
    }
    assert!(backup_files(&env).is_empty());
}

#[test]
fn an_unowned_provider_entry_is_a_conflict_until_forced() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let existing = r#"{
  "providers": {
    "tokencamp": {
      "name": "mine",
      "baseUrl": "https://user.example/v1",
      "api": "openai-completions",
      "unknown": "keep-me"
    }
  }
}
"#;
    fs::write(models_path(&env), existing).expect("write models.json");

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_conflict"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(models_path(&env)).expect("read models"),
        existing
    );
    assert!(backup_files(&env).is_empty());
    assert!(
        !settings_path(&env).exists(),
        "a conflict must not write the defaults file"
    );

    let output = run(&env, &["pi", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let backups = backup_files(&env);
    assert_eq!(backups.len(), 1, "backups: {backups:?}");
    assert_eq!(
        fs::read_to_string(&backups[0]).expect("read backup"),
        existing
    );
    let models = models_value(&env);
    assert_eq!(provider(&models)["unknown"], "keep-me");
    assert_eq!(provider(&models)["name"], "tokencamp");
    assert_eq!(
        provider(&models)["baseUrl"],
        format!("{}/v1", server.url(""))
    );

    let manifest =
        fs::read_to_string(env.xdg_config.join("tokencamp/managed.toml")).expect("read manifest");
    assert!(manifest.contains("adopted = true"), "manifest: {manifest}");
}

#[test]
fn a_base_url_change_by_tcamp_is_not_reported_as_drift() {
    let env = TestEnv::new();
    let first = all_model_server();
    login(&env, &first.url(""));
    run(&env, &["pi", "--no-launch"]);

    let second = all_model_server();
    let output = env
        .command()
        .args(["pi", "--no-launch", "--base-url", &second.url("")])
        .env("TOKENCAMP_API_KEY", KEY)
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        !stderr(&output).contains("config_drift"),
        "a base URL change requested by tcamp is not drift: {}",
        stderr(&output)
    );
    assert_eq!(
        provider(&models_value(&env))["baseUrl"],
        format!("{}/v1", second.url(""))
    );
}

#[test]
fn adoption_only_claims_the_models_that_tcamp_creates() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    let existing = r#"{
  "providers": {
    "tokencamp": {
      "name": "mine",
      "baseUrl": "https://user.example/v1",
      "api": "openai-completions",
      "apiKey": "$TOKENCAMP_API_KEY",
      "models": [
        { "id": "zhipu/glm-5.3", "name": "user model", "custom": "keep-me" }
      ]
    }
  }
}
"#;
    fs::write(models_path(&env), existing).expect("write models.json");

    let output = run(&env, &["pi", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let manifest: toml_edit::DocumentMut =
        fs::read_to_string(env.xdg_config.join("tokencamp/managed.toml"))
            .expect("read manifest")
            .parse()
            .expect("parse manifest");
    let key = models_path(&env).display().to_string();
    let ids: Vec<&str> = manifest["targets"].as_table().expect("targets table")[key.as_str()]
        .as_table()
        .expect("models entry")["model_ids"]
        .as_array()
        .expect("model ids")
        .iter()
        .map(|value| value.as_str().expect("model id"))
        .collect();
    assert_eq!(
        ids,
        vec![
            "deepseek/deepseek-v4-pro",
            "zhipu/glm-5.3-flash",
            "kimi/kimi-k2.7-code",
            "tokencamp/auto",
        ],
        "the pre-existing model must not be claimed as tcamp-created"
    );
    assert_eq!(
        model(provider(&models_value(&env)), "zhipu/glm-5.3")["custom"],
        "keep-me"
    );
}

#[test]
fn configure_records_ownership_and_reports_drift() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["pi", "--no-launch"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let manifest: toml_edit::DocumentMut =
        fs::read_to_string(env.xdg_config.join("tokencamp/managed.toml"))
            .expect("read manifest")
            .parse()
            .expect("parse manifest");
    let key = models_path(&env).display().to_string();
    let entry = manifest["targets"].as_table().expect("targets table")[key.as_str()]
        .as_table()
        .unwrap_or_else(|| panic!("missing manifest entry for {key}"));
    assert_eq!(entry["harness"].as_str(), Some("pi"));
    assert_eq!(entry["provider_entry"].as_str(), Some("tokencamp"));
    assert_eq!(entry["created_file"].as_bool(), Some(true));
    let ids: Vec<&str> = entry["model_ids"]
        .as_array()
        .expect("model ids")
        .iter()
        .map(|value| value.as_str().expect("model id"))
        .collect();
    assert_eq!(
        ids,
        vec![
            "zhipu/glm-5.3",
            "deepseek/deepseek-v4-pro",
            "zhipu/glm-5.3-flash",
            "kimi/kimi-k2.7-code",
            "tokencamp/auto",
        ]
    );
    assert!(
        !entry["last_written_hash"]
            .as_str()
            .expect("hash")
            .is_empty()
    );

    let edited = models_text(&env).replace(
        &format!("{}/v1", server.url("")),
        "https://drifted.example/v1",
    );
    assert_ne!(edited, models_text(&env));
    fs::write(models_path(&env), &edited).expect("edit models.json");

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_drift"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        provider(&models_value(&env))["baseUrl"],
        format!("{}/v1", server.url(""))
    );
}

#[test]
fn a_missing_manifest_cannot_take_ownership_of_an_existing_entry() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    run(&env, &["pi", "--no-launch"]);
    fs::remove_file(env.xdg_config.join("tokencamp/managed.toml")).expect("delete manifest");

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_conflict"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn dry_run_plans_both_files_without_writing() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["pi", "--no-launch", "--dry-run"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("--- "), "stdout: {text}");
    assert!(text.contains("+++ "), "stdout: {text}");
    assert!(text.contains("\"openai-completions\""), "stdout: {text}");
    assert!(text.contains("\"$TOKENCAMP_API_KEY\""), "stdout: {text}");
    assert!(stderr(&output).contains("no files written"));
    assert!(!models_path(&env).exists());
    assert!(!settings_path(&env).exists());
    assert!(!env.xdg_config.join("tokencamp/managed.toml").exists());
    assert!(!env.xdg_state.join("tokencamp").exists());
    assert!(server.requests().is_empty(), "dry-run must stay offline");

    let output = run(&env, &["pi", "--no-launch", "--dry-run", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
    let value: Value = serde_json::from_str(&stdout(&output)).expect("one JSON object");
    assert_eq!(value["ok"], true);
    let changes = value["changes"].as_array().expect("changes array");
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["path"], models_path(&env).display().to_string());
    assert_eq!(changes[0]["action"], "create");
    assert_eq!(
        changes[1]["path"],
        settings_path(&env).display().to_string()
    );
    assert_eq!(changes[1]["action"], "create");
    assert!(!stdout(&output).contains(KEY));
}

#[test]
fn configure_falls_back_to_home_agent_dir_when_the_override_is_unset() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));

    let output = env
        .command()
        .args(["pi", "--no-launch"])
        .env_remove("PI_CODING_AGENT_DIR")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(env.home.join(".pi/agent/models.json").exists());
    assert!(env.home.join(".pi/agent/settings.json").exists());
}

#[test]
fn configure_does_not_write_project_or_store_files() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let project = env.root.join("project");
    fs::create_dir_all(&project).expect("create project directory");

    let output = env
        .command()
        .current_dir(&project)
        .args(["pi", "--no-launch"])
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        fs::read_dir(&project).expect("read project").count(),
        0,
        "project-level configuration must not be written"
    );
    assert!(!env.pi_agent.join("models-store.json").exists());
}

#[test]
fn a_missing_model_exits_2_without_writing() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(vec![(
        "/v1/models/zhipu/glm-5.3",
        MockResponse::json(404, r#"{"error":{"code":"model_not_found"}}"#),
    )]);
    login(&env, &server.url(""));

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("zhipu/glm-5.3"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!models_path(&env).exists());
}

#[test]
fn a_model_check_network_failure_warns_and_continues() {
    let env = TestEnv::new();
    let base_url = env.unused_loopback_url();
    login(&env, &base_url);

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("model_check_failed"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(models_path(&env).exists());
}

fn all_model_server() -> MockServer {
    let mut routes = support::model_routes();
    routes.push((
        "/v1/models/tokencamp/auto",
        MockResponse::json(200, r#"{"id":"model"}"#),
    ));
    MockServer::start_routes(routes)
}

#[test]
fn launch_execs_pi_with_the_provider_model_and_injected_key() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    let pi = env.fake_harness("pi", "1.4.0");

    let output = run(&env, &["pi"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        pi.args(),
        vec![
            "--provider".to_string(),
            "tokencamp".to_string(),
            "--model".to_string(),
            "zhipu/glm-5.3".to_string(),
        ]
    );
    assert_eq!(pi.env_value("TOKENCAMP_API_KEY").as_deref(), Some(KEY));
    assert!(
        stderr(&output).contains("Launching pi ..."),
        "stderr: {}",
        stderr(&output)
    );
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!combined.contains(KEY), "output leaked the key: {combined}");
    assert!(!models_text(&env).contains(KEY));
    assert!(
        !fs::read_to_string(settings_path(&env))
            .expect("read settings")
            .contains(KEY)
    );
    assert!(!env.pi_auth_path().exists());
}

#[test]
fn no_launch_writes_the_config_even_when_pi_is_missing() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(models_path(&env).exists());
    assert!(settings_path(&env).exists());
}

#[test]
fn launch_requires_the_binary_without_no_launch() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["pi"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("harness_not_found"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        server.requests().is_empty(),
        "the binary check must run before the model checks"
    );
    assert!(!models_path(&env).exists());
    assert!(!settings_path(&env).exists());
}

#[test]
fn model_aliases_resolve_to_the_exposed_canonical_ids() {
    for (alias, canonical) in [
        ("glm-5.3", "zhipu/glm-5.3"),
        ("deepseek-v4-pro", "deepseek/deepseek-v4-pro"),
        ("glm-5.3-flash", "zhipu/glm-5.3-flash"),
        ("kimi-k2.7-code", "kimi/kimi-k2.7-code"),
        ("auto", "tokencamp/auto"),
        ("zhipu/glm-5.3", "zhipu/glm-5.3"),
        ("tokencamp/auto", "tokencamp/auto"),
    ] {
        let env = TestEnv::new();
        let server = all_model_server();
        login(&env, &server.url(""));
        let pi = env.fake_harness("pi", "1.4.0");

        let output = run(&env, &["pi", "--model", alias]);

        assert_eq!(
            output.status.code(),
            Some(0),
            "alias {alias}: stderr: {}",
            stderr(&output)
        );
        assert_eq!(
            pi.args(),
            vec![
                "--provider".to_string(),
                "tokencamp".to_string(),
                "--model".to_string(),
                canonical.to_string(),
            ],
            "alias {alias}"
        );
    }
}

#[test]
fn an_invalid_model_exits_2_and_lists_the_five_exposed_models() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    let pi = env.fake_harness("pi", "1.4.0");

    for invalid in ["", "not-a-model", "@preset/fast", "~family-latest"] {
        let output = run(&env, &["pi", "--model", invalid]);

        assert_eq!(
            output.status.code(),
            Some(2),
            "model {invalid:?}: stderr: {}",
            stderr(&output)
        );
        let message = stderr(&output);
        assert!(message.contains("[usage]"), "model {invalid:?}: {message}");
        for available in [
            "zhipu/glm-5.3",
            "deepseek/deepseek-v4-pro",
            "zhipu/glm-5.3-flash",
            "kimi/kimi-k2.7-code",
            "tokencamp/auto",
        ] {
            assert!(
                message.contains(available),
                "model {invalid:?} must list {available}: {message}"
            );
        }
    }

    assert!(!pi.ran());
    assert!(!models_path(&env).exists());
    assert!(server.requests().is_empty());
}

#[test]
fn tokencamp_model_overrides_the_launch_and_an_empty_value_is_ignored() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    let pi = env.fake_harness("pi", "1.4.0");

    let output = env
        .command()
        .args(["pi"])
        .env("TOKENCAMP_MODEL", "kimi/kimi-k2.7-code")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        pi.args(),
        vec![
            "--provider".to_string(),
            "tokencamp".to_string(),
            "--model".to_string(),
            "kimi/kimi-k2.7-code".to_string(),
        ]
    );

    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    let pi = env.fake_harness("pi", "1.4.0");
    let output = env
        .command()
        .args(["pi"])
        .env("TOKENCAMP_MODEL", "")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        pi.args(),
        vec![
            "--provider".to_string(),
            "tokencamp".to_string(),
            "--model".to_string(),
            "zhipu/glm-5.3".to_string(),
        ]
    );
}

#[test]
fn the_model_flag_wins_over_the_environment() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    let pi = env.fake_harness("pi", "1.4.0");

    let output = env
        .command()
        .args(["pi", "--model", "auto"])
        .env("TOKENCAMP_MODEL", "kimi/kimi-k2.7-code")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(pi.args()[3], "tokencamp/auto");
}

#[test]
fn the_model_override_does_not_change_the_config() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    env.fake_harness("pi", "1.4.0");

    let output = run(&env, &["pi", "--model", "auto"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(settings_value(&env)["defaultModel"], "zhipu/glm-5.3");
    let models = models_value(&env);
    assert_eq!(
        provider(&models)["models"]
            .as_array()
            .expect("models")
            .len(),
        5
    );
    assert_eq!(
        model(provider(&models), "zhipu/glm-5.3")["contextWindow"],
        1_048_576
    );

    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    let output = run(&env, &["pi", "--no-launch", "--model", "auto"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(settings_value(&env)["defaultModel"], "zhipu/glm-5.3");
}

#[test]
fn a_version_below_the_floor_warns_but_still_launches() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    let pi = env.fake_harness("pi", "1.3.9");

    let output = run(&env, &["pi"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(pi.ran());
    let message = stderr(&output);
    assert!(
        message.contains("below the tested version"),
        "stderr: {message}"
    );
    assert!(message.contains("1.4.0"), "stderr: {message}");
}

#[test]
fn a_version_at_the_floor_does_not_warn() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    env.fake_harness("pi", "1.4.0");

    let output = run(&env, &["pi"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        !stderr(&output).contains("below the tested version"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn preflight_checks_only_the_launch_model() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    env.fake_harness("pi", "1.4.0");

    let output = run(&env, &["pi"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let paths: Vec<String> = server
        .requests()
        .into_iter()
        .map(|request| request.path)
        .collect();
    assert_eq!(paths, vec!["/v1/models/zhipu/glm-5.3".to_string()]);

    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    env.fake_harness("pi", "1.4.0");

    let output = run(&env, &["pi", "--model", "auto"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let paths: Vec<String> = server
        .requests()
        .into_iter()
        .map(|request| request.path)
        .collect();
    assert_eq!(paths, vec!["/v1/models/tokencamp/auto".to_string()]);
}

#[test]
fn dry_run_does_not_contact_the_models_endpoint() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["pi", "--dry-run"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(server.requests().is_empty(), "dry-run must stay offline");
    assert!(!models_path(&env).exists());
}

#[test]
fn dry_run_reports_an_unowned_conflict_unless_forced() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    fs::write(
        models_path(&env),
        r#"{"providers":{"tokencamp":{"name":"mine"}}}"#,
    )
    .expect("write models.json");

    let output = run(&env, &["pi", "--no-launch", "--dry-run"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_conflict"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["pi", "--no-launch", "--dry-run", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("+++ "),
        "stdout: {}",
        stdout(&output)
    );
    assert!(backup_files(&env).is_empty());
}

#[test]
fn corrupt_settings_are_rejected_before_models_are_written() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    fs::write(settings_path(&env), "{\n  \"defaultProvider\":\n").expect("write settings");

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("config_unreadable"), "stderr: {message}");
    assert!(
        message.contains(&settings_path(&env).display().to_string()),
        "stderr: {message}"
    );
    assert!(message.contains("line "), "stderr: {message}");
    assert!(!models_path(&env).exists());
}

#[test]
fn configure_json_reports_create_then_none() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["pi", "--no-launch", "--json"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
    let value: Value = serde_json::from_str(&stdout(&output)).expect("one JSON object");
    assert_eq!(value["ok"], true);
    assert_eq!(value["key"]["masked"], "sk-…cdef");
    assert_eq!(
        value["harnesses"]["pi"]["target"],
        models_path(&env).display().to_string()
    );
    assert_eq!(value["harnesses"]["pi"]["action"], "create");

    let output = run(&env, &["pi", "--no-launch", "--json"]);
    let value: Value = serde_json::from_str(&stdout(&output)).expect("one JSON object");
    assert_eq!(value["harnesses"]["pi"]["action"], "none");
    assert!(!stdout(&output).contains(KEY));
}

#[test]
fn the_manifest_records_the_defaults_that_were_written() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["pi", "--no-launch"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let manifest: toml_edit::DocumentMut =
        fs::read_to_string(env.xdg_config.join("tokencamp/managed.toml"))
            .expect("read manifest")
            .parse()
            .expect("parse manifest");
    let key = settings_path(&env).display().to_string();
    let entry = manifest["targets"].as_table().expect("targets table")[key.as_str()]
        .as_table()
        .unwrap_or_else(|| panic!("missing manifest entry for {key}"));
    assert_eq!(entry["harness"].as_str(), Some("pi"));
    assert_eq!(
        entry["defaults"]["defaultProvider"].as_str(),
        Some("tokencamp")
    );
    assert_eq!(
        entry["defaults"]["defaultModel"].as_str(),
        Some("zhipu/glm-5.3")
    );
}

#[test]
fn a_second_configure_is_quiet_and_writes_no_backup() {
    let env = TestEnv::new();
    let server = all_model_server();
    login(&env, &server.url(""));
    run(&env, &["pi", "--no-launch"]);

    let output = run(&env, &["pi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("No changes."),
        "stdout: {}",
        stdout(&output)
    );
    assert!(backup_files(&env).is_empty());
}
