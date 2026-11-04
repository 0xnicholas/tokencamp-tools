#![cfg(unix)]

mod support;

use support::TestEnv;
use support::model_server;
use support::pty::{PtyStep, run_pty};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";

fn configure_pi(env: &TestEnv) {
    let server = model_server();
    env.login(&server.url(""), KEY);
    env.command().args(["pi", "--no-launch"]).assert().success();
}

#[test]
fn confirming_the_uninstall_plan_removes_the_managed_configuration() {
    let env = TestEnv::new();
    configure_pi(&env);

    let mut command = env.std_command();
    command.args(["uninstall"]);
    let output = run_pty(
        command,
        &[PtyStep {
            await_text: "Proceed with uninstall? [y/N] ",
            send: "y\n",
        }],
    );

    assert_eq!(
        output.status.as_ref().and_then(|status| status.code()),
        Some(0),
        "pty output: {}",
        output.output
    );
    assert!(
        output.output.contains("Uninstall plan:"),
        "pty output: {}",
        output.output
    );
    assert!(!env.pi_models_path().exists());
    assert!(!env.pi_settings_path().exists());
    assert!(env.credentials_path().exists());
}

#[test]
fn declining_the_uninstall_plan_keeps_everything() {
    let env = TestEnv::new();
    configure_pi(&env);
    let models = std::fs::read(env.pi_models_path()).expect("read models");

    let mut command = env.std_command();
    command.args(["uninstall"]);
    let output = run_pty(
        command,
        &[PtyStep {
            await_text: "Proceed with uninstall? [y/N] ",
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
    assert_eq!(
        std::fs::read(env.pi_models_path()).expect("models still exist"),
        models
    );
    assert!(env.pi_settings_path().exists());
}
