mod support;

use std::fs;
use std::path::PathBuf;
use std::process::Output;
use std::time::{Duration, Instant};

use support::{MockResponse, MockServer, StallingServer, TestEnv, model_routes};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";

fn login(env: &TestEnv, base_url: &str) {
    env.login(base_url, KEY);
}

fn run(env: &TestEnv, args: &[&str]) -> Output {
    env.command().args(args).output().expect("run tcamp")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn write_overlay(env: &TestEnv, contents: &str) {
    let path = env.claude_overlay_path();
    fs::create_dir_all(path.parent().expect("overlay parent")).expect("create overlay dir");
    fs::write(path, contents).expect("write overlay");
}

fn overlay_path(env: &TestEnv) -> PathBuf {
    env.claude_overlay_path()
}

fn ok() -> MockResponse {
    MockResponse::json(200, r#"{"id":"model"}"#)
}

fn requested_paths(server: &MockServer) -> Vec<String> {
    let mut paths: Vec<String> = server
        .requests()
        .into_iter()
        .map(|request| request.path)
        .collect();
    paths.sort();
    paths
}

#[test]
fn preflight_checks_the_deduplicated_claude_models_before_launching() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(model_routes());
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(claude.ran());

    let requests = server.requests();
    assert_eq!(
        requested_paths(&server),
        vec![
            "/v1/models/deepseek/deepseek-v4-pro".to_string(),
            "/v1/models/zhipu/glm-5.3".to_string(),
            "/v1/models/zhipu/glm-5.3-flash".to_string(),
        ],
        "main and four slots must be deduplicated"
    );
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("authorization")
                    && value == &format!("Bearer {KEY}")),
            "model checks must authenticate: {request:?}"
        );
    }
    assert!(
        requests
            .iter()
            .all(|request| request.path.starts_with("/v1/models/")),
        "preflight must not run an auth probe or verify request: {requests:?}"
    );
}

#[test]
fn a_missing_model_exits_2_without_writing_or_launching() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(vec![
        ("/v1/models/zhipu/glm-5.3", ok()),
        (
            "/v1/models/zhipu/glm-5.3-flash",
            MockResponse::json(404, r#"{"error":{"code":"model_not_found"}}"#),
        ),
        ("/v1/models/deepseek/deepseek-v4-pro", ok()),
    ]);
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("[usage]"), "stderr: {message}");
    assert!(message.contains("zhipu/glm-5.3-flash"), "stderr: {message}");
    assert!(!claude.ran(), "a missing model must not launch");
    assert!(
        !overlay_path(&env).exists(),
        "preflight must fail before writing"
    );
}

#[test]
fn a_model_check_network_failure_warns_and_continues() {
    let env = TestEnv::new();
    let base_url = env.unused_loopback_url();
    login(&env, &base_url);
    let claude = env.fake_harness("claude", "2.1.316");

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(claude.ran());
    let message = stderr(&output);
    assert!(message.contains("warning:"), "stderr: {message}");
    assert!(message.contains("model_check_failed"), "stderr: {message}");
    assert!(overlay_path(&env).exists());
}

#[test]
fn the_model_override_is_checked_too() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(model_routes());
    login(&env, &server.url(""));
    env.fake_harness("claude", "2.1.316");

    let output = run(&env, &["claude", "--model", "kimi/kimi-k2.7-code"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        requested_paths(&server).contains(&"/v1/models/kimi/kimi-k2.7-code".to_string()),
        "the override must be checked: {:?}",
        requested_paths(&server)
    );
}

#[test]
fn an_unowned_overlay_conflict_fails_before_the_model_checks() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(model_routes());
    login(&env, &server.url(""));
    let claude = env.fake_harness("claude", "2.1.316");
    write_overlay(&env, r#"{"env": {"FOO": "bar"}}"#);

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("config_conflict"));
    assert!(server.requests().is_empty(), "conflicts must be local");
    assert!(!claude.ran());
}

#[test]
fn a_corrupt_overlay_is_rejected_before_the_model_checks() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(model_routes());
    login(&env, &server.url(""));
    env.fake_harness("claude", "2.1.316");
    write_overlay(&env, "{\n  \"env\": {\n");

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("config_unreadable"));
    assert!(server.requests().is_empty());
}

#[test]
fn a_corrupt_manifest_is_rejected_before_the_model_checks() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(model_routes());
    login(&env, &server.url(""));
    env.fake_harness("claude", "2.1.316");
    fs::create_dir_all(env.xdg_config.join("tokencamp")).expect("create tokencamp dir");
    fs::write(
        env.xdg_config.join("tokencamp/managed.toml"),
        "version = 2\n",
    )
    .expect("write manifest");

    let output = run(&env, &["claude"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("config_unreadable"));
    assert!(server.requests().is_empty());
}

#[test]
fn a_stalled_model_check_gives_up_within_the_budget_and_continues() {
    let env = TestEnv::new();
    let server = StallingServer::start();
    login(&env, &server.url());
    let claude = env.fake_harness("claude", "2.1.316");

    let started = Instant::now();
    let output = run(&env, &["claude"]);
    let elapsed = started.elapsed();

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(claude.ran());
    assert!(
        elapsed >= Duration::from_millis(1_500),
        "the check must actually wait for the timeout: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(6),
        "parallel checks must share a roughly 2s budget: {elapsed:?}"
    );
    assert!(
        stderr(&output).contains("model_check_failed"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn dry_run_does_not_contact_the_models_endpoint() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(model_routes());
    login(&env, &server.url(""));

    let output = run(&env, &["claude", "--dry-run"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(server.requests().is_empty(), "dry-run must stay offline");
    assert!(!overlay_path(&env).exists());
}

#[test]
fn no_launch_still_fails_on_a_missing_model() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(vec![(
        "/v1/models/zhipu/glm-5.3",
        MockResponse::json(404, r#"{"error":{"code":"model_not_found"}}"#),
    )]);
    login(&env, &server.url(""));

    let output = run(&env, &["claude", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(!overlay_path(&env).exists());
}
