#![cfg(unix)]

mod support;

use std::fs;
use std::process::Output;

use support::TestEnv;
use support::pty::{PtyStep, run_pty};
use support::{MockResponse, MockServer};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

#[test]
fn declining_the_first_write_does_not_launch() {
    let env = TestEnv::new();
    let server = MockServer::start_sequence(vec![MockResponse::json(200, r#"{"id":"model"}"#)]);
    env.command()
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            &server.url(""),
        ])
        .write_stdin(KEY)
        .assert()
        .success();
    let claude = env.fake_harness("claude", "2.1.316");

    let mut command = env.std_command();
    command.args(["claude"]);
    let output = run_pty(
        command,
        &[PtyStep {
            await_text: "Write it? [Y/n] ",
            send: "n\n",
        }],
    );

    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(0),
        "pty output: {}",
        output.output
    );
    assert!(
        output.output.contains("No changes."),
        "pty output: {}",
        output.output
    );
    assert!(!claude.ran(), "declining must not launch");
    assert!(!env.claude_overlay_path().exists());
}

#[test]
fn declining_the_kimi_first_write_does_not_launch() {
    let env = TestEnv::new();
    let server = MockServer::start_sequence(vec![MockResponse::json(200, r#"{"id":"model"}"#)]);
    env.command()
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            &server.url(""),
        ])
        .write_stdin(KEY)
        .assert()
        .success();
    let kimi = env.fake_harness("kimi", "2.1.1");

    let mut command = env.std_command();
    command.args(["kimi"]);
    let output = run_pty(
        command,
        &[PtyStep {
            await_text: "Write it? [Y/n] ",
            send: "n\n",
        }],
    );

    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(0),
        "pty output: {}",
        output.output
    );
    assert!(
        output.output.contains("No changes."),
        "pty output: {}",
        output.output
    );
    assert!(!kimi.ran(), "declining must not launch");
    assert!(!env.kimi_config_path().exists());
}

#[test]
fn first_launch_on_a_tty_prompts_for_the_key_then_the_overlay() {
    let env = TestEnv::new();
    let server = MockServer::start_sequence(vec![
        MockResponse::json(404, r#"{"error":{"code":"generation_not_found"}}"#),
        MockResponse::json(200, r#"{"id":"model"}"#),
    ]);
    let claude = env.fake_harness("claude", "2.1.316");

    let mut command = env.std_command();
    command
        .args(["claude"])
        .env("TOKENCAMP_BASE_URL", server.url(""));
    let output = run_pty(
        command,
        &[
            PtyStep {
                await_text: "API key: ",
                send: "sk-0123456789abcdef0123456789abcdef\n",
            },
            PtyStep {
                await_text: "Write it? [Y/n] ",
                send: "y\n",
            },
        ],
    );

    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(0),
        "pty output: {}",
        output.output
    );
    assert!(
        !output.output.contains(KEY),
        "the hidden key input must not be echoed: {}",
        output.output
    );
    assert!(
        output.output.contains("Launching claude ..."),
        "pty output: {}",
        output.output
    );
    assert!(env.credentials_path().exists());
    assert!(env.claude_overlay_path().exists());
    assert!(claude.args().contains(&"--settings".to_string()));
    assert_eq!(
        claude.env_value("ANTHROPIC_AUTH_TOKEN").as_deref(),
        Some(KEY)
    );
}

#[test]
fn a_non_tty_launch_without_a_key_exits_2_without_prompting() {
    let env = TestEnv::new();
    env.fake_harness("claude", "2.1.316");

    let output = env.command().args(["claude"]).output().expect("run tcamp");

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("missing_key"), "stderr: {message}");
    assert!(
        message.contains("tcamp login --key-stdin"),
        "stderr: {message}"
    );
    assert!(message.contains("TOKENCAMP_API_KEY"), "stderr: {message}");
    assert!(!message.contains("API key: "), "stderr: {message}");
    assert!(!env.credentials_path().exists());
    assert!(!env.claude_overlay_path().exists());
}

#[test]
fn a_rejected_key_in_the_tty_flow_is_not_saved_and_writes_nothing() {
    let env = TestEnv::new();
    let server = MockServer::start(MockResponse::json(
        401,
        r#"{"error":{"code":"authentication"}}"#,
    ));
    env.fake_harness("claude", "2.1.316");

    let mut command = env.std_command();
    command
        .args(["claude"])
        .env("TOKENCAMP_BASE_URL", server.url(""));
    let output = run_pty(
        command,
        &[PtyStep {
            await_text: "API key: ",
            send: "sk-0123456789abcdef0123456789abcdef\n",
        }],
    );

    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(1),
        "pty output: {}",
        output.output
    );
    assert!(
        output.output.contains("[authentication]"),
        "pty output: {}",
        output.output
    );
    assert!(!env.credentials_path().exists());
    assert!(!env.claude_overlay_path().exists());
}

#[test]
fn no_launch_does_not_trigger_the_first_run_prompt() {
    let env = TestEnv::new();

    let mut command = env.std_command();
    command.args(["claude", "--no-launch"]);
    let output = run_pty(command, &[]);

    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(2),
        "pty output: {}",
        output.output
    );
    assert!(
        output.output.contains("missing_key"),
        "pty output: {}",
        output.output
    );
    assert!(
        !output.output.contains("API key: "),
        "pty output: {}",
        output.output
    );
    assert!(!env.claude_overlay_path().exists());
}

#[test]
fn configured_launches_do_not_ask_again_on_a_tty() {
    let env = TestEnv::new();
    let server = MockServer::start_sequence(vec![MockResponse::json(200, r#"{"id":"model"}"#)]);
    env.command()
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            &server.url(""),
        ])
        .write_stdin(KEY)
        .assert()
        .success();
    env.command()
        .args(["claude", "--no-launch", "--yes"])
        .assert()
        .success();
    env.fake_harness("claude", "2.1.316");

    let mut command = env.std_command();
    command.args(["claude"]);
    let output = run_pty(command, &[]);

    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(0),
        "pty output: {}",
        output.output
    );
    assert!(
        !output.output.contains("Write it?"),
        "pty output: {}",
        output.output
    );
    assert!(
        !output.output.contains("API key: "),
        "pty output: {}",
        output.output
    );
    assert!(
        output.output.contains("Launching claude ..."),
        "pty output: {}",
        output.output
    );
    assert!(fs::read_to_string(env.claude_overlay_path()).is_ok());
}
