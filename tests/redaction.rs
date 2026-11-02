mod support;

use std::process::Output;

use support::{MockResponse, MockServer, TestEnv};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";
const MASKED: &str = "sk-…cdef";
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
