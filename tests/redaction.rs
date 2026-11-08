mod support;

use std::fs;
use std::path::Path;
use std::process::Output;

use serde_json::Value;
use support::{MockResponse, MockServer, TestEnv, model_server};

const KEY: &str = "sk-sentinel0123456789abcdef01234567";
const MASKED: &str = "sk-…4567";
const REQUEST_ID: &str = "req_redaction_401";

fn run(env: &TestEnv, args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = env.command();
    command.args(args);
    if let Some(input) = stdin {
        command.write_stdin(input);
    }
    command.output().expect("run tcamp")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_redacted(output: &Output) -> String {
    let text = combined(output);
    assert!(!text.contains(KEY), "full API key leaked in output: {text}");
    text
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is one JSON object")
}

fn configure_all(env: &TestEnv, server: &MockServer) {
    env.login(&server.url(""), KEY);
    for harness in ["claude", "pi", "kimi"] {
        let output = run(env, &[harness, "--no-launch"], None);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{harness} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_redacted(&output);
    }
}

fn assert_tree_redacted(env: &TestEnv) {
    let mut files_scanned = 0;
    for directory in [
        &env.home,
        &env.xdg_config,
        &env.xdg_state,
        &env.claude_config,
        &env.pi_agent,
        &env.kimi_home,
    ] {
        scan_directory(directory, &mut files_scanned);
    }
    assert!(files_scanned > 0, "redaction scan found no files");
}

fn scan_directory(path: &Path, files_scanned: &mut usize) {
    for entry in fs::read_dir(path).expect("read directory for redaction scan") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            scan_directory(&path, files_scanned);
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("credentials.toml") {
            // The credential store is the one place the key is allowed to live.
            continue;
        }
        let contents = fs::read(&path).expect("read file for redaction scan");
        *files_scanned += 1;
        assert!(
            !contents
                .windows(KEY.len())
                .any(|window| window == KEY.as_bytes()),
            "full API key leaked into {}",
            path.display()
        );
    }
}

#[test]
fn successful_login_status_and_logout_never_print_the_full_key() {
    let env = TestEnv::new();
    let server = MockServer::start(MockResponse::json(200, "{}"));

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            "https://redaction.test",
            "--json",
        ],
        Some(KEY),
    );
    let text = assert_redacted(&output);
    assert!(text.contains(MASKED));

    let output = run(
        &env,
        &["login", "--key-stdin", "--base-url", &server.url("")],
        Some(KEY),
    );
    let text = assert_redacted(&output);
    assert!(text.contains(MASKED));
    assert!(text.contains(&server.url("")));

    let output = run(&env, &["status"], None);
    let text = assert_redacted(&output);
    assert!(text.contains(MASKED));
    assert!(text.contains(&env.credentials_path().display().to_string()));

    let output = run(&env, &["status", "--json"], None);
    assert_redacted(&output);

    let output = run(&env, &["logout", "--all"], None);
    assert_redacted(&output);

    let output = run(&env, &["logout", "--all", "--json"], None);
    assert_redacted(&output);
}

#[test]
fn login_from_the_environment_and_short_keys_are_always_masked() {
    let env = TestEnv::new();

    let output = env
        .command()
        .args([
            "login",
            "--no-verify",
            "--base-url",
            "https://env-redaction.test",
        ])
        .env("TOKENCAMP_API_KEY", KEY)
        .output()
        .expect("run tcamp");
    assert_eq!(output.status.code(), Some(0));
    let text = assert_redacted(&output);
    assert!(text.contains(MASKED));
    assert!(text.contains("Using API key from TOKENCAMP_API_KEY"));

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            "https://short-redaction.test",
        ],
        Some("abcdefg"),
    );
    assert_eq!(output.status.code(), Some(0));
    let text = combined(&output);
    assert!(!text.contains("abcdefg"), "short key leaked: {text}");
    assert!(text.contains("Saved API key: …"), "{text}");

    let output = run(&env, &["status", "--json"], None);
    let value = json(&output);
    assert_eq!(value["key"]["masked"], "…");
}

#[test]
fn rejected_keys_and_server_bodies_are_redacted() {
    let env = TestEnv::new();
    let body = format!(r#"{{"error":{{"code":"authentication","message":"rejected {KEY}"}}}}"#);
    let server =
        MockServer::start(MockResponse::json(401, body).with_header("x-request-id", REQUEST_ID));

    let output = run(
        &env,
        &["login", "--key-stdin", "--base-url", &server.url("")],
        Some(KEY),
    );
    assert_eq!(output.status.code(), Some(1));
    let text = assert_redacted(&output);
    assert!(text.contains("[authentication]"));
    assert!(
        !text.contains(REQUEST_ID),
        "request id is only shown with -v or --json: {text}"
    );

    let output = run(
        &env,
        &["-v", "login", "--key-stdin", "--base-url", &server.url("")],
        Some(KEY),
    );
    let text = assert_redacted(&output);
    assert!(text.contains(REQUEST_ID));

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--base-url",
            &server.url(""),
            "--json",
        ],
        Some(KEY),
    );
    let text = assert_redacted(&output);
    assert!(text.contains(REQUEST_ID));
}

#[test]
fn auth_passed_warnings_do_not_echo_the_server_body() {
    let env = TestEnv::new();
    let body =
        format!(r#"{{"error":{{"code":"insufficient_quota","message":"balance for {KEY}"}}}}"#);
    let server = MockServer::start(MockResponse::json(402, body));

    let output = run(
        &env,
        &["login", "--key-stdin", "--base-url", &server.url("")],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(0));
    let text = assert_redacted(&output);
    assert!(text.contains("insufficient_quota"));
    assert!(!text.contains("balance for"));
}

#[test]
fn server_controlled_codes_and_request_ids_cannot_leak_the_key() {
    let env = TestEnv::new();
    let body = format!(r#"{{"error":{{"code":"{KEY}"}}}}"#);
    let server = MockServer::start(MockResponse::json(401, body).with_header("x-request-id", KEY));

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--base-url",
            &server.url(""),
            "--json",
        ],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(1));
    let text = assert_redacted(&output);
    assert!(text.contains("\"code\":\"authentication\""));
    assert!(!text.contains("request_id"));

    let env = TestEnv::new();
    let body = format!(r#"{{"error":{{"code":"{KEY}"}}}}"#);
    let server = MockServer::start(MockResponse::json(402, body));
    let output = run(
        &env,
        &["login", "--key-stdin", "--base-url", &server.url("")],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(0));
    let text = assert_redacted(&output);
    assert!(text.contains("probe_402"));
}

#[test]
fn every_harness_dry_run_in_human_and_json_form_is_redacted() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);

    for harness in ["claude", "pi", "kimi"] {
        let output = run(&env, &[harness, "--dry-run"], None);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{harness} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_redacted(&output);

        let output = run(&env, &[harness, "--dry-run", "--json"], None);
        assert_eq!(output.status.code(), Some(0));
        assert_redacted(&output);
        let value = json(&output);
        assert_eq!(value["ok"], true, "{harness}: {value}");
        assert!(
            value["changes"]
                .as_array()
                .is_some_and(|changes| !changes.is_empty()),
            "{harness} must report dry-run changes: {value}"
        );
    }
}

#[test]
fn configure_outputs_configs_backups_and_state_never_contain_the_key() {
    let env = TestEnv::new();
    configure_all(&env, &model_server());
    configure_all(&env, &model_server());

    let backups = env.xdg_state.join("tokencamp/backups");
    assert!(
        backups.is_dir(),
        "second configure must leave backups in {}",
        backups.display()
    );
    let backup_count = walk_count(&backups);
    assert!(backup_count > 0, "expected at least one backup file");

    assert_tree_redacted(&env);
}

fn walk_count(path: &Path) -> usize {
    if path.is_dir() {
        return fs::read_dir(path)
            .expect("read backup directory")
            .map(|entry| walk_count(&entry.expect("backup entry").path()))
            .sum();
    }
    usize::from(path.is_file())
}

#[test]
fn dry_run_human_and_json_diffs_redact_literal_keys() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let config = format!(
        "default_model = \"tokencamp/glm-5.3\"\n\n\
         [providers.tokencamp]\n\
         type = \"openai\"\n\
         base_url = \"https://old.example/v1\"\n\
         api_key = \"{KEY}\"\n"
    );
    fs::write(env.kimi_config_path(), config).expect("write literal-key config");

    let output = run(&env, &["kimi", "--dry-run", "--force"], None);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = assert_redacted(&output);
    assert!(text.contains("sk-…4567"), "diff must show the mask: {text}");
    assert!(
        text.contains("api_key_env"),
        "diff must show the replacement: {text}"
    );

    let output = run(&env, &["kimi", "--dry-run", "--force", "--json"], None);
    assert_eq!(output.status.code(), Some(0));
    let text = assert_redacted(&output);
    let value = json(&output);
    assert_eq!(value["ok"], true);
    assert!(
        value["changes"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("sk-…4567")
    );
    assert!(text.contains("\"changes\""));
}

#[test]
fn uninstall_plans_and_warnings_never_print_a_literal_key() {
    let env = TestEnv::new();
    let config = format!(
        "[providers.tokencamp]\n\
         type = \"openai\"\n\
         base_url = \"https://old.example/v1\"\n\
         api_key = \"{KEY}\"\n"
    );
    fs::write(env.kimi_config_path(), config).expect("write literal-key config");

    let output = run(&env, &["uninstall", "--dry-run", "--harness", "kimi"], None);
    assert_eq!(output.status.code(), Some(0));
    let text = assert_redacted(&output);
    assert!(text.contains("config_conflict"), "warning missing: {text}");

    let output = run(
        &env,
        &["uninstall", "--dry-run", "--harness", "kimi", "--json"],
        None,
    );
    assert_eq!(output.status.code(), Some(0));
    assert_redacted(&output);

    let output = run(
        &env,
        &["uninstall", "--dry-run", "--harness", "kimi", "--force"],
        None,
    );
    assert_eq!(output.status.code(), Some(0));
    assert_redacted(&output);
}

#[test]
fn doctor_human_json_and_network_outputs_are_redacted() {
    let env = TestEnv::new();
    let body = format!(r#"{{"error":{{"code":"authentication","message":"rejected {KEY}"}}}}"#);
    let server =
        MockServer::start(MockResponse::json(401, body).with_header("x-request-id", REQUEST_ID));
    env.login(&server.url(""), KEY);

    let output = run(&env, &["doctor"], None);
    let text = assert_redacted(&output);
    assert!(text.contains(MASKED), "doctor must show the mask: {text}");

    let output = run(&env, &["doctor", "--json"], None);
    let text = assert_redacted(&output);
    assert!(text.contains(MASKED));
    assert!(
        !text.contains(REQUEST_ID),
        "offline doctor has no request id"
    );
    let value = json(&output);
    assert_eq!(value["key"]["masked"], MASKED);

    let output = run(&env, &["doctor", "--network"], None);
    let text = assert_redacted(&output);
    assert!(text.contains(MASKED));
    assert!(
        !text.contains(REQUEST_ID),
        "request ids require -v or --json: {text}"
    );

    let output = run(&env, &["-v", "doctor", "--network"], None);
    let text = assert_redacted(&output);
    assert!(
        text.contains(REQUEST_ID),
        "verbose doctor must show request ids"
    );

    let output = run(&env, &["doctor", "--network", "--json"], None);
    let text = assert_redacted(&output);
    assert!(text.contains(REQUEST_ID));
    assert!(
        !text.contains("rejected sk-"),
        "raw server body leaked: {text}"
    );
}

#[test]
fn corrupt_credentials_files_do_not_echo_their_contents() {
    let env = TestEnv::new();
    let path = env.credentials_path();
    fs::create_dir_all(path.parent().expect("credentials parent")).expect("create config dir");
    fs::write(
        &path,
        format!("version = \"not-an-integer\"\n# secret {KEY}\n"),
    )
    .expect("write corrupt credentials");

    for args in [
        &["status"][..],
        &["status", "--json"][..],
        &["doctor"][..],
        &["doctor", "--json"][..],
        &["login", "--key-stdin", "--no-verify"][..],
    ] {
        let output = run(&env, args, Some(KEY));
        assert_redacted(&output);
        assert!(
            output.status.code().is_some_and(|code| code != 0),
            "{args:?} must fail on corrupt credentials"
        );
    }
}
