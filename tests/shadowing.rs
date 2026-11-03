mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use support::{TestEnv, model_server};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";
const TOKEN: &str = "sk-shadowed0123456789abcdef012345";

fn login(env: &TestEnv, base_url: &str) {
    env.login(base_url, KEY);
}

fn run(env: &TestEnv, args: &[&str]) -> Output {
    env.command().args(args).output().expect("run tcamp")
}

fn run_in(env: &TestEnv, directory: &Path, args: &[&str]) -> Output {
    env.command()
        .current_dir(directory)
        .args(args)
        .output()
        .expect("run tcamp")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn overlay_path(env: &TestEnv) -> PathBuf {
    env.claude_overlay_path()
}

fn write_json(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("settings parent")).expect("create settings dir");
    fs::write(path, contents).expect("write settings");
}

fn assert_blocked(output: &Output, claude: &support::FakeHarness, expected_path: &Path, key: &str) {
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(output));
    let message = stderr(output);
    assert!(message.contains("config_conflict"), "stderr: {message}");
    assert!(
        message.contains(&expected_path.display().to_string()),
        "stderr must name the source path: {message}"
    );
    assert!(message.contains(key), "stderr must name the key: {message}");
    assert!(
        !message.contains(TOKEN),
        "stderr must not print the shadowed value: {message}"
    );
    assert!(!claude.ran(), "a shadowed launch must not exec the harness");
}

#[test]
fn user_settings_auth_token_blocks_launch_before_any_network_call() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let settings = env.claude_config.join("settings.json");
    write_json(
        &settings,
        &format!(r#"{{"env":{{"ANTHROPIC_AUTH_TOKEN":"{TOKEN}"}}}}"#),
    );

    let output = run(&env, &["claude"]);

    assert_blocked(&output, &claude, &settings, "ANTHROPIC_AUTH_TOKEN");
    assert!(
        server.requests().is_empty(),
        "shadowing must be checked before model requests"
    );
    assert!(!overlay_path(&env).exists());
}

#[test]
fn user_settings_api_key_helper_blocks_launch() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let settings = env.claude_config.join("settings.json");
    write_json(&settings, r#"{"apiKeyHelper": "/bin/echo hidden"}"#);

    let output = run(&env, &["claude"]);

    assert_blocked(&output, &claude, &settings, "apiKeyHelper");
}

#[test]
fn force_does_not_bypass_shadowing() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let settings = env.claude_config.join("settings.json");
    write_json(
        &settings,
        &format!(r#"{{"env":{{"ANTHROPIC_AUTH_TOKEN":"{TOKEN}"}}}}"#),
    );

    let output = run(&env, &["claude", "--force"]);

    assert_blocked(&output, &claude, &settings, "ANTHROPIC_AUTH_TOKEN");
}

#[test]
fn project_settings_block_launch() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let project = env.root.join("project");
    let settings = project.join(".claude/settings.json");
    write_json(
        &settings,
        &format!(r#"{{"env":{{"ANTHROPIC_AUTH_TOKEN":"{TOKEN}"}}}}"#),
    );

    let output = run_in(&env, &project, &["claude"]);

    assert_blocked(&output, &claude, &settings, "ANTHROPIC_AUTH_TOKEN");
}

#[test]
fn project_local_settings_block_launch() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let project = env.root.join("project");
    let settings = project.join(".claude/settings.local.json");
    write_json(&settings, r#"{"apiKeyHelper": "helper"}"#);

    let output = run_in(&env, &project, &["claude"]);

    assert_blocked(&output, &claude, &settings, "apiKeyHelper");
}

#[test]
fn shared_settings_at_the_git_root_block_launch_from_a_subdirectory() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let project = env.root.join("project");
    let nested = project.join("src/nested");
    fs::create_dir_all(project.join(".git")).expect("create git marker");
    fs::create_dir_all(&nested).expect("create nested directory");
    let settings = project.join(".claude/settings.json");
    write_json(
        &settings,
        &format!(r#"{{"env":{{"ANTHROPIC_AUTH_TOKEN":"{TOKEN}"}}}}"#),
    );

    let output = run_in(&env, &nested, &["claude"]);

    assert_blocked(&output, &claude, &settings, "ANTHROPIC_AUTH_TOKEN");
}

#[test]
fn readable_managed_settings_block_launch() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let managed = env.root.join("managed-settings.json");
    write_json(
        &managed,
        &format!(r#"{{"env":{{"ANTHROPIC_AUTH_TOKEN":"{TOKEN}"}}}}"#),
    );

    let output = env
        .command()
        .args(["claude"])
        .env("TCAMP_TEST_CLAUDE_MANAGED_SETTINGS", &managed)
        .output()
        .expect("run tcamp");

    assert_blocked(&output, &claude, &managed, "ANTHROPIC_AUTH_TOKEN");
}

#[cfg(unix)]
#[test]
fn unreadable_managed_settings_warn_and_continue() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let managed = env.root.join("managed-settings.json");
    fs::write(&managed, r#"{"env":{}}"#).expect("write managed settings");
    fs::set_permissions(&managed, fs::Permissions::from_mode(0o000))
        .expect("make managed settings unreadable");

    let output = env
        .command()
        .args(["claude"])
        .env("TCAMP_TEST_CLAUDE_MANAGED_SETTINGS", &managed)
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(claude.ran(), "an unreadable managed file must not block");
    let message = stderr(&output);
    assert!(message.contains("warning:"), "stderr: {message}");
    assert!(
        message.contains(&managed.display().to_string()),
        "stderr must name the managed path: {message}"
    );
    assert!(
        message.contains("shadowing_managed_unreadable"),
        "stderr: {message}"
    );
}

#[test]
fn no_launch_warns_about_shadowing_but_still_writes() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let settings = env.claude_config.join("settings.json");
    write_json(
        &settings,
        &format!(r#"{{"env":{{"ANTHROPIC_AUTH_TOKEN":"{TOKEN}"}}}}"#),
    );

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("warning:"), "stderr: {message}");
    assert!(message.contains("shadowing_conflict"), "stderr: {message}");
    assert!(
        message.contains(&settings.display().to_string()),
        "stderr: {message}"
    );
    assert!(!message.contains(TOKEN), "stderr: {message}");
    assert!(overlay_path(&env).exists());
}

#[test]
fn dry_run_warns_about_shadowing_without_blocking() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let settings = env.claude_config.join("settings.json");
    write_json(
        &settings,
        &format!(r#"{{"env":{{"ANTHROPIC_AUTH_TOKEN":"{TOKEN}"}}}}"#),
    );

    let output = run(&env, &["claude", "--dry-run"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(server.requests().is_empty());
    let message = stderr(&output);
    assert!(message.contains("shadowing_conflict"), "stderr: {message}");
    assert!(
        message.contains(&settings.display().to_string()),
        "stderr: {message}"
    );
    assert!(!overlay_path(&env).exists());
}

#[cfg(unix)]
#[test]
fn an_unreadable_user_settings_file_blocks_launch() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let settings = env.claude_config.join("settings.json");
    write_json(&settings, r#"{"env": {}}"#);
    fs::set_permissions(&settings, fs::Permissions::from_mode(0o000))
        .expect("make user settings unreadable");

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("config_unreadable"), "stderr: {message}");
    assert!(
        message.contains(&settings.display().to_string()),
        "stderr: {message}"
    );
    assert!(!claude.ran());
    assert!(!overlay_path(&env).exists());
}

#[cfg(unix)]
#[test]
fn no_launch_warns_about_unreadable_user_settings_and_still_writes() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let settings = env.claude_config.join("settings.json");
    write_json(&settings, r#"{"env": {}}"#);
    fs::set_permissions(&settings, fs::Permissions::from_mode(0o000))
        .expect("make user settings unreadable");

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(
        message.contains("shadowing_unreadable"),
        "stderr: {message}"
    );
    assert!(overlay_path(&env).exists());
}

#[test]
fn no_launch_json_reports_shadowing_in_the_warnings_array() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let settings = env.claude_config.join("settings.json");
    write_json(
        &settings,
        &format!(r#"{{"env":{{"ANTHROPIC_AUTH_TOKEN":"{TOKEN}"}}}}"#),
    );

    let output = run(&env, &["claude", "--no-launch", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
    let value: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).expect("stdout"))
            .expect("stdout is one JSON object");
    let warnings = value["warnings"].as_array().expect("warnings array");
    let warning = warnings
        .iter()
        .find(|warning| warning["code"] == "shadowing_conflict")
        .unwrap_or_else(|| panic!("missing shadowing warning: {value}"));
    let message = warning["message"].as_str().expect("warning message");
    assert!(message.contains(&settings.display().to_string()));
    assert!(message.contains("ANTHROPIC_AUTH_TOKEN"));
    assert!(!message.contains(TOKEN));
    assert!(overlay_path(&env).exists());
}

#[test]
fn an_empty_auth_token_does_not_block_launch() {
    let env = TestEnv::new();
    let server = model_server();
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    let settings = env.claude_config.join("settings.json");
    write_json(&settings, r#"{"env": {"ANTHROPIC_AUTH_TOKEN": ""}}"#);

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(claude.ran());
}
