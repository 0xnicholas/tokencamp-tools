mod support;

use std::fs;
use std::process::Output;

use serde_json::Value;
use support::{MockResponse, MockServer, TestEnv, model_server};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";
const MASKED: &str = "sk-…cdef";

fn run(env: &TestEnv, args: &[&str]) -> Output {
    env.command().args(args).output().expect("run tcamp")
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

fn configure_all(env: &TestEnv, server: &MockServer) {
    env.login(&server.url(""), KEY);
    for harness in ["claude", "pi", "kimi"] {
        let output = run(env, &[harness, "--no-launch"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{harness} stderr: {}",
            stderr(&output)
        );
    }
}

#[test]
fn status_reports_the_connection_and_every_harness_without_network_requests() {
    let env = TestEnv::new();
    let server = model_server();
    configure_all(&env, &server);
    env.fake_harness("claude", "2.1.316");
    env.fake_harness("pi", "1.4.0");
    env.fake_harness("kimi", "2.1.1");

    let requests_before = server.requests().len();
    let output = run(&env, &["status", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        server.requests().len(),
        requests_before,
        "status must not make network requests"
    );

    let value = json(&output);
    assert_eq!(value["ok"], true);
    assert_eq!(value["base_url"], server.url(""));
    assert_eq!(value["key"]["masked"], MASKED);
    assert_eq!(value["key"]["source"], "file");
    assert_eq!(value["key"]["verified"], false);
    assert!(value["key"]["verified_at"].is_null());
    assert_eq!(
        value["credentials"],
        env.credentials_path().display().to_string()
    );

    for (harness, target) in [
        ("claude", env.claude_overlay_path()),
        ("pi", env.pi_models_path()),
        ("kimi", env.kimi_config_path()),
    ] {
        let report = &value["harnesses"][harness];
        assert_eq!(report["installed"], true, "{harness}: {report}");
        assert_eq!(report["configured"], true, "{harness}: {report}");
        assert_eq!(report["conflict"], false, "{harness}: {report}");
        assert_eq!(
            report["target"],
            target.display().to_string(),
            "{harness}: {report}"
        );
    }

    assert!(!stdout(&output).contains(KEY), "full API key leaked");
}

#[test]
fn status_reports_verified_at_when_the_key_was_verified() {
    let env = TestEnv::new();
    let server = MockServer::start(MockResponse::json(200, "{}"));
    let output = env
        .command()
        .args([
            "login",
            "--key-stdin",
            "--base-url",
            &server.url(""),
            "--json",
        ])
        .write_stdin(KEY)
        .output()
        .expect("run login");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let output = run(&env, &["status", "--json"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json(&output);
    assert_eq!(value["key"]["verified"], true);
    let verified_at = value["key"]["verified_at"]
        .as_str()
        .expect("verified_at is a string");
    assert!(!verified_at.is_empty());

    let human = run(&env, &["status"]);
    assert_eq!(human.status.code(), Some(0), "stderr: {}", stderr(&human));
    assert!(stdout(&human).contains(&format!("Verified at: {verified_at}")));
    assert!(stdout(&human).contains(MASKED));
    assert!(stdout(&human).contains(&env.credentials_path().display().to_string()));
}

#[test]
fn status_reports_missing_harnesses_and_unconfigured_targets() {
    let env = TestEnv::new();

    let output = run(&env, &["status", "--json"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json(&output);
    assert_eq!(value["base_url"], "https://api.tokencamp.com");
    assert!(value["key"].is_null());
    for harness in ["claude", "pi", "kimi"] {
        let report = &value["harnesses"][harness];
        assert_eq!(report["installed"], false, "{harness}: {report}");
        assert_eq!(report["configured"], false, "{harness}: {report}");
        assert_eq!(report["conflict"], false, "{harness}: {report}");
    }

    let human = run(&env, &["status"]);
    assert_eq!(human.status.code(), Some(0), "stderr: {}", stderr(&human));
    let text = stdout(&human);
    for harness in ["claude", "pi", "kimi"] {
        assert!(text.contains(harness), "missing {harness} in status board");
    }
    assert!(text.contains("API key: none"));
}

#[test]
fn status_flags_entries_that_tcamp_does_not_own_but_keeps_a_snapshot() {
    let env = TestEnv::new();
    let overlay = env.claude_overlay_path();
    fs::create_dir_all(overlay.parent().expect("overlay directory")).expect("create overlay dir");
    fs::write(&overlay, "{\"env\":{\"CUSTOM\":\"1\"}}\n").expect("write overlay");
    fs::create_dir_all(&env.pi_agent).expect("create pi dir");
    fs::write(
        env.pi_models_path(),
        "{\"providers\":{\"tokencamp\":{\"name\":\"mine\"}}}\n",
    )
    .expect("write models");
    fs::write(
        env.kimi_config_path(),
        "[providers.tokencamp]\ntype = \"openai\"\n",
    )
    .expect("write kimi config");

    let output = run(&env, &["status", "--json"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json(&output);
    for harness in ["claude", "pi", "kimi"] {
        let report = &value["harnesses"][harness];
        assert_eq!(report["configured"], false, "{harness}: {report}");
        assert_eq!(report["conflict"], true, "{harness}: {report}");
    }
}

#[test]
fn status_warns_about_an_unreadable_harness_target_without_failing() {
    let env = TestEnv::new();
    let overlay = env.claude_overlay_path();
    fs::create_dir_all(overlay.parent().expect("overlay directory")).expect("create overlay dir");
    fs::write(&overlay, "not json").expect("write corrupt overlay");

    let output = run(&env, &["status", "--json"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json(&output);
    assert_eq!(value["harnesses"]["claude"]["conflict"], true);
    assert!(
        value["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning["code"] == "config_unreadable"),
        "warnings: {}",
        value["warnings"]
    );

    let human = run(&env, &["status"]);
    assert_eq!(human.status.code(), Some(0), "stderr: {}", stderr(&human));
    assert!(stderr(&human).contains("warning:"));
}
