mod support;

use std::fs;
use std::path::PathBuf;
use std::process::Output;

use support::{MockResponse, MockServer, TestEnv, model_server};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";
const BASE_URL: &str = "https://tokencamp.example";

const EXPECTED_OVERLAY: &str = r#"{
  "$schema": "https://json.schemastore.org/claude-code-settings.json",
  "_tcamp": {
    "version": 1,
    "harness": "claude"
  },
  "env": {
    "ANTHROPIC_BASE_URL": "https://tokencamp.example",
    "ANTHROPIC_API_KEY": "",
    "ANTHROPIC_MODEL": "zhipu/glm-5.3",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "deepseek/deepseek-v4-pro",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "zhipu/glm-5.3",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "zhipu/glm-5.3-flash",
    "ANTHROPIC_DEFAULT_FABLE_MODEL": "deepseek/deepseek-v4-pro",
    "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "1048576",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "32768",
    "ENABLE_TOOL_SEARCH": "false",
    "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS": "1",
    "MAX_THINKING_TOKENS": "0"
  }
}
"#;

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

fn overlay_path(env: &TestEnv) -> PathBuf {
    env.claude_overlay_path()
}

fn overlay_value(env: &TestEnv) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(overlay_path(env)).expect("read overlay"))
        .expect("overlay is JSON")
}

#[test]
fn launch_execs_claude_with_the_overlay_and_injected_credentials() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(vec![
        ("/v1/models/zhipu/glm-5.3", MockResponse::json(200, "{}")),
        (
            "/v1/models/deepseek/deepseek-v4-pro",
            MockResponse::json(200, "{}"),
        ),
        (
            "/v1/models/zhipu/glm-5.3-flash",
            MockResponse::json(200, "{}"),
        ),
    ]);
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        claude.args(),
        vec![
            "--settings".to_string(),
            overlay_path(&env).display().to_string()
        ]
    );
    assert_eq!(claude.env_value("TOKENCAMP_API_KEY").as_deref(), Some(KEY));
    assert_eq!(
        claude.env_value("ANTHROPIC_AUTH_TOKEN").as_deref(),
        Some(KEY)
    );
    assert_eq!(claude.env_value("ANTHROPIC_API_KEY").as_deref(), Some(""));

    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!combined.contains(KEY), "output leaked the key: {combined}");
    assert!(
        stderr(&output).contains("Launching claude ..."),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("/v1/messages capability unverified"),
        "stderr: {}",
        stderr(&output)
    );

    let overlay = fs::read_to_string(overlay_path(&env)).expect("read overlay");
    assert!(!overlay.contains(KEY));
    assert!(!overlay.contains("ANTHROPIC_AUTH_TOKEN"));
    assert!(
        !env.claude_config.join("settings.json").exists(),
        "the user's Claude settings must not be touched"
    );
}

#[test]
fn launch_requires_the_binary_without_no_launch() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("harness_not_found"), "stderr: {message}");
    assert!(message.contains("https://"), "stderr: {message}");
    assert!(
        server.requests().is_empty(),
        "the binary check must run before the model checks"
    );
    assert!(!overlay_path(&env).exists());
}

#[test]
fn no_launch_writes_the_config_even_when_the_binary_is_missing() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(overlay_path(&env).exists());
}

#[test]
fn arguments_after_double_dash_reach_the_harness_verbatim() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness_with_exit_code("claude", "2.1.316", 7);

    let output = run(&env, &["claude", "--", "--foo", "bar baz"]);

    assert_eq!(output.status.code(), Some(7), "stderr: {}", stderr(&output));
    assert_eq!(
        claude.args(),
        vec![
            "--settings".to_string(),
            overlay_path(&env).display().to_string(),
            "--foo".to_string(),
            "bar baz".to_string()
        ]
    );
}

#[test]
fn a_launch_reports_configured_once_then_stays_quiet() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    env.fake_harness("claude", "2.1.316");

    let first = run(&env, &["claude"]);
    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));
    assert!(
        stderr(&first).contains(&format!(
            "Configured claude: {}",
            overlay_path(&env).display()
        )),
        "stderr: {}",
        stderr(&first)
    );

    let second = run(&env, &["claude"]);
    assert_eq!(second.status.code(), Some(0), "stderr: {}", stderr(&second));
    assert!(
        !stderr(&second).contains("Configured claude:"),
        "stderr: {}",
        stderr(&second)
    );
    assert!(
        stderr(&second).contains("Launching claude ..."),
        "stderr: {}",
        stderr(&second)
    );
}

#[test]
fn model_override_changes_the_launch_but_not_the_overlay() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");

    let output = run(&env, &["claude", "--model", "kimi/kimi-k2.7-code"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        claude.env_value("ANTHROPIC_MODEL").as_deref(),
        Some("kimi/kimi-k2.7-code")
    );
    assert_eq!(
        claude
            .env_value("CLAUDE_CODE_MAX_CONTEXT_TOKENS")
            .as_deref(),
        Some("262144")
    );
    assert_eq!(
        claude.args(),
        vec![
            "--settings".to_string(),
            overlay_path(&env).display().to_string(),
            "--model".to_string(),
            "kimi/kimi-k2.7-code".to_string()
        ]
    );
    let overlay = overlay_value(&env);
    let values = overlay["env"].as_object().expect("env object");
    assert_eq!(values["ANTHROPIC_MODEL"], "zhipu/glm-5.3");
    assert_eq!(
        values["ANTHROPIC_DEFAULT_OPUS_MODEL"],
        "deepseek/deepseek-v4-pro"
    );
    assert_eq!(values["ANTHROPIC_DEFAULT_SONNET_MODEL"], "zhipu/glm-5.3");
    assert_eq!(
        values["ANTHROPIC_DEFAULT_HAIKU_MODEL"],
        "zhipu/glm-5.3-flash"
    );
    assert_eq!(
        values["ANTHROPIC_DEFAULT_FABLE_MODEL"],
        "deepseek/deepseek-v4-pro"
    );
}

#[test]
fn invalid_model_values_exit_2_and_list_the_available_ids() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");

    for model in [
        "",
        "auto",
        "tokencamp/auto",
        "@preset/fast",
        "~family-latest",
        "not-a-model",
    ] {
        let output = run(&env, &["claude", "--model", model]);

        assert_eq!(
            output.status.code(),
            Some(2),
            "model {model:?}: stderr: {}",
            stderr(&output)
        );
        let message = stderr(&output);
        assert!(message.contains("[usage]"), "model {model:?}: {message}");
        for available in [
            "zhipu/glm-5.3",
            "deepseek/deepseek-v4-pro",
            "zhipu/glm-5.3-flash",
            "kimi/kimi-k2.7-code",
        ] {
            assert!(
                message.contains(available),
                "model {model:?} must list {available}: {message}"
            );
        }
    }

    assert!(!claude.ran());
    assert!(!overlay_path(&env).exists());
    assert!(server.requests().is_empty());
}

#[test]
fn tokencamp_model_overrides_the_launch() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");

    let output = env
        .command()
        .args(["claude"])
        .env("TOKENCAMP_MODEL", "kimi/kimi-k2.7-code")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        claude.env_value("ANTHROPIC_MODEL").as_deref(),
        Some("kimi/kimi-k2.7-code")
    );
    assert_eq!(
        overlay_value(&env)["env"]["ANTHROPIC_MODEL"],
        "zhipu/glm-5.3"
    );
}

#[test]
fn the_model_flag_wins_over_the_environment_and_an_empty_environment_value_is_ignored() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");

    let output = env
        .command()
        .args(["claude", "--model", "kimi/kimi-k2.7-code"])
        .env("TOKENCAMP_MODEL", "zhipu/glm-5.3-flash")
        .output()
        .expect("run tcamp");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        claude.env_value("ANTHROPIC_MODEL").as_deref(),
        Some("kimi/kimi-k2.7-code")
    );

    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let output = env
        .command()
        .args(["claude"])
        .env("TOKENCAMP_MODEL", "")
        .output()
        .expect("run tcamp");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(claude.env_value("ANTHROPIC_MODEL"), None);
}

#[test]
fn no_launch_does_not_write_the_model_override() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));

    let output = run(
        &env,
        &["claude", "--no-launch", "--model", "kimi/kimi-k2.7-code"],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        overlay_value(&env)["env"]["ANTHROPIC_MODEL"],
        "zhipu/glm-5.3"
    );
}

#[test]
fn configure_writes_the_claude_overlay_on_first_run() {
    let env = TestEnv::new();
    login(&env, BASE_URL);

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains(&format!(
            "Configured claude: {}",
            overlay_path(&env).display()
        )),
        "stdout: {}",
        stdout(&output)
    );

    let overlay = fs::read_to_string(overlay_path(&env)).expect("read overlay");
    assert_eq!(overlay, EXPECTED_OVERLAY);
    assert!(!overlay.contains(KEY));
    assert!(!overlay.contains("ANTHROPIC_AUTH_TOKEN"));

    assert!(
        !env.claude_config.join("settings.json").exists(),
        "the user's Claude settings must not be touched"
    );
}

#[test]
fn configure_without_a_key_exits_2_and_writes_nothing() {
    let env = TestEnv::new();

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stdout(&output).is_empty());
    assert!(
        stderr(&output).contains("missing_key"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("tcamp login --key-stdin"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!overlay_path(&env).exists());
    assert!(!env.xdg_config.join("tokencamp").exists());
}

#[cfg(unix)]
#[test]
fn configure_creates_private_directories_and_files() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);

    let mode = |path: &std::path::Path| {
        fs::metadata(path)
            .unwrap_or_else(|_| panic!("metadata for {}", path.display()))
            .permissions()
            .mode()
            & 0o777
    };

    assert_eq!(mode(&env.xdg_config.join("tokencamp")), 0o700);
    assert_eq!(mode(&env.xdg_config.join("tokencamp/harnesses")), 0o700);
    assert_eq!(
        mode(&env.xdg_config.join("tokencamp/harnesses/claude")),
        0o700
    );
    assert_eq!(mode(&overlay_path(&env)), 0o600);
}

#[test]
fn configure_does_not_rewrite_an_identical_overlay() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout(&output).contains("No changes."),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        !env.xdg_state.join("tokencamp/backups/claude").exists(),
        "an unchanged configure must not create a backup"
    );
}

#[cfg(unix)]
#[test]
fn configure_backs_up_an_owned_overlay_before_updating_it() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);

    let original = fs::read_to_string(overlay_path(&env)).expect("read overlay");
    let edited = original.replace(
        "\"MAX_THINKING_TOKENS\": \"0\"",
        "\"MAX_THINKING_TOKENS\": \"1\"",
    );
    assert_ne!(edited, original);
    fs::write(overlay_path(&env), &edited).expect("edit overlay");

    let output = run(&env, &["claude", "--no-launch"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    assert_eq!(
        fs::read_to_string(overlay_path(&env)).expect("read overlay"),
        EXPECTED_OVERLAY
    );

    let backups = backup_files(&env);
    assert_eq!(backups.len(), 1, "backups: {backups:?}");
    assert_eq!(
        fs::read_to_string(&backups[0]).expect("read backup"),
        edited
    );
    let mode = fs::metadata(&backups[0])
        .expect("backup metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[cfg(unix)]
#[test]
fn configure_preserves_the_mode_of_an_existing_overlay() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);

    fs::set_permissions(overlay_path(&env), fs::Permissions::from_mode(0o640))
        .expect("chmod overlay");
    let original = fs::read_to_string(overlay_path(&env)).expect("read overlay");
    fs::write(
        overlay_path(&env),
        original.replace(
            "\"MAX_THINKING_TOKENS\": \"0\"",
            "\"MAX_THINKING_TOKENS\": \"1\"",
        ),
    )
    .expect("edit overlay");

    run(&env, &["claude", "--no-launch"]);

    let mode = fs::metadata(overlay_path(&env))
        .expect("overlay metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o640);
}

#[test]
fn configure_refuses_an_unowned_overlay_without_force() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    let user_overlay = r#"{"env": {"FOO": "bar"}}"#;
    write_overlay(&env, user_overlay);

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2));
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
        fs::read_to_string(overlay_path(&env)).expect("read overlay"),
        user_overlay
    );
    assert!(backup_files(&env).is_empty());
}

#[test]
fn yes_does_not_bypass_a_conflict() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    write_overlay(&env, r#"{"env": {"FOO": "bar"}}"#);

    let output = run(&env, &["claude", "--no-launch", "--yes"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn configure_adopts_an_unowned_overlay_with_force() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    let user_overlay = "{\"env\": {\"FOO\": \"bar\"}}\n";
    write_overlay(&env, user_overlay);

    let output = run(&env, &["claude", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        fs::read_to_string(overlay_path(&env)).expect("read overlay"),
        EXPECTED_OVERLAY
    );
    let backups = backup_files(&env);
    assert_eq!(backups.len(), 1);
    assert_eq!(
        fs::read_to_string(&backups[0]).expect("read backup"),
        user_overlay
    );
}

#[test]
fn configure_rejects_an_unknown_marker_version() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    let foreign = "{\"_tcamp\": {\"version\": 2, \"harness\": \"claude\"}}\n";
    write_overlay(&env, foreign);

    let output = run(&env, &["claude", "--no-launch"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["claude", "--no-launch", "--force"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        fs::read_to_string(overlay_path(&env)).expect("read overlay"),
        EXPECTED_OVERLAY
    );
}

#[test]
fn dry_run_prints_a_diff_without_writing() {
    let env = TestEnv::new();
    login(&env, BASE_URL);

    let output = run(&env, &["claude", "--no-launch", "--dry-run"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("--- "), "stdout: {text}");
    assert!(text.contains("+++ "), "stdout: {text}");
    assert!(
        text.lines().any(|line| line
            == "+  \"$schema\": \"https://json.schemastore.org/claude-code-settings.json\","),
        "stdout: {text}"
    );
    assert!(
        stderr(&output).contains("no files written"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!overlay_path(&env).exists());
    assert!(!env.xdg_config.join("tokencamp/harnesses").exists());
    assert!(!env.xdg_state.join("tokencamp").exists());
}

#[test]
fn dry_run_does_not_require_no_launch() {
    let env = TestEnv::new();
    login(&env, BASE_URL);

    let output = run(&env, &["claude", "--dry-run"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("+++ "),
        "stdout: {}",
        stdout(&output)
    );
    assert!(!overlay_path(&env).exists());
}

#[test]
fn dry_run_json_returns_the_planned_change() {
    let env = TestEnv::new();
    login(&env, BASE_URL);

    let output = run(&env, &["claude", "--no-launch", "--dry-run", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("stdout is one JSON object");
    assert_eq!(value["ok"], true);
    let changes = value["changes"].as_array().expect("changes is an array");
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["path"], overlay_path(&env).display().to_string());
    assert_eq!(changes[0]["action"], "create");
    assert!(
        changes[0]["diff"]
            .as_str()
            .expect("diff is a string")
            .contains("\"ANTHROPIC_MODEL\": \"zhipu/glm-5.3\"")
    );
    assert!(!stdout(&output).contains(KEY));
    assert!(!overlay_path(&env).exists());
}

#[test]
fn configure_json_reports_create_then_none() {
    let env = TestEnv::new();
    login(&env, BASE_URL);

    let output = run(&env, &["claude", "--no-launch", "--json"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("stdout is one JSON object");
    assert_eq!(value["ok"], true);
    assert_eq!(value["base_url"], BASE_URL);
    assert_eq!(value["key"]["masked"], "sk-…cdef");
    assert_eq!(value["key"]["source"], "file");
    assert_eq!(
        value["harnesses"]["claude"]["target"],
        overlay_path(&env).display().to_string()
    );
    assert_eq!(value["harnesses"]["claude"]["action"], "create");

    let output = run(&env, &["claude", "--no-launch", "--json"]);
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("stdout is one JSON object");
    assert_eq!(value["harnesses"]["claude"]["action"], "none");
    assert!(!stdout(&output).contains(KEY));
}

#[test]
fn dry_run_masks_literal_keys_in_the_diff() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    write_overlay(
        &env,
        "{\"env\": {\"ANTHROPIC_AUTH_TOKEN\": \"sk-0123456789abcdef0123456789abcdef\"}}\n",
    );

    let output = run(&env, &["claude", "--no-launch", "--dry-run", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        !stdout(&output).contains(KEY),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("sk-…cdef"),
        "stdout: {}",
        stdout(&output)
    );
}

#[test]
fn configure_records_ownership_in_the_manifest() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);

    let document = manifest(&env);
    assert_eq!(document["version"].as_integer(), Some(1));
    let entry = manifest_entry(&document, &env);

    assert_eq!(entry["harness"].as_str(), Some("claude"));
    assert_eq!(entry["created_file"].as_bool(), Some(true));
    assert!(
        entry["last_written_hash"]
            .as_str()
            .expect("hash string")
            .starts_with("fnv1a64:")
    );
    assert!(
        entry["backups"]
            .as_array()
            .expect("backups array")
            .is_empty()
    );
    assert!(entry["updated_at"].as_str().is_some());
    let text = fs::read_to_string(env.xdg_config.join("tokencamp/managed.toml")).expect("manifest");
    assert!(!text.contains(KEY));
}

#[cfg(unix)]
#[test]
fn manifest_is_private() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);

    let mode = fs::metadata(env.xdg_config.join("tokencamp/managed.toml"))
        .expect("manifest metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn manifest_records_backup_paths() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");
    run(&env, &["claude", "--no-launch"]);

    let backups = backup_files(&env);
    assert_eq!(backups.len(), 1);
    let document = manifest(&env);
    let entry = manifest_entry(&document, &env);
    let recorded: Vec<&str> = entry["backups"]
        .as_array()
        .expect("backups array")
        .iter()
        .map(|value| value.as_str().expect("backup path"))
        .collect();
    assert_eq!(recorded, vec![backups[0].display().to_string()]);
}

#[test]
fn unsupported_manifest_version_is_rejected() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    fs::create_dir_all(env.xdg_config.join("tokencamp")).expect("create tokencamp dir");
    fs::write(
        env.xdg_config.join("tokencamp/managed.toml"),
        "version = 2\n",
    )
    .expect("write manifest");

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("config_unreadable"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("managed.toml"));
    assert!(!overlay_path(&env).exists());
}

#[cfg(unix)]
#[test]
fn a_symlinked_manifest_is_rejected() {
    use std::os::unix::fs::symlink;

    let env = TestEnv::new();
    login(&env, BASE_URL);
    let real = env.root.join("real-managed.toml");
    fs::write(&real, "version = 1\n").expect("write real manifest");
    symlink(&real, env.xdg_config.join("tokencamp/managed.toml")).expect("symlink manifest");

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("symlink"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(&real).expect("read real manifest"),
        "version = 1\n"
    );
}

#[test]
fn drift_is_reported_but_the_overlay_is_still_converged() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_drift"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(overlay_path(&env)).expect("read overlay"),
        EXPECTED_OVERLAY
    );
}

#[test]
fn drift_is_reported_in_json_warnings() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");

    let output = run(&env, &["claude", "--no-launch", "--json"]);
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("stdout is one JSON object");
    assert!(
        value["warnings"]
            .as_array()
            .expect("warnings array")
            .iter()
            .any(|warning| warning["code"] == "config_drift"),
        "value: {value}"
    );
}

#[test]
fn corrupt_overlay_is_rejected_with_a_location() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    let corrupt = "{\n  \"env\": {\n";
    write_overlay(&env, corrupt);

    for args in [
        vec!["claude", "--no-launch"],
        vec!["claude", "--no-launch", "--force"],
        vec!["claude", "--no-launch", "--dry-run"],
    ] {
        let output = run(&env, &args);
        assert_eq!(output.status.code(), Some(2), "args: {args:?}");
        let message = stderr(&output);
        assert!(
            message.contains("config_unreadable"),
            "args: {args:?}: {message}"
        );
        assert!(
            message.contains(&overlay_path(&env).display().to_string()),
            "args: {args:?}: {message}"
        );
        assert!(message.contains("line "), "args: {args:?}: {message}");
        assert!(message.contains("column "), "args: {args:?}: {message}");
        assert_eq!(
            fs::read_to_string(overlay_path(&env)).expect("read overlay"),
            corrupt
        );
    }
    assert!(backup_files(&env).is_empty());
}

#[cfg(unix)]
#[test]
fn configure_follows_a_symlinked_overlay_and_preserves_the_link() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);

    let real = env.root.join("real-settings.json");
    fs::rename(overlay_path(&env), &real).expect("move overlay");
    fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).expect("chmod real overlay");
    symlink(&real, overlay_path(&env)).expect("symlink overlay");

    let other_base_url = "https://other.example";
    login(&env, other_base_url);
    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        fs::symlink_metadata(overlay_path(&env))
            .expect("overlay metadata")
            .file_type()
            .is_symlink(),
        "the symlink must be preserved"
    );
    let contents = fs::read_to_string(&real).expect("read real overlay");
    assert!(contents.contains(other_base_url));
    let mode = fs::metadata(&real)
        .expect("real metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o640);
}

#[cfg(unix)]
#[test]
fn broken_symlink_is_rejected() {
    use std::os::unix::fs::symlink;

    let env = TestEnv::new();
    login(&env, BASE_URL);
    fs::create_dir_all(env.xdg_config.join("tokencamp/harnesses/claude"))
        .expect("create overlay dir");
    symlink(env.root.join("missing.json"), overlay_path(&env)).expect("broken symlink");

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("symlink"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!env.root.join("missing.json").exists());
}

#[test]
fn a_directory_target_is_rejected() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    fs::create_dir_all(overlay_path(&env)).expect("create directory target");

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("not a regular file"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn at_most_three_backups_are_kept() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);

    for value in 1..=5 {
        edit_managed_value(&env, &value.to_string());
        let output = run(&env, &["claude", "--no-launch"]);
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    }

    assert_eq!(backup_files(&env).len(), 3);
    let document = manifest(&env);
    let entry = manifest_entry(&document, &env);
    assert_eq!(entry["backups"].as_array().expect("backups array").len(), 3);
}

#[test]
fn configure_falls_back_to_home_config_when_xdg_config_home_is_unset() {
    let env = TestEnv::new();

    let mut command = env.command();
    command
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            BASE_URL,
        ])
        .env_remove("XDG_CONFIG_HOME")
        .write_stdin(KEY)
        .assert()
        .success();

    let mut command = env.command();
    command
        .args(["claude", "--no-launch"])
        .env_remove("XDG_CONFIG_HOME")
        .assert()
        .success();

    assert!(
        env.home
            .join(".config/tokencamp/harnesses/claude/settings.json")
            .exists()
    );
}

#[test]
fn backups_fall_back_to_home_state_when_xdg_state_home_is_unset() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");

    let mut command = env.command();
    command
        .args(["claude", "--no-launch"])
        .env_remove("XDG_STATE_HOME")
        .assert()
        .success();

    let backups = env.home.join(".local/state/tokencamp/backups/claude");
    let count = fs::read_dir(&backups)
        .expect("read fallback backup dir")
        .count();
    assert_eq!(count, 1);
}

#[test]
fn rename_failure_leaves_the_overlay_untouched_and_cleans_up() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");
    let before = fs::read_to_string(overlay_path(&env)).expect("read overlay");

    let output = env
        .command()
        .args(["claude", "--no-launch"])
        .env("TCAMP_TEST_FAIL_CONFIG_RENAME", "1")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert_eq!(
        fs::read_to_string(overlay_path(&env)).expect("read overlay"),
        before
    );
    assert!(
        !overlay_path(&env)
            .parent()
            .expect("overlay parent")
            .read_dir()
            .expect("read overlay dir")
            .any(|entry| entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")),
        "temporary files must be cleaned up"
    );
}

#[test]
fn rename_failure_on_create_leaves_no_overlay() {
    let env = TestEnv::new();
    login(&env, BASE_URL);

    let output = env
        .command()
        .args(["claude", "--no-launch"])
        .env("TCAMP_TEST_FAIL_CONFIG_RENAME", "1")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(!overlay_path(&env).exists());
    assert!(
        fs::read_dir(env.xdg_config.join("tokencamp/harnesses/claude"))
            .expect("read overlay dir")
            .next()
            .is_none(),
        "the failed create must not leave a file behind"
    );
}

#[test]
fn manifest_write_failure_rolls_back_the_overlay() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");
    let before = fs::read_to_string(overlay_path(&env)).expect("read overlay");

    let output = env
        .command()
        .args(["claude", "--no-launch"])
        .env("TCAMP_TEST_FAIL_MANIFEST_WRITE", "1")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("managed.toml"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(overlay_path(&env)).expect("read overlay"),
        before
    );
}

#[test]
fn manifest_write_failure_on_create_removes_the_new_overlay() {
    let env = TestEnv::new();
    login(&env, BASE_URL);

    let output = env
        .command()
        .args(["claude", "--no-launch"])
        .env("TCAMP_TEST_FAIL_MANIFEST_WRITE", "1")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(!overlay_path(&env).exists());
}

#[test]
fn rollback_failure_exits_2_with_both_paths() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");

    let output = env
        .command()
        .args(["claude", "--no-launch"])
        .env("TCAMP_TEST_FAIL_MANIFEST_WRITE", "1")
        .env("TCAMP_TEST_FAIL_ROLLBACK", "1")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(
        message.contains(&overlay_path(&env).display().to_string()),
        "stderr: {message}"
    );
    assert!(
        message.contains(
            &env.xdg_config
                .join("tokencamp/managed.toml")
                .display()
                .to_string()
        ),
        "stderr: {message}"
    );
    assert!(message.contains("config_unreadable"), "stderr: {message}");
}

#[test]
fn lock_timeout_exits_1() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");

    let mut holder = env.std_command();
    holder
        .args(["claude", "--no-launch"])
        .env("TCAMP_TEST_LOCK_HOLD_MS", "1500")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = holder.spawn().expect("spawn lock holder");
    std::thread::sleep(std::time::Duration::from_millis(300));

    let output = env
        .command()
        .args(["claude", "--no-launch"])
        .env("TCAMP_TEST_LOCK_TIMEOUT_MS", "150")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("lock"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(child.wait().expect("wait for lock holder").success());
}

#[test]
fn a_single_concurrent_change_is_retried() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");

    let output = env
        .command()
        .args(["claude", "--no-launch"])
        .env("TCAMP_TEST_MUTATE_AFTER_READ", "once")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        fs::read_to_string(overlay_path(&env)).expect("read overlay"),
        EXPECTED_OVERLAY
    );
}

#[test]
fn a_persistent_concurrent_change_exits_1() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    edit_managed_value(&env, "1");
    let before = fs::read_to_string(overlay_path(&env)).expect("read overlay");

    let output = env
        .command()
        .args(["claude", "--no-launch"])
        .env("TCAMP_TEST_MUTATE_AFTER_READ", "always")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("changed"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        fs::read_to_string(overlay_path(&env))
            .expect("read overlay")
            .starts_with(&before)
    );
}

#[test]
fn configure_does_not_write_shell_profiles_or_user_claude_settings() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);

    for path in [
        env.home.join(".bashrc"),
        env.home.join(".zshrc"),
        env.home.join(".profile"),
        env.home.join(".claude.json"),
        env.home.join(".claude/settings.json"),
        env.claude_config.join("settings.json"),
        env.claude_config.join("settings.local.json"),
    ] {
        assert!(!path.exists(), "{} must not be written", path.display());
    }
    assert_eq!(
        fs::read_dir(&env.home).expect("read home").count(),
        0,
        "configure must not write outside the tcamp directories"
    );
}

#[test]
fn configure_recreates_a_missing_manifest_without_reporting_drift() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    fs::remove_file(env.xdg_config.join("tokencamp/managed.toml")).expect("delete manifest");
    edit_managed_value(&env, "1");

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        !stderr(&output).contains("config_drift"),
        "a missing manifest cannot establish drift: {}",
        stderr(&output)
    );
    assert!(env.xdg_config.join("tokencamp/managed.toml").exists());
    assert_eq!(
        fs::read_to_string(overlay_path(&env)).expect("read overlay"),
        EXPECTED_OVERLAY
    );
}

#[test]
fn configure_uses_an_environment_key_without_storing_it() {
    let env = TestEnv::new();

    let output = env
        .command()
        .args(["claude", "--no-launch", "--json"])
        .env("TOKENCAMP_API_KEY", KEY)
        .env("TOKENCAMP_BASE_URL", BASE_URL)
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("stdout is one JSON object");
    assert_eq!(value["key"]["source"], "env");
    assert!(!env.credentials_path().exists());
    assert!(
        fs::read_to_string(overlay_path(&env))
            .expect("read overlay")
            .contains(BASE_URL)
    );
}

#[test]
fn configure_writes_the_base_url_root_not_the_v1_path() {
    let env = TestEnv::new();
    login(&env, "https://tokencamp.example/v1/");

    let output = run(
        &env,
        &[
            "claude",
            "--no-launch",
            "--base-url",
            "https://tokencamp.example/v1/",
        ],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let contents = fs::read_to_string(overlay_path(&env)).expect("read overlay");
    assert!(contents.contains("\"ANTHROPIC_BASE_URL\": \"https://tokencamp.example\""));
    assert!(!contents.contains("tokencamp.example/v1"));
}

#[cfg(unix)]
#[test]
fn an_adopted_literal_key_is_backed_up_privately_and_never_printed() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    login(&env, BASE_URL);
    write_overlay(
        &env,
        "{\"env\": {\"ANTHROPIC_AUTH_TOKEN\": \"sk-0123456789abcdef0123456789abcdef\"}}\n",
    );

    let output = run(&env, &["claude", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(!stdout(&output).contains(KEY));
    assert!(!stderr(&output).contains(KEY));
    let backups = backup_files(&env);
    assert_eq!(backups.len(), 1);
    let mode = fs::metadata(&backups[0])
        .expect("backup metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    assert!(
        fs::read_to_string(&backups[0])
            .expect("read backup")
            .contains(KEY)
    );
    assert!(
        !fs::read_to_string(overlay_path(&env))
            .expect("read overlay")
            .contains(KEY)
    );
}

#[test]
fn dry_run_does_not_fail_on_a_corrupt_manifest() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    fs::write(
        env.xdg_config.join("tokencamp/managed.toml"),
        "version = 2\n",
    )
    .expect("write corrupt manifest");

    let output = run(&env, &["claude", "--no-launch", "--dry-run"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(!stderr(&output).contains("config_unreadable"));
}

#[test]
fn dry_run_reports_a_conflict_unless_forced() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    write_overlay(&env, "{\"env\": {\"FOO\": \"bar\"}}\n");

    let output = run(&env, &["claude", "--no-launch", "--dry-run"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["claude", "--no-launch", "--dry-run", "--force"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("+++ "),
        "stdout: {}",
        stdout(&output)
    );
    assert!(backup_files(&env).is_empty());
}

#[test]
fn configure_does_not_write_project_level_config() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    let project = env.root.join("project");
    fs::create_dir_all(&project).expect("create project directory");

    let mut command = env.command();
    command
        .current_dir(&project)
        .args(["claude", "--no-launch"]);
    let output = command.output().expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        fs::read_dir(&project)
            .expect("read project directory")
            .count(),
        0,
        "configure must not create project-level configuration"
    );
}

#[test]
fn dry_run_renders_multiple_hunks() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    run(&env, &["claude", "--no-launch"]);
    let original = fs::read_to_string(overlay_path(&env)).expect("read overlay");
    let edited = original
        .replace(
            "\"ANTHROPIC_API_KEY\": \"\"",
            "\"ANTHROPIC_API_KEY\": \"x\"",
        )
        .replace(
            "\"MAX_THINKING_TOKENS\": \"0\"",
            "\"MAX_THINKING_TOKENS\": \"1\"",
        );
    fs::write(overlay_path(&env), edited).expect("edit overlay");

    let output = run(&env, &["claude", "--no-launch", "--dry-run"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let hunks = stdout(&output)
        .lines()
        .filter(|line| line.starts_with("@@ "))
        .count();
    assert_eq!(hunks, 2, "stdout: {}", stdout(&output));
}

fn edit_managed_value(env: &TestEnv, value: &str) {
    let original = fs::read_to_string(overlay_path(env)).expect("read overlay");
    let edited = original.replace(
        "\"MAX_THINKING_TOKENS\": \"0\"",
        &format!("\"MAX_THINKING_TOKENS\": \"{value}\""),
    );
    assert_ne!(edited, original);
    fs::write(overlay_path(env), edited).expect("edit overlay");
}

fn manifest(env: &TestEnv) -> toml_edit::DocumentMut {
    fs::read_to_string(env.xdg_config.join("tokencamp/managed.toml"))
        .expect("read manifest")
        .parse()
        .expect("parse manifest")
}

fn manifest_entry<'a>(document: &'a toml_edit::DocumentMut, env: &TestEnv) -> &'a toml_edit::Table {
    let key = overlay_path(env).display().to_string();
    document["targets"].as_table().expect("targets table")[key.as_str()]
        .as_table()
        .unwrap_or_else(|| panic!("missing manifest entry for {key}"))
}

#[test]
fn dry_run_masks_non_sk_literal_tokens_in_known_secret_fields() {
    let env = TestEnv::new();
    login(&env, BASE_URL);
    write_overlay(
        &env,
        "{\"env\": {\"ANTHROPIC_AUTH_TOKEN\": \"plain-secret-value\"}}\n",
    );

    let output = run(&env, &["claude", "--no-launch", "--dry-run", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        !stdout(&output).contains("plain-secret-value"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("sk-…alue"),
        "stdout: {}",
        stdout(&output)
    );
}

fn write_overlay(env: &TestEnv, contents: &str) {
    let path = overlay_path(env);
    fs::create_dir_all(path.parent().expect("overlay parent")).expect("create overlay directory");
    fs::write(path, contents).expect("write overlay");
}

fn backup_files(env: &TestEnv) -> Vec<PathBuf> {
    let directory = env.xdg_state.join("tokencamp/backups/claude");
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
