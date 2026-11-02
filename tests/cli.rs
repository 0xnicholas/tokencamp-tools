mod support;

use std::fs;
use std::process::Output;

use predicates::prelude::*;
use serde_json::Value;
use support::{MockResponse, MockServer, TestEnv};

#[test]
fn version_prints_package_version() {
    let env = TestEnv::new();

    env.command()
        .arg("--version")
        .assert()
        .success()
        .stdout("tcamp 0.1.0\n")
        .stderr("");
}

#[test]
fn help_lists_the_command_tree() {
    let env = TestEnv::new();

    for help_arg in ["help", "--help"] {
        let mut assert = env
            .command()
            .arg(help_arg)
            .assert()
            .success()
            .stderr("")
            .stdout(predicate::str::contains("Usage: tcamp"))
            .stdout(predicate::str::contains("--no-color"))
            .stdout(predicate::str::contains("--json"));

        for command in [
            "claude",
            "pi",
            "kimi",
            "login",
            "logout",
            "status",
            "doctor",
            "uninstall",
            "help",
        ] {
            assert = assert.stdout(predicate::str::contains(format!("  {command}")));
        }

        assert.stdout(predicate::str::contains("codex").not());
    }
}

#[test]
fn bare_run_prints_help_and_detected_harnesses() {
    let env = TestEnv::new();
    env.fake_harness("pi", "1.4.0");

    env.command()
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: tcamp"))
        .stdout(predicate::str::contains("Detected harnesses: pi"))
        .stderr("");
}

#[test]
fn bare_run_reports_none_when_no_harness_is_installed() {
    let env = TestEnv::new();

    env.command()
        .assert()
        .success()
        .stdout(predicate::str::contains("Detected harnesses: none"))
        .stderr("");
}

#[test]
fn bare_run_with_json_is_a_usage_error() {
    let env = TestEnv::new();
    let output = env.command().arg("--json").output().expect("run tcamp");

    let error = assert_json_error(&output, "usage");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("`--json` requires a command")
    );
    assert!(error["hint"].as_str().unwrap().contains("tcamp help"));
}

#[test]
fn non_executable_harness_files_are_not_detected() {
    let env = TestEnv::new();
    fs::write(env.bin_dir.join("pi"), "#!/bin/sh\n").expect("write non-executable file");

    env.command()
        .assert()
        .success()
        .stdout(predicate::str::contains("Detected harnesses: none"));
}

#[test]
fn unknown_command_exits_2_with_error_and_hint() {
    let env = TestEnv::new();

    env.command()
        .arg("nope")
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains(
            "error: unrecognized subcommand 'nope' [usage]",
        ))
        .stderr(predicate::str::contains("hint: run `tcamp help`"));
}

#[test]
fn unknown_flag_exits_2_with_passthrough_hint() {
    let env = TestEnv::new();

    env.command()
        .args(["pi", "--nope"])
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains(
            "error: unexpected argument '--nope' found [usage]",
        ))
        .stderr(predicate::str::contains(
            "hint: unknown flags before `--` are rejected",
        ));
}

#[test]
fn codex_exits_2_with_scope_hint_and_writes_nothing() {
    let env = TestEnv::new();

    env.command()
        .arg("codex")
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains(
            "error: unrecognized subcommand 'codex' [usage]",
        ))
        .stderr(predicate::str::contains(
            "hint: Codex is not supported in v1",
        ));

    assert_no_files(&env);
}

#[test]
fn removed_commands_and_flags_are_rejected() {
    let env = TestEnv::new();
    let cases: &[&[&str]] = &[
        &["opencode"],
        &["env"],
        &["pi", "--project"],
        &["doctor", "--fix"],
        &["status", "--lang", "zh"],
    ];

    for args in cases {
        env.command().args(*args).assert().code(2).stdout("");
    }

    assert_no_files(&env);
}

#[test]
fn placeholder_commands_exit_2_without_touching_the_filesystem() {
    let env = TestEnv::new();
    let cases: &[&[&str]] = &[
        &["claude", "--no-launch"],
        &["pi", "--no-launch"],
        &["kimi", "--no-launch"],
        &["login"],
        &["logout"],
        &["status"],
        &["doctor"],
        &["uninstall", "--dry-run"],
    ];

    for args in cases {
        env.command()
            .args(*args)
            .assert()
            .code(2)
            .stdout("")
            .stderr(predicate::str::contains("is not implemented yet"))
            .stderr(predicate::str::contains("hint:"));
    }

    assert_no_files(&env);
}

#[test]
fn json_placeholder_failure_is_exactly_one_object() {
    let env = TestEnv::new();
    let output = env
        .command()
        .args(["status", "--json"])
        .output()
        .expect("run tcamp");

    let error = assert_json_error(&output, "usage");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("`tcamp status` is not implemented yet")
    );
    assert!(
        error["hint"]
            .as_str()
            .unwrap()
            .contains("command-line shell")
    );
}

#[test]
fn json_parse_failure_is_exactly_one_object() {
    let env = TestEnv::new();
    let output = env
        .command()
        .args(["login", "--json", "--nope"])
        .output()
        .expect("run tcamp");

    let error = assert_json_error(&output, "usage");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("unexpected argument '--nope'")
    );
    assert!(error["hint"].as_str().unwrap().contains("tcamp help"));
}

#[test]
fn json_with_launch_is_a_usage_error_pointing_at_no_launch() {
    let env = TestEnv::new();
    let output = env
        .command()
        .args(["claude", "--json"])
        .output()
        .expect("run tcamp");

    let error = assert_json_error(&output, "usage");
    assert!(
        error["hint"]
            .as_str()
            .unwrap()
            .contains("tcamp claude --no-launch --json")
    );
}

#[test]
fn arguments_after_double_dash_are_passed_through() {
    let env = TestEnv::new();

    env.command()
        .args(["pi", "--no-launch", "--", "--nope"])
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains("is not implemented yet"))
        .stderr(predicate::str::contains("unexpected argument").not());
}

#[test]
fn placeholder_commands_make_no_network_requests() {
    let env = TestEnv::new();
    let server = MockServer::start(MockResponse::json(200, "{}"));
    let cases: &[&[&str]] = &[
        &["login"],
        &["status", "--json"],
        &["doctor", "--network"],
        &["uninstall", "--dry-run"],
    ];

    for args in cases {
        env.command()
            .env("TOKENCAMP_BASE_URL", server.url(""))
            .args(*args)
            .assert()
            .code(2);
    }

    assert!(server.requests().is_empty());
}

#[test]
fn color_is_disabled_by_flag_environment_and_dumb_terminal() {
    let env = TestEnv::new();
    assert_no_ansi(&env, &["nope", "--no-color"], &[]);
    assert_no_ansi(&env, &["nope"], &[("NO_COLOR", "1")]);
    assert_no_ansi(&env, &["nope"], &[("TERM", "dumb")]);
    assert_no_ansi(&env, &["nope"], &[]);
}

fn assert_no_ansi(env: &TestEnv, args: &[&str], variables: &[(&str, &str)]) {
    let mut command = env.command();
    command.args(args);
    for (key, value) in variables {
        command.env(key, value);
    }

    let output = command.output().expect("run tcamp");
    let combined = format!("{}{}", stdout_text(&output), stderr_text(&output));
    assert!(
        !combined.contains('\u{1b}'),
        "unexpected ANSI escape for {args:?} / {variables:?}"
    );
}

fn assert_json_error(output: &Output, code: &str) -> serde_json::Map<String, Value> {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    assert_eq!(stdout_text(output).lines().count(), 1);

    let value = parse_json(output);
    assert_eq!(value.as_object().expect("object").len(), 2);
    assert_eq!(value["ok"].as_bool(), Some(false));
    let error = value["error"].as_object().expect("error object").clone();
    assert_eq!(error.len(), 3);
    assert_eq!(error["code"].as_str(), Some(code));
    error
}

fn assert_no_files(env: &TestEnv) {
    for dir in [
        &env.home,
        &env.xdg_config,
        &env.xdg_state,
        &env.claude_config,
        &env.pi_agent,
        &env.kimi_home,
    ] {
        assert_empty(dir);
    }
}

fn assert_empty(path: &std::path::Path) {
    let entries: Vec<_> = fs::read_dir(path)
        .expect("read fixture directory")
        .map(|entry| entry.expect("read entry").file_name())
        .collect();
    assert!(
        entries.is_empty(),
        "expected {path:?} to stay empty, got {entries:?}"
    );
}

fn parse_json(output: &Output) -> Value {
    serde_json::from_str(&stdout_text(output)).expect("stdout is one JSON object")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}
