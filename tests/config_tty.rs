#![cfg(unix)]

mod support;

use std::fs;
use std::path::PathBuf;

use support::TestEnv;
use support::pty::{PtyOutput, PtyStep, run_pty};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";
const BASE_URL: &str = "https://tokencamp.example";
const YES_INPUT: &str = "y\n";
const NO_INPUT: &str = "n\n";

fn login(env: &TestEnv) {
    env.login(BASE_URL, KEY);
}

fn pty_configure(env: &TestEnv, steps: &[PtyStep]) -> PtyOutput {
    let mut command = env.std_command();
    command.args(["claude", "--no-launch"]);
    run_pty(command, steps)
}

fn overlay_path(env: &TestEnv) -> PathBuf {
    env.xdg_config
        .join("tokencamp/harnesses/claude/settings.json")
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
fn first_write_prompts_on_a_tty_and_writes_on_yes() {
    let env = TestEnv::new();
    login(&env);

    let output = pty_configure(
        &env,
        &[PtyStep {
            await_text: "Write it? [Y/n] ",
            send: YES_INPUT,
        }],
    );

    assert_success(&output);
    assert!(
        output
            .output
            .contains(&format!("Config target: {}", overlay_path(&env).display())),
        "pty output: {}",
        output.output
    );
    assert!(overlay_path(&env).exists());
}

#[test]
fn first_write_prompt_declines_without_writing() {
    let env = TestEnv::new();
    login(&env);

    let output = pty_configure(
        &env,
        &[PtyStep {
            await_text: "Write it? [Y/n] ",
            send: NO_INPUT,
        }],
    );

    assert_success(&output);
    assert!(
        output.output.contains("No changes."),
        "pty output: {}",
        output.output
    );
    assert!(!overlay_path(&env).exists());
}

#[test]
fn enter_accepts_the_first_write_prompt() {
    let env = TestEnv::new();
    login(&env);

    let output = pty_configure(
        &env,
        &[PtyStep {
            await_text: "Write it? [Y/n] ",
            send: "\n",
        }],
    );

    assert_success(&output);
    assert!(overlay_path(&env).exists());
}

#[test]
fn yes_skips_the_first_write_prompt() {
    let env = TestEnv::new();
    login(&env);

    let mut command = env.std_command();
    command.args(["claude", "--no-launch", "--yes"]);
    let output = run_pty(command, &[]);

    assert_success(&output);
    assert!(
        !output.output.contains("Write it?"),
        "pty output: {}",
        output.output
    );
    assert!(overlay_path(&env).exists());
    assert!(
        fs::read_to_string(overlay_path(&env))
            .expect("read overlay")
            .contains("\"ANTHROPIC_MODEL\"")
    );
}
