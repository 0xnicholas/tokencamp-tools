mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::Value;
use support::{MockServer, TestEnv, model_server};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";

fn run(env: &TestEnv, args: &[&str]) -> Output {
    env.command().args(args).output().expect("run tcamp")
}

fn run_with_env(env: &TestEnv, args: &[&str], variables: &[(&str, &str)]) -> Output {
    let mut command = env.command();
    command.args(args);
    for (key, value) in variables {
        command.env(key, value);
    }
    command.output().expect("run tcamp")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn json(output: &Output) -> Value {
    serde_json::from_str(&stdout(output)).expect("stdout is one JSON object")
}

fn assert_success(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(output));
}

fn setup(env: &TestEnv, harnesses: &[&str]) -> MockServer {
    let server = model_server();
    env.login(&server.url(""), KEY);
    for harness in harnesses {
        let output = run(env, &[harness, "--no-launch"]);
        assert_success(&output);
    }
    server
}

fn configure_all(env: &TestEnv) -> MockServer {
    env.fake_harness("claude", "2.1.316");
    env.fake_harness("pi", "1.4.0");
    env.fake_harness("kimi", "2.1.1");
    setup(env, &["claude", "pi", "kimi"])
}

fn backup_files(env: &TestEnv) -> Vec<PathBuf> {
    let directory = env.xdg_state.join("tokencamp/backups");
    let mut files = Vec::new();
    collect_files(&directory, &mut files);
    files
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn models_value(env: &TestEnv) -> Value {
    serde_json::from_str(&fs::read_to_string(env.pi_models_path()).expect("read pi models.json"))
        .expect("models.json is JSON")
}

fn kimi_document(env: &TestEnv) -> toml_edit::DocumentMut {
    fs::read_to_string(env.kimi_config_path())
        .expect("read kimi config")
        .parse()
        .expect("config.toml is TOML")
}

fn settings_value(env: &TestEnv) -> Value {
    serde_json::from_str(
        &fs::read_to_string(env.pi_settings_path()).expect("read pi settings.json"),
    )
    .expect("settings.json is JSON")
}

#[test]
fn uninstall_removes_unmodified_tcamp_configuration_and_keeps_credentials_and_backups() {
    let env = TestEnv::new();
    configure_all(&env);

    let overlay = env.claude_overlay_path();
    let mut edited = fs::read_to_string(&overlay).expect("read overlay");
    edited.push('\n');
    fs::write(&overlay, &edited).expect("edit overlay");
    assert_success(&run(&env, &["claude", "--no-launch"]));
    let backups = backup_files(&env);
    assert!(!backups.is_empty(), "expected a backup from the rewrite");

    let output = run(&env, &["uninstall", "--yes"]);
    assert_success(&output);

    assert!(!overlay.exists(), "the owned overlay should be removed");
    assert!(
        !env.pi_models_path().exists(),
        "the created models.json should be removed"
    );
    assert!(
        !env.pi_settings_path().exists(),
        "the created settings.json should be removed"
    );
    assert!(
        !env.kimi_config_path().exists(),
        "the created config.toml should be removed"
    );
    assert!(
        env.credentials_path().exists(),
        "credentials must be retained"
    );
    for backup in &backups {
        assert!(
            backup.exists(),
            "backup {} must be retained",
            backup.display()
        );
    }

    let text = stdout(&output);
    assert!(text.contains("delete"), "plan/summary: {text}");
    assert!(text.contains(&env.credentials_path().display().to_string()));
    assert!(text.contains("tcamp logout --all"));
    assert!(text.contains("rm \"$(command -v tcamp)\""));
    for backup in &backups {
        assert!(
            text.contains(&backup.display().to_string()),
            "backup path missing from output: {text}"
        );
    }
    assert!(!text.contains(KEY), "full API key leaked");
}

#[test]
fn uninstall_keeps_modified_entries_with_a_warning_until_forced() {
    let env = TestEnv::new();
    let server = configure_all(&env);

    let overlay = env.claude_overlay_path();
    let overlay_text = fs::read_to_string(&overlay).expect("read overlay");
    fs::write(
        &overlay,
        overlay_text.replace(
            "\"MAX_THINKING_TOKENS\": \"0\"",
            "\"MAX_THINKING_TOKENS\": \"7\"",
        ),
    )
    .expect("edit overlay");
    let models_text = fs::read_to_string(env.pi_models_path()).expect("read models");
    fs::write(
        env.pi_models_path(),
        models_text.replace(
            &format!("{}/v1", server.url("")),
            "https://edited.example/v1",
        ),
    )
    .expect("edit models");
    let kimi_text = fs::read_to_string(env.kimi_config_path()).expect("read kimi config");
    fs::write(
        env.kimi_config_path(),
        kimi_text.replace(
            &format!("{}/v1", server.url("")),
            "https://edited.example/v1",
        ),
    )
    .expect("edit kimi config");

    let output = run(&env, &["uninstall", "--yes"]);
    assert_success(&output);
    assert!(overlay.exists(), "a modified overlay must be kept");
    assert!(
        env.pi_models_path().exists(),
        "a modified models.json must be kept"
    );
    assert!(
        env.kimi_config_path().exists(),
        "a modified config.toml must be kept"
    );
    assert!(
        stderr(&output).contains("config_drift"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["uninstall", "--yes", "--force"]);
    assert_success(&output);
    assert!(!overlay.exists(), "force removes the modified overlay");
    assert!(
        !env.pi_models_path().exists(),
        "force removes the modified models"
    );
    assert!(
        !env.kimi_config_path().exists(),
        "force removes the modified config"
    );
}

#[test]
fn uninstall_keeps_adopted_entries_until_forced() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    fs::write(
        env.pi_models_path(),
        r#"{
  "providers": {
    "tokencamp": {
      "name": "mine",
      "baseUrl": "https://mine.example/v1",
      "api": "openai-completions",
      "apiKey": "$TOKENCAMP_API_KEY",
      "custom": "keep-me"
    }
  }
}
"#,
    )
    .expect("write models");

    let output = run(&env, &["pi", "--no-launch", "--force"]);
    assert_success(&output);
    assert_eq!(
        models_value(&env)["providers"]["tokencamp"]["custom"],
        "keep-me"
    );

    let output = run(&env, &["uninstall", "--yes"]);
    assert_success(&output);
    assert!(env.pi_models_path().exists());
    assert!(models_value(&env)["providers"]["tokencamp"].is_object());
    assert!(
        stderr(&output).contains("adopt"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["uninstall", "--yes", "--force"]);
    assert_success(&output);
    assert!(env.pi_models_path().exists(), "the user-created file stays");
    assert!(
        models_value(&env)["providers"]["tokencamp"].is_null(),
        "force removes the adopted provider entry"
    );
}

#[test]
fn uninstall_without_yes_on_a_non_tty_exits_2_and_writes_nothing() {
    let env = TestEnv::new();
    configure_all(&env);

    let output = run(&env, &["uninstall"]);
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(env.claude_overlay_path().exists());
    assert!(
        stderr(&output).contains("--yes"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stdout(&output).contains("delete"),
        "the plan is printed first"
    );

    let output = run(&env, &["uninstall", "--json"]);
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(output.stderr.is_empty());
    assert_eq!(stdout(&output).lines().count(), 1);
    let value = json(&output);
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "config_conflict");
    assert!(value["error"]["hint"].as_str().unwrap().contains("--yes"));
    assert!(env.claude_overlay_path().exists());
}

#[test]
fn uninstall_dry_run_prints_the_plan_and_writes_nothing() {
    let env = TestEnv::new();
    configure_all(&env);
    let overlay_before = fs::read(env.claude_overlay_path()).expect("read overlay");
    let models_before = fs::read(env.pi_models_path()).expect("read models");
    let manifest = env.xdg_config.join("tokencamp/managed.toml");
    let manifest_before = fs::read(&manifest).expect("read manifest");

    let output = run(&env, &["uninstall", "--dry-run"]);
    assert_success(&output);
    let text = stdout(&output);
    assert!(text.contains("claude"));
    assert!(text.contains("delete"));
    assert!(text.contains(&env.claude_overlay_path().display().to_string()));
    assert!(text.contains(&env.pi_models_path().display().to_string()));
    assert!(text.contains(&env.kimi_config_path().display().to_string()));
    assert!(stderr(&output).contains("no files written"));

    assert_eq!(fs::read(env.claude_overlay_path()).unwrap(), overlay_before);
    assert_eq!(fs::read(env.pi_models_path()).unwrap(), models_before);
    assert_eq!(fs::read(&manifest).unwrap(), manifest_before);
    assert!(backup_files(&env).is_empty());

    let output = run(&env, &["uninstall", "--dry-run", "--json"]);
    assert_success(&output);
    let value = json(&output);
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(
        value["harnesses"]["claude"]["targets"][0]["action"],
        "delete"
    );
    assert_eq!(
        value["harnesses"]["claude"]["targets"][0]["path"],
        env.claude_overlay_path().display().to_string()
    );
    assert_eq!(value["credentials"]["retained"], true);
    assert_eq!(value["credentials"]["command"], "tcamp logout --all");
    assert_eq!(value["binary"]["command"], "rm \"$(command -v tcamp)\"");
    assert_eq!(fs::read(&manifest).unwrap(), manifest_before);
}

#[test]
fn uninstall_harness_filters_and_rejects_unknown_names() {
    let env = TestEnv::new();
    configure_all(&env);

    let output = run(&env, &["uninstall", "--yes", "--harness", "pi"]);
    assert_success(&output);
    assert!(env.claude_overlay_path().exists());
    assert!(!env.pi_models_path().exists());
    assert!(env.kimi_config_path().exists());

    let output = run(&env, &["uninstall", "--yes", "--harness", "nope"]);
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("claude"),
        "the hint lists valid harnesses: {}",
        stderr(&output)
    );
    assert!(env.claude_overlay_path().exists());

    let output = run(
        &env,
        &[
            "uninstall",
            "--yes",
            "--harness",
            "claude",
            "--harness",
            "kimi",
        ],
    );
    assert_success(&output);
    assert!(!env.claude_overlay_path().exists());
    assert!(!env.kimi_config_path().exists());
}

#[test]
fn uninstall_with_a_missing_manifest_cleans_claude_by_marker_and_requires_force_elsewhere() {
    let env = TestEnv::new();
    configure_all(&env);
    let overlay = env.claude_overlay_path();
    let mut edited = fs::read_to_string(&overlay).expect("read overlay");
    edited.push('\n');
    fs::write(&overlay, edited).expect("edit overlay");
    assert_success(&run(&env, &["claude", "--no-launch"]));
    let backups = backup_files(&env);
    assert!(!backups.is_empty());
    fs::remove_file(env.xdg_config.join("tokencamp/managed.toml")).expect("delete manifest");

    let output = run(&env, &["uninstall", "--yes"]);
    assert_success(&output);
    for backup in &backups {
        assert!(
            stdout(&output).contains(&backup.display().to_string()),
            "existing backups are printed even without a manifest: {}",
            stdout(&output)
        );
    }
    assert!(
        !env.claude_overlay_path().exists(),
        "the _tcamp marker is enough to clean the overlay"
    );
    assert!(
        env.pi_models_path().exists(),
        "Pi is not touched without a manifest"
    );
    assert!(
        env.kimi_config_path().exists(),
        "Kimi is not touched without a manifest"
    );
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["uninstall", "--yes", "--force"]);
    assert_success(&output);
    assert!(
        models_value(&env)["providers"]["tokencamp"].is_null(),
        "force removes the Pi provider entry"
    );
    let settings = settings_value(&env);
    assert!(
        settings.get("defaultProvider").is_none(),
        "force removes the Pi default provider: {settings}"
    );
    assert!(
        settings.get("defaultModel").is_none(),
        "force removes the Pi default model: {settings}"
    );
    let document = kimi_document(&env);
    assert!(document.get("providers").is_none(), "kimi: {document}");
    assert!(document.get("models").is_none(), "kimi: {document}");
    assert!(
        document.get("default_model").is_none(),
        "force removes the Kimi default model: {document}"
    );
}

#[test]
fn uninstall_keeps_an_adopted_claude_overlay_until_forced() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let overlay = env.claude_overlay_path();
    fs::create_dir_all(overlay.parent().expect("overlay directory"))
        .expect("create overlay directory");
    fs::write(&overlay, "{\"env\":{\"CUSTOM\":\"1\"}}\n").expect("write overlay");

    assert_success(&run(&env, &["claude", "--no-launch", "--force"]));

    let output = run(&env, &["uninstall", "--yes"]);
    assert_success(&output);
    assert!(overlay.exists(), "an adopted overlay is kept by default");
    assert!(
        stderr(&output).contains("adopt"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["uninstall", "--yes", "--force"]);
    assert_success(&output);
    assert!(!overlay.exists(), "force removes the adopted overlay");
}

#[test]
fn uninstall_keeps_adopted_kimi_entries_until_forced() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    fs::write(
        env.kimi_config_path(),
        "# mine\n[providers.tokencamp]\ntype = \"openai\"\ncustom = \"keep-me\"\n",
    )
    .expect("write kimi config");

    assert_success(&run(&env, &["kimi", "--no-launch", "--force"]));

    let output = run(&env, &["uninstall", "--yes"]);
    assert_success(&output);
    let document = kimi_document(&env);
    assert!(
        document.get("providers").is_some(),
        "adopted Kimi entries are kept by default: {document}"
    );
    assert!(
        stderr(&output).contains("adopt"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["uninstall", "--yes", "--force"]);
    assert_success(&output);
    assert!(
        env.kimi_config_path().exists(),
        "the user-created file stays"
    );
    let document = kimi_document(&env);
    assert!(
        document.get("providers").is_none(),
        "force removes the adopted provider: {document}"
    );
}

#[test]
fn uninstall_continues_past_an_unreadable_target_and_summarizes() {
    let env = TestEnv::new();
    fs::write(env.kimi_config_path(), "not = [valid").expect("write corrupt config");
    setup(&env, &["claude", "pi"]);

    let output = run(&env, &["uninstall", "--yes"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(!env.claude_overlay_path().exists());
    assert!(!env.pi_models_path().exists());
    assert!(
        env.kimi_config_path().exists(),
        "the unreadable target must be left untouched"
    );
    assert!(
        stderr(&output).contains(&env.kimi_config_path().display().to_string()),
        "the summary names the failed target: {}",
        stderr(&output)
    );
}

#[test]
fn uninstall_dry_run_with_an_unreadable_target_exits_2() {
    let env = TestEnv::new();
    fs::write(env.kimi_config_path(), "not = [valid").expect("write corrupt config");
    setup(&env, &["pi"]);

    let output = run(&env, &["uninstall", "--dry-run"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("delete"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        env.pi_models_path().exists(),
        "a dry run must not remove anything"
    );
    assert!(env.kimi_config_path().exists());
}

#[test]
fn uninstall_requires_force_when_the_last_written_hash_is_missing() {
    let env = TestEnv::new();
    setup(&env, &["pi"]);
    let manifest_path = env.xdg_config.join("tokencamp/managed.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("read manifest");
    let edited: String = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with("last_written_hash"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(edited, manifest);
    fs::write(&manifest_path, edited).expect("edit manifest");

    let output = run(&env, &["uninstall", "--yes"]);
    assert_success(&output);
    assert!(
        env.pi_models_path().exists(),
        "a missing last-written hash must not count as unmodified"
    );
    assert!(
        stderr(&output).contains("config_drift"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["uninstall", "--yes", "--force"]);
    assert_success(&output);
    assert!(!env.pi_models_path().exists());
    assert!(!env.pi_settings_path().exists());
}

#[test]
fn uninstall_reports_partial_failures_and_exits_1() {
    let env = TestEnv::new();
    fs::write(env.kimi_config_path(), "# user config\n").expect("write user config");
    configure_all(&env);

    let output = run_with_env(
        &env,
        &["uninstall", "--yes"],
        &[
            ("TCAMP_TEST_FAIL_CONFIG_RENAME", "1"),
            ("TCAMP_TEST_FAIL_CONFIG_RENAME_NAME", "config.toml"),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(!env.claude_overlay_path().exists());
    assert!(!env.pi_models_path().exists());
    assert!(
        env.kimi_config_path().exists(),
        "the failed target must not be half-removed"
    );
    assert!(
        stderr(&output).contains("config.toml"),
        "the summary names the failed target: {}",
        stderr(&output)
    );
}

#[test]
fn uninstall_json_reports_the_applied_plan() {
    let env = TestEnv::new();
    configure_all(&env);

    let output = run(&env, &["uninstall", "--json", "--yes"]);
    assert_success(&output);
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
    let value = json(&output);
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["harnesses"]["pi"]["targets"][0]["action"], "delete");
    assert_eq!(
        value["credentials"]["path"],
        env.credentials_path().display().to_string()
    );
    assert_eq!(value["credentials"]["retained"], true);
    assert_eq!(value["binary"]["removed"], false);
    assert!(value["backups"].is_array());
    assert!(value["warnings"].is_array());
    assert!(!stdout(&output).contains(KEY));
}

#[test]
fn a_second_uninstall_is_a_quiet_no_op() {
    let env = TestEnv::new();
    configure_all(&env);
    assert_success(&run(&env, &["uninstall", "--yes"]));

    let output = run(&env, &["uninstall", "--yes"]);
    assert_success(&output);
    assert!(
        stdout(&output).contains("No managed configuration"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(env.credentials_path().exists());
}
