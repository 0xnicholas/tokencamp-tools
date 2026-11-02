#![cfg(unix)]

mod support;

use std::fs;

use support::TestEnv;
use support::pty::{PtyOutput, PtyStep, run_pty};
use toml_edit::DocumentMut;

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";
const KEY2: &str = "sk-fedcba9876543210fedcba9876543210";
const MASKED_KEY: &str = "sk-…cdef";
const KEY_INPUT: &str = "sk-0123456789abcdef0123456789abcdef\n";
const KEY2_INPUT: &str = "sk-fedcba9876543210fedcba9876543210\n";
const YES_INPUT: &str = "y\n";
const NO_INPUT: &str = "n\n";
const EOF_INPUT: &str = "\u{4}";

fn pty_login(
    env: &TestEnv,
    args: &[&str],
    variables: &[(&str, &str)],
    steps: &[PtyStep],
) -> PtyOutput {
    let mut command = env.std_command();
    command.args(args);
    for (key, value) in variables {
        command.env(key, value);
    }
    run_pty(command, steps)
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

fn assert_success(output: &PtyOutput) {
    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(0),
        "pty output: {}",
        output.output
    );
}

#[test]
fn tty_login_prompts_with_hidden_input_and_saves_the_key() {
    let env = TestEnv::new();
    let server = support::MockServer::start(support::MockResponse::json(200, "{}"));
    let base_url = server.url("");

    let output = pty_login(
        &env,
        &["login", "--base-url", &base_url],
        &[],
        &[PtyStep {
            await_text: "API key: ",
            send: KEY_INPUT,
        }],
    );

    assert_success(&output);
    assert!(output.output.contains(MASKED_KEY));
    assert!(output.output.contains(&base_url));
    assert!(
        !output.output.contains(KEY),
        "hidden input was echoed: {}",
        output.output
    );
    assert_eq!(
        connection(&credentials(&env), &base_url)["api_key"].as_str(),
        Some(KEY)
    );
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn tty_login_prompts_even_when_the_environment_has_a_key() {
    let env = TestEnv::new();
    let server = support::MockServer::start(support::MockResponse::json(200, "{}"));
    let base_url = server.url("");

    let output = pty_login(
        &env,
        &["login", "--base-url", &base_url],
        &[("TOKENCAMP_API_KEY", KEY2)],
        &[PtyStep {
            await_text: "API key: ",
            send: KEY_INPUT,
        }],
    );

    assert_success(&output);
    assert!(!output.output.contains(KEY2));
    assert_eq!(
        connection(&credentials(&env), &base_url)["api_key"].as_str(),
        Some(KEY)
    );
}

#[test]
fn tty_login_retries_once_after_a_401() {
    let env = TestEnv::new();
    let server = support::MockServer::start_sequence(vec![
        support::MockResponse::json(401, r#"{"error":{"code":"authentication"}}"#),
        support::MockResponse::json(404, r#"{"error":{"code":"generation_not_found"}}"#),
    ]);
    let base_url = server.url("");

    let output = pty_login(
        &env,
        &["login", "--base-url", &base_url],
        &[],
        &[
            PtyStep {
                await_text: "API key: ",
                send: KEY2_INPUT,
            },
            PtyStep {
                await_text: "API key: ",
                send: KEY_INPUT,
            },
        ],
    );

    assert_success(&output);
    assert!(output.output.contains("try again"));
    assert!(!output.output.contains(KEY));
    assert!(!output.output.contains(KEY2));
    assert_eq!(
        connection(&credentials(&env), &base_url)["api_key"].as_str(),
        Some(KEY)
    );
    assert_eq!(server.requests().len(), 2);
}

#[test]
fn tty_overwrite_can_be_cancelled_without_writing_or_verifying() {
    let env = TestEnv::new();
    let server = support::MockServer::start(support::MockResponse::json(200, "{}"));
    let base_url = server.url("");

    let first = env
        .command()
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            &base_url,
        ])
        .write_stdin(KEY)
        .output()
        .expect("seed credential");
    assert!(first.status.success());

    let output = pty_login(
        &env,
        &["login", "--base-url", &base_url],
        &[],
        &[
            PtyStep {
                await_text: "API key: ",
                send: KEY2_INPUT,
            },
            PtyStep {
                await_text: "Replace it? [y/N]",
                send: NO_INPUT,
            },
        ],
    );

    assert_success(&output);
    assert!(output.output.contains("No changes."));
    assert_eq!(
        connection(&credentials(&env), &base_url)["api_key"].as_str(),
        Some(KEY)
    );
    assert!(server.requests().is_empty());
}

#[test]
fn tty_overwrite_replaces_the_key_after_confirmation() {
    let env = TestEnv::new();
    let server = support::MockServer::start(support::MockResponse::json(200, "{}"));
    let base_url = server.url("");

    let first = env
        .command()
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            &base_url,
        ])
        .write_stdin(KEY)
        .output()
        .expect("seed credential");
    assert!(first.status.success());

    let output = pty_login(
        &env,
        &["login", "--base-url", &base_url],
        &[],
        &[
            PtyStep {
                await_text: "API key: ",
                send: KEY2_INPUT,
            },
            PtyStep {
                await_text: "Replace it? [y/N]",
                send: YES_INPUT,
            },
        ],
    );

    assert_success(&output);
    assert_eq!(
        connection(&credentials(&env), &base_url)["api_key"].as_str(),
        Some(KEY2)
    );
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn json_mode_does_not_prompt_on_a_tty() {
    let env = TestEnv::new();
    let server = support::MockServer::start(support::MockResponse::json(200, "{}"));
    let base_url = server.url("");

    let mut command = env.std_command();
    command.args(["login", "--base-url", &base_url, "--json"]);
    let output = run_pty(command, &[]);

    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(2),
        "pty output: {}",
        output.output
    );
    assert!(output.output.contains("\"code\":\"missing_key\""));
    assert!(!output.output.contains("API key: "));
    assert!(!env.credentials_path().exists());
}

#[test]
fn tty_overwrite_eof_exits_2_without_writing() {
    let env = TestEnv::new();
    let server = support::MockServer::start(support::MockResponse::json(200, "{}"));
    let base_url = server.url("");

    let first = env
        .command()
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            &base_url,
        ])
        .write_stdin(KEY)
        .output()
        .expect("seed credential");
    assert!(first.status.success());

    let output = pty_login(
        &env,
        &["login", "--base-url", &base_url],
        &[],
        &[
            PtyStep {
                await_text: "API key: ",
                send: KEY2_INPUT,
            },
            PtyStep {
                await_text: "Replace it? [y/N]",
                send: EOF_INPUT,
            },
        ],
    );

    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(2),
        "pty output: {}",
        output.output
    );
    assert!(output.output.contains("cannot confirm"));
    assert_eq!(
        connection(&credentials(&env), &base_url)["api_key"].as_str(),
        Some(KEY)
    );
    assert!(server.requests().is_empty());
}
