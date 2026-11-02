mod support;

use std::fs;
use std::io::Write;
use std::process::{Output, Stdio};

#[cfg(unix)]
use std::os::unix::fs::symlink;

use serde_json::{Value, json};
use support::{MockResponse, MockServer, StallingServer, TestEnv};
use toml_edit::DocumentMut;

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";
const KEY2: &str = "sk-fedcba9876543210fedcba9876543210";
const MASKED_KEY: &str = "sk-…cdef";

fn run(env: &TestEnv, args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = env.command();
    command.args(args);
    if let Some(input) = stdin {
        command.write_stdin(input);
    }
    command.output().expect("run tcamp")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn json_output(output: &Output) -> Value {
    serde_json::from_str(&stdout(output)).expect("stdout is one JSON object")
}

fn warning<'a>(value: &'a Value, code: &str) -> &'a Value {
    value["warnings"]
        .as_array()
        .unwrap_or_else(|| panic!("warnings is an array: {value}"))
        .iter()
        .find(|warning| warning["code"].as_str() == Some(code))
        .unwrap_or_else(|| panic!("missing warning {code}: {value}"))
}

fn credentials(env: &TestEnv) -> DocumentMut {
    fs::read_to_string(env.credentials_path())
        .expect("read credentials file")
        .parse()
        .expect("parse credentials file")
}

fn connection<'a>(document: &'a DocumentMut, base_url: &str) -> &'a toml_edit::Table {
    document["connections"]
        .as_table()
        .expect("connections table")[base_url]
        .as_table()
        .expect("connection table")
}

#[test]
fn login_no_verify_saves_credentials_and_reports_json() {
    let env = TestEnv::new();
    let server = MockServer::start(MockResponse::json(200, "{}"));
    let base_url = server.url("/v1/");
    let root = server.url("");

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            &base_url,
            "--json",
        ],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(output.stderr.is_empty());

    let value = json_output(&output);
    assert_eq!(value["ok"], true);
    assert_eq!(value["base_url"], root);
    assert_eq!(value["key"]["masked"], MASKED_KEY);
    assert_eq!(value["key"]["source"], "stdin");
    assert_eq!(value["key"]["verified"], false);
    assert!(value["key"]["verified_at"].is_null());
    assert_eq!(value["harnesses"], json!({}));
    warning(&value, "insecure_http");
    assert!(
        !value["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "key_format"),
        "a valid key must not trigger the shape warning: {value}"
    );

    assert!(
        server.requests().is_empty(),
        "--no-verify must not make network requests"
    );

    let document = credentials(&env);
    assert_eq!(document["version"].as_integer(), Some(1));
    assert_eq!(document["active"].as_str(), Some(root.as_str()));
    let stored = connection(&document, &root);
    assert_eq!(stored["api_key"].as_str(), Some(KEY));
    assert!(stored["created_at"].as_str().is_some());
    assert!(stored.get("verified_at").is_none());
}

#[cfg(unix)]
#[test]
fn login_creates_private_directories_and_files() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    let base_url = "https://example.test";
    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            base_url,
        ],
        Some(KEY),
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let directory = env.xdg_config.join("tokencamp");
    let directory_mode = fs::metadata(&directory)
        .expect("credential directory")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(directory_mode, 0o700);

    let file_mode = fs::metadata(env.credentials_path())
        .expect("credential file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(file_mode, 0o600);
}

#[test]
fn auth_probe_accepts_generation_not_found_and_records_verification() {
    let env = TestEnv::new();
    let server = MockServer::start(MockResponse::json(
        404,
        r#"{"error":{"code":"generation_not_found"}}"#,
    ));
    let base_url = server.url("");
    let output = run(
        &env,
        &["login", "--key-stdin", "--base-url", &base_url, "--json"],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    assert_eq!(value["key"]["verified"], true);
    assert!(value["key"]["verified_at"].as_str().is_some());
    let document = credentials(&env);
    assert!(
        connection(&document, &base_url)
            .get("verified_at")
            .and_then(|item| item.as_str())
            .is_some()
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert!(requests[0].path.starts_with("/v1/generation/"));
    let generation = requests[0].path.trim_start_matches("/v1/generation/");
    let uuid = uuid::Uuid::parse_str(generation).expect("probe uses a uuid");
    assert_eq!(uuid.get_version_num(), 4);
    assert_eq!(
        request_header(&requests[0].headers, "authorization"),
        Some(format!("Bearer {KEY}"))
    );
}

#[test]
fn auth_probe_accepts_200_and_records_verification() {
    let env = TestEnv::new();
    let server = MockServer::start(MockResponse::json(200, "{}"));

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

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(json_output(&output)["key"]["verified"], true);
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn auth_probe_401_does_not_save_the_key() {
    let env = TestEnv::new();
    let server = MockServer::start(
        MockResponse::json(401, r#"{"error":{"code":"authentication"}}"#)
            .with_header("x-request-id", "req_login_401"),
    );
    let base_url = server.url("");

    let output = run(
        &env,
        &["login", "--key-stdin", "--base-url", &base_url, "--json"],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(!env.credentials_path().exists());

    let value = json_output(&output);
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "authentication");
    assert_eq!(value["error"]["request_id"], "req_login_401");
    let hint = value["error"]["hint"].as_str().expect("hint");
    assert!(hint.contains(&base_url), "hint names the base URL: {hint}");
    assert!(
        !hint.contains("/keys"),
        "self-hosted hint must not guess a portal path: {hint}"
    );
    assert!(!stdout(&output).contains(KEY));
}

#[test]
fn auth_probe_auth_passed_client_errors_save_the_key_with_a_warning() {
    for (status, code) in [
        (402, "insufficient_quota"),
        (403, "permission_denied"),
        (429, "rate_limited"),
    ] {
        let env = TestEnv::new();
        let server = MockServer::start(MockResponse::json(
            status,
            format!(r#"{{"error":{{"code":"{code}"}}}}"#),
        ));

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

        assert_eq!(
            output.status.code(),
            Some(0),
            "status {status} stderr: {}",
            stderr(&output)
        );
        let value = json_output(&output);
        assert_eq!(value["key"]["verified"], true, "status {status}");
        warning(&value, code);
        let document = credentials(&env);
        assert_eq!(
            connection(&document, &server.url(""))["api_key"].as_str(),
            Some(KEY)
        );
    }
}

#[test]
fn indeterminate_probe_results_do_not_save_the_key() {
    for response in [
        MockResponse::json(500, r#"{"error":{"code":"internal"}}"#),
        MockResponse::json(201, "{}"),
        MockResponse::empty(302),
        MockResponse::json(404, r#"{"error":{"code":"not_found"}}"#),
    ] {
        let env = TestEnv::new();
        let server = MockServer::start(response);
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

        assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
        assert!(!env.credentials_path().exists());
        let value = json_output(&output);
        assert_eq!(value["error"]["code"], "network");
        assert!(
            value["error"]["hint"]
                .as_str()
                .unwrap()
                .contains("--no-verify")
        );
    }
}

#[test]
fn unreachable_probe_does_not_save_the_key() {
    let env = TestEnv::new();
    let base_url = env.unused_loopback_url();
    let output = run(
        &env,
        &["login", "--key-stdin", "--base-url", &base_url, "--json"],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(!env.credentials_path().exists());
    let value = json_output(&output);
    assert_eq!(value["error"]["code"], "network");
    assert!(
        value["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("--no-verify")
    );
}

#[test]
fn verify_sends_one_minimal_chat_request_after_the_probe() {
    let env = TestEnv::new();
    let server = MockServer::start_sequence(vec![
        MockResponse::json(404, r#"{"error":{"code":"generation_not_found"}}"#),
        MockResponse::json(200, r#"{"id":"chatcmpl_1"}"#),
    ]);
    let base_url = server.url("");

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--verify",
            "--base-url",
            &base_url,
            "--json",
        ],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(json_output(&output)["key"]["verified"], true);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].method, "POST");
    assert_eq!(requests[1].path, "/v1/chat/completions");
    assert_eq!(
        request_header(&requests[1].headers, "authorization"),
        Some(format!("Bearer {KEY}"))
    );
    let body: Value = serde_json::from_str(&requests[1].body).expect("chat body is JSON");
    assert_eq!(body["model"], "auto");
    assert_eq!(body["max_tokens"], 1);
    assert_eq!(body["stream"], false);
    let messages = body["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert!(
        messages[0]["content"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
}

#[test]
fn verify_401_does_not_save_the_key() {
    let env = TestEnv::new();
    let server = MockServer::start_sequence(vec![
        MockResponse::json(404, r#"{"error":{"code":"generation_not_found"}}"#),
        MockResponse::json(401, r#"{"error":{"code":"authentication"}}"#),
    ]);

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--verify",
            "--base-url",
            &server.url(""),
            "--json",
        ],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(!env.credentials_path().exists());
    assert_eq!(json_output(&output)["error"]["code"], "authentication");
}

#[test]
fn verify_402_saves_the_key_with_a_warning() {
    let env = TestEnv::new();
    let server = MockServer::start_sequence(vec![
        MockResponse::json(404, r#"{"error":{"code":"generation_not_found"}}"#),
        MockResponse::json(402, r#"{"error":{"code":"insufficient_quota"}}"#),
    ]);

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--verify",
            "--base-url",
            &server.url(""),
            "--json",
        ],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    warning(&value, "insufficient_quota");
    assert_eq!(value["key"]["verified"], true);
    assert!(env.credentials_path().exists());
}

#[test]
fn key_input_honors_stdin_env_and_non_interactive_rules() {
    let env = TestEnv::new();
    let missing = run(&env, &["login", "--no-verify", "--json"], None);
    assert_eq!(missing.status.code(), Some(2));
    let value = json_output(&missing);
    assert_eq!(value["error"]["code"], "missing_key");
    let hint = value["error"]["hint"].as_str().unwrap();
    assert!(hint.contains("--key-stdin"));
    assert!(hint.contains("TOKENCAMP_API_KEY"));
    assert!(!env.credentials_path().exists());

    let empty = run(
        &env,
        &["login", "--key-stdin", "--no-verify", "--json"],
        Some(" \n"),
    );
    assert_eq!(empty.status.code(), Some(2));
    assert_eq!(json_output(&empty)["error"]["code"], "missing_key");
    assert!(!env.credentials_path().exists());

    let control = run(
        &env,
        &["login", "--key-stdin", "--no-verify", "--json"],
        Some("sk-good\u{1}bad"),
    );
    assert_eq!(control.status.code(), Some(2));
    assert!(!env.credentials_path().exists());

    let literal_flag = run(&env, &["login", "--key", KEY, "--json"], None);
    assert_eq!(literal_flag.status.code(), Some(2));
    assert_eq!(json_output(&literal_flag)["error"]["code"], "usage");

    let env_key = format!("{}\n", KEY);
    let mut command = env.command();
    command
        .env("TOKENCAMP_API_KEY", KEY)
        .args(["login", "--no-verify", "--base-url", "https://example.test"])
        .write_stdin(env_key.as_str());
    let output = command.output().expect("run login");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let human_stdout = stdout(&output);
    assert!(human_stdout.contains(MASKED_KEY));
    assert!(human_stdout.contains("https://example.test"));
    assert!(!human_stdout.contains(KEY));
    assert!(!stderr(&output).contains(KEY));
    assert!(stderr(&output).contains("TOKENCAMP_API_KEY"));
    assert_eq!(
        connection(&credentials(&env), "https://example.test")["api_key"].as_str(),
        Some(KEY)
    );
}

#[test]
fn key_shape_is_only_a_warning_and_whitespace_is_trimmed() {
    let env = TestEnv::new();
    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            "https://example.test",
            "--json",
        ],
        Some("  short-key\n"),
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    warning(&value, "key_format");
    assert_eq!(
        connection(&credentials(&env), "https://example.test")["api_key"].as_str(),
        Some("short-key")
    );
}

#[test]
fn overwrite_requires_force_on_non_tty_and_refreshes_timestamps() {
    let env = TestEnv::new();
    let base_url = "https://example.test";
    let first = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            base_url,
        ],
        Some(KEY),
    );
    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));
    let created_at = connection(&credentials(&env), base_url)["created_at"]
        .as_str()
        .expect("created_at")
        .to_string();
    std::thread::sleep(std::time::Duration::from_millis(10));

    let refused = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            base_url,
            "--json",
        ],
        Some(KEY2),
    );
    assert_eq!(refused.status.code(), Some(2));
    let value = json_output(&refused);
    assert_eq!(value["error"]["code"], "config_conflict");
    assert!(value["error"]["hint"].as_str().unwrap().contains("--force"));
    assert_eq!(
        connection(&credentials(&env), base_url)["api_key"].as_str(),
        Some(KEY)
    );

    let replaced = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--force",
            "--base-url",
            base_url,
            "--json",
        ],
        Some(KEY2),
    );
    assert_eq!(
        replaced.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&replaced)
    );
    let value = json_output(&replaced);
    warning(&value, "credential_replaced");
    let document = credentials(&env);
    let stored = connection(&document, base_url);
    assert_eq!(stored["api_key"].as_str(), Some(KEY2));
    assert_ne!(stored["created_at"].as_str(), Some(created_at.as_str()));
    assert!(stored.get("verified_at").is_none());
}

#[test]
fn overwrite_clears_verified_at_after_a_verified_login() {
    let env = TestEnv::new();
    let server = MockServer::start(MockResponse::json(200, "{}"));
    let base_url = server.url("");
    let first = run(
        &env,
        &["login", "--key-stdin", "--base-url", &base_url, "--json"],
        Some(KEY),
    );
    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));
    assert!(
        connection(&credentials(&env), &base_url)
            .get("verified_at")
            .is_some()
    );

    let replaced = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--force",
            "--base-url",
            &base_url,
            "--json",
        ],
        Some(KEY2),
    );
    assert_eq!(
        replaced.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&replaced)
    );
    let document = credentials(&env);
    assert!(
        connection(&document, &base_url)
            .get("verified_at")
            .is_none()
    );
}

fn request_header(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

#[test]
fn logout_removes_active_or_named_connections_without_switching() {
    let env = TestEnv::new();
    let harness_settings = env.claude_config.join("settings.json");
    fs::write(&harness_settings, "{\"user\":true}\n").expect("write user settings");

    for base_url in ["https://a.test", "https://b.test", "https://c.test"] {
        let output = run(
            &env,
            &[
                "login",
                "--key-stdin",
                "--no-verify",
                "--base-url",
                base_url,
            ],
            Some(KEY),
        );
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    }

    let document = credentials(&env);
    assert_eq!(document["active"].as_str(), Some("https://c.test"));
    assert_eq!(document["connections"].as_table().unwrap().len(), 3);

    let output = run(&env, &["logout", "--json"], None);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    assert_eq!(value["base_url"], "https://c.test");
    warning(&value, "harness_restart_required");
    let document = credentials(&env);
    assert!(document.get("active").is_none());
    let connections = document["connections"].as_table().unwrap();
    assert_eq!(connections.len(), 2);
    assert!(connections.contains_key("https://a.test"));
    assert!(connections.contains_key("https://b.test"));

    let output = run(
        &env,
        &["logout", "--base-url", "https://a.test", "--json"],
        None,
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    assert_eq!(value["base_url"], "https://a.test");
    let document = credentials(&env);
    assert!(document.get("active").is_none());
    let connections = document["connections"].as_table().unwrap();
    assert_eq!(connections.len(), 1);
    assert!(connections.contains_key("https://b.test"));

    let output = run(&env, &["logout", "--all", "--json"], None);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    assert!(value["base_url"].is_null());
    warning(&value, "harness_restart_required");
    let document = credentials(&env);
    assert!(document.get("active").is_none());
    assert_eq!(
        document
            .get("connections")
            .and_then(toml_edit::Item::as_table)
            .map_or(0, toml_edit::Table::len),
        0
    );

    assert_eq!(
        fs::read_to_string(&harness_settings).expect("read user settings"),
        "{\"user\":true}\n"
    );
}

#[test]
fn status_reports_env_override_without_persisting_it() {
    let env = TestEnv::new();
    let first = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            "https://status.test",
        ],
        Some(KEY),
    );
    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));

    let mut command = env.command();
    command
        .env("TOKENCAMP_API_KEY", KEY2)
        .args(["status", "--json"]);
    let output = command.output().expect("run status");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    assert_eq!(value["base_url"], "https://status.test");
    assert_eq!(value["key"]["masked"], "sk-…3210");
    assert_eq!(value["key"]["source"], "env");
    assert_eq!(value["key"]["verified"], false);
    assert!(value["key"]["verified_at"].is_null());
    assert_eq!(value["harnesses"], json!({}));
    assert_eq!(
        connection(&credentials(&env), "https://status.test")["api_key"].as_str(),
        Some(KEY)
    );

    let output = run(&env, &["status", "--json"], None);
    let value = json_output(&output);
    assert_eq!(value["key"]["masked"], MASKED_KEY);
    assert_eq!(value["key"]["source"], "file");

    let fresh = TestEnv::new();
    let output = run(&fresh, &["status", "--json"], None);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    assert_eq!(value["base_url"], "https://api.tokencamp.com");
    assert!(value["key"].is_null());
    assert!(!fresh.credentials_path().exists());
}

#[test]
fn corrupt_or_unsupported_credentials_are_never_overwritten() {
    let env = TestEnv::new();
    let path = env.credentials_path();
    fs::create_dir_all(path.parent().expect("credentials directory"))
        .expect("create credentials directory");

    for contents in ["this is not toml", "version = 2\n"] {
        fs::write(&path, contents).expect("write corrupt credentials");
        let output = run(
            &env,
            &[
                "login",
                "--key-stdin",
                "--no-verify",
                "--force",
                "--base-url",
                "https://example.test",
                "--json",
            ],
            Some(KEY),
        );

        assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
        let value = json_output(&output);
        assert_eq!(value["error"]["code"], "config_unreadable");
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains(path.to_str().unwrap())
        );
        assert_eq!(
            fs::read_to_string(&path).expect("read credentials"),
            contents
        );
    }
}

#[cfg(unix)]
#[test]
fn credential_symlink_is_rejected_and_not_followed() {
    let env = TestEnv::new();
    let path = env.credentials_path();
    fs::create_dir_all(path.parent().expect("credentials directory"))
        .expect("create credentials directory");
    let target = env.root.join("credential-target.toml");
    fs::write(&target, "version = 1\n").expect("write symlink target");
    symlink(&target, &path).expect("create credential symlink");

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--force",
            "--base-url",
            "https://example.test",
            "--json",
        ],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    assert_eq!(value["error"]["code"], "config_unreadable");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("symlink")
    );
    assert!(
        fs::symlink_metadata(&path)
            .expect("symlink metadata")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(&target).expect("read symlink target"),
        "version = 1\n"
    );
}

#[test]
fn concurrent_logins_are_atomic_and_last_writer_wins() {
    let env = TestEnv::new();
    let base_url = "https://concurrent.test";
    let keys: Vec<String> = (0..8).map(|index| format!("sk-{index:032}")).collect();

    let mut children = Vec::new();
    for key in &keys {
        let mut command = env.std_command();
        command
            .args([
                "login",
                "--key-stdin",
                "--no-verify",
                "--force",
                "--base-url",
                base_url,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn concurrent login");
        child
            .stdin
            .take()
            .expect("child stdin")
            .write_all(key.as_bytes())
            .expect("write key to concurrent login");
        children.push(child);
    }

    for child in children {
        let output = child.wait_with_output().expect("wait for concurrent login");
        assert!(
            output.status.success(),
            "concurrent login failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let document = credentials(&env);
    let connections = document["connections"].as_table().expect("connections");
    assert_eq!(connections.len(), 1);
    let stored = connections[base_url].as_table().expect("connection table")["api_key"]
        .as_str()
        .expect("api_key");
    assert!(keys.iter().any(|key| key == stored));

    let mut entries: Vec<_> =
        fs::read_dir(env.credentials_path().parent().expect("credentials dir"))
            .expect("read credentials directory")
            .map(|entry| entry.expect("read entry").file_name())
            .collect();
    entries.sort();
    assert_eq!(entries, [std::ffi::OsString::from("credentials.toml")]);
}

#[test]
fn credential_rename_failure_keeps_the_original_file_and_cleans_up() {
    let env = TestEnv::new();
    let base_url = "https://rename.test";
    let first = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            base_url,
        ],
        Some(KEY),
    );
    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));
    let original = fs::read_to_string(env.credentials_path()).expect("read original credentials");

    let mut command = env.command();
    command
        .env("TCAMP_TEST_FAIL_CREDENTIAL_RENAME", "1")
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--force",
            "--base-url",
            base_url,
            "--json",
        ])
        .write_stdin(KEY2);
    let output = command.output().expect("run login with injected failure");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert_eq!(json_output(&output)["error"]["code"], "io");
    assert_eq!(
        fs::read_to_string(env.credentials_path()).expect("read original credentials"),
        original
    );

    let mut entries: Vec<_> =
        fs::read_dir(env.credentials_path().parent().expect("credentials dir"))
            .expect("read credentials directory")
            .map(|entry| entry.expect("read entry").file_name())
            .collect();
    entries.sort();
    assert_eq!(entries, [std::ffi::OsString::from("credentials.toml")]);
}

#[test]
fn credentials_fall_back_to_home_config_when_xdg_is_unset() {
    let env = TestEnv::new();
    let output = env
        .command()
        .env_remove("XDG_CONFIG_HOME")
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            "https://home-fallback.test",
            "--json",
        ])
        .write_stdin(KEY)
        .output()
        .expect("run login without xdg config home");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let path = env
        .home
        .join(".config")
        .join("tokencamp")
        .join("credentials.toml");
    assert!(path.is_file());
    assert!(!env.credentials_path().exists());
    let document: DocumentMut = fs::read_to_string(&path)
        .expect("read fallback credentials")
        .parse()
        .expect("parse fallback credentials");
    assert_eq!(
        document["active"].as_str(),
        Some("https://home-fallback.test")
    );
}

#[test]
fn a_timed_out_probe_does_not_save_the_key() {
    let env = TestEnv::new();
    let server = StallingServer::start();
    let started = std::time::Instant::now();

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--base-url",
            &server.url(),
            "--json",
        ],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(!env.credentials_path().exists());
    let value = json_output(&output);
    assert_eq!(value["error"]["code"], "network");
    assert!(
        value["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("--no-verify")
    );
    assert!(
        started.elapsed() >= std::time::Duration::from_secs(9),
        "the probe should have waited for its timeout"
    );
}

#[test]
fn verify_5xx_does_not_save_the_key() {
    let env = TestEnv::new();
    let server = MockServer::start_sequence(vec![
        MockResponse::json(404, r#"{"error":{"code":"generation_not_found"}}"#),
        MockResponse::json(500, r#"{"error":{"code":"internal"}}"#),
    ]);

    let output = run(
        &env,
        &[
            "login",
            "--key-stdin",
            "--verify",
            "--base-url",
            &server.url(""),
            "--json",
        ],
        Some(KEY),
    );

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(!env.credentials_path().exists());
    assert_eq!(json_output(&output)["error"]["code"], "network");
}

#[test]
fn key_stdin_wins_over_the_environment() {
    let env = TestEnv::new();
    let mut command = env.command();
    command
        .env("TOKENCAMP_API_KEY", KEY2)
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            "https://stdin-wins.test",
            "--json",
        ])
        .write_stdin(KEY);
    let output = command.output().expect("run login");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(json_output(&output)["key"]["source"], "stdin");
    assert_eq!(
        connection(&credentials(&env), "https://stdin-wins.test")["api_key"].as_str(),
        Some(KEY)
    );
}

#[test]
fn logout_without_credentials_is_a_no_op() {
    let env = TestEnv::new();
    let output = run(&env, &["logout", "--json"], None);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json_output(&output);
    assert!(value["base_url"].is_null());
    assert_eq!(value["warnings"], json!([]));
    assert!(!env.credentials_path().exists());
}
