mod support;

use std::process::Output;

use serde_json::Value;
use support::{MockResponse, MockServer, TestEnv};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";
const MODEL_NOT_FOUND: &str = r#"{"error":{"code":"model_not_found"}}"#;

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
    assert!(
        stderr(output).is_empty(),
        "stderr must stay empty: {}",
        stderr(output)
    );
    assert_eq!(
        stdout(output).lines().count(),
        1,
        "stdout must be exactly one JSON object: {}",
        stdout(output)
    );
    serde_json::from_str(&stdout(output)).expect("stdout is one JSON object")
}

fn check<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .find(|check| check["id"] == id)
        .unwrap_or_else(|| panic!("missing check {id}: {report}"))
}

fn check_status<'a>(report: &'a Value, id: &str) -> &'a str {
    check(report, id)["status"].as_str().expect("check status")
}

fn auth_ok() -> MockResponse {
    MockResponse::json(404, r#"{"error":{"code":"generation_not_found"}}"#)
}

fn reachable_ok() -> MockResponse {
    MockResponse::json(200, r#"{"data":[]}"#)
}

fn chat_ok() -> MockResponse {
    MockResponse::json(
        200,
        r#"{"id":"chat","choices":[{"message":{"role":"assistant","content":"pong"}}]}"#,
    )
}

fn messages_ok() -> MockResponse {
    MockResponse::json(
        200,
        r#"{"id":"msg","type":"message","role":"assistant","content":[{"type":"text","text":"pong"}]}"#,
    )
}

fn model_ok() -> MockResponse {
    MockResponse::json(200, r#"{"id":"model"}"#)
}

fn network_server(
    reachable: MockResponse,
    auth: MockResponse,
    chat: MockResponse,
    messages: MockResponse,
) -> MockServer {
    MockServer::start_routes(vec![
        ("/v1/models", reachable),
        ("/v1/generation/*", auth),
        ("/v1/chat/completions", chat),
        ("/v1/messages", messages),
        ("/v1/models/*", model_ok()),
    ])
}

fn default_server() -> MockServer {
    network_server(reachable_ok(), auth_ok(), chat_ok(), messages_ok())
}

fn logged_in_server(env: &TestEnv) -> MockServer {
    let server = default_server();
    env.login(&server.url(""), KEY);
    server
}

fn requests_to(server: &MockServer, method: &str, path: &str) -> usize {
    server
        .requests()
        .iter()
        .filter(|request| request.method == method && request.path == path)
        .count()
}

fn request_header<'a>(request: &'a support::RecordedRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

#[test]
fn offline_network_checks_include_verify_but_make_no_requests() {
    let env = TestEnv::new();
    let server = default_server();
    env.login(&server.url(""), KEY);

    let output = run(&env, &["doctor", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(report["network"], false);
    assert_eq!(report["verify"], false);
    for id in ["network.reachable", "network.auth", "network.verify"] {
        assert_eq!(check_status(&report, id), "skip", "{id}: {report}");
    }
    assert!(report["key"]["verified"].is_null(), "{report}");
    assert!(server.requests().is_empty(), "offline doctor made requests");
}

#[test]
fn network_checks_are_free_and_report_reachability_auth_and_models() {
    let env = TestEnv::new();
    let server = logged_in_server(&env);

    let output = run(&env, &["doctor", "--network", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(report["network"], true);
    assert_eq!(report["verify"], false);
    assert_eq!(report["key"]["verified"], true);
    assert_eq!(check_status(&report, "network.reachable"), "pass");
    assert_eq!(check_status(&report, "network.auth"), "pass");
    assert_eq!(check_status(&report, "network.verify"), "skip");
    for harness in ["claude", "pi", "kimi"] {
        assert_eq!(
            check_status(&report, &format!("harness.{harness}.models")),
            "pass",
            "{harness}: {report}"
        );
    }

    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == "GET" && request.path == "/v1/models")
            .count(),
        1,
        "reachability must use GET /v1/models once"
    );
    assert!(
        requests
            .iter()
            .any(|request| request.path.starts_with("/v1/generation/")),
        "auth probe must run"
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.path == "/v1/chat/completions"),
        "--network must not send a real chat request"
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.path == "/v1/messages"),
        "--network must not send /v1/messages"
    );
    for model in [
        "zhipu/glm-5.3",
        "deepseek/deepseek-v4-pro",
        "zhipu/glm-5.3-flash",
        "kimi/kimi-k2.7-code",
        "tokencamp/auto",
    ] {
        assert!(
            requests
                .iter()
                .any(|request| request.path == format!("/v1/models/{model}")),
            "missing online model check for {model}: {requests:?}"
        );
    }
}

#[test]
fn network_checks_skip_without_a_key_and_make_no_requests() {
    let env = TestEnv::new();
    let server = default_server();

    let output = env
        .command()
        .args(["doctor", "--network", "--json"])
        .env("TOKENCAMP_BASE_URL", server.url(""))
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert!(report["key"].is_null(), "{report}");
    assert_eq!(check_status(&report, "credential.key"), "fail");
    for id in ["network.reachable", "network.auth", "network.verify"] {
        assert_eq!(check_status(&report, id), "skip", "{id}: {report}");
    }
    assert!(server.requests().is_empty(), "no key must make no requests");
}

#[test]
fn reachable_non_2xx_and_network_errors_fail() {
    let env = TestEnv::new();
    let server = network_server(
        MockResponse::json(500, r#"{"error":{"code":"internal"}}"#),
        auth_ok(),
        chat_ok(),
        messages_ok(),
    );
    env.login(&server.url(""), KEY);

    let output = run(&env, &["doctor", "--network", "--json"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(check_status(&report, "network.reachable"), "fail");
    assert_eq!(check_status(&report, "network.auth"), "pass");
    assert!(
        check(&report, "network.reachable")["message"]
            .as_str()
            .expect("message")
            .contains("500"),
        "{report}"
    );

    let env = TestEnv::new();
    let output = env
        .command()
        .args(["doctor", "--network", "--json"])
        .env("TOKENCAMP_API_KEY", KEY)
        .env("TOKENCAMP_BASE_URL", env.unused_loopback_url())
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(check_status(&report, "network.reachable"), "fail");
    assert_eq!(check_status(&report, "network.auth"), "fail");
    assert_eq!(report["key"]["verified"], true);
}

#[test]
fn auth_probe_status_mapping_matches_the_contract() {
    let cases: &[(u16, &str, &str, Option<i32>)] = &[
        (200, r#"{"ok":true}"#, "pass", Some(0)),
        (
            404,
            r#"{"error":{"code":"generation_not_found"}}"#,
            "pass",
            Some(0),
        ),
        (
            401,
            r#"{"error":{"code":"authentication"}}"#,
            "fail",
            Some(1),
        ),
        (404, r#"{"error":{"code":"not_found"}}"#, "fail", Some(1)),
        (
            402,
            r#"{"error":{"code":"payment_required"}}"#,
            "warn",
            Some(0),
        ),
        (403, r#"{"error":{"code":"forbidden"}}"#, "warn", Some(0)),
        (429, r#"{"error":{"code":"rate_limit"}}"#, "warn", Some(0)),
        (500, r#"{"error":{"code":"internal"}}"#, "fail", Some(1)),
    ];

    for (status, body, expected, exit) in cases {
        let env = TestEnv::new();
        let server = network_server(
            reachable_ok(),
            MockResponse::json(*status, *body).with_header("x-request-id", "req_auth"),
            chat_ok(),
            messages_ok(),
        );
        env.login(&server.url(""), KEY);

        let output = run(&env, &["doctor", "--network", "--json"]);

        assert_eq!(
            output.status.code(),
            *exit,
            "HTTP {status} stderr: {}",
            stderr(&output)
        );
        let report = json(&output);
        assert_eq!(
            check_status(&report, "network.auth"),
            *expected,
            "HTTP {status}: {report}"
        );
        let auth = check(&report, "network.auth");
        assert_eq!(auth["request_id"], "req_auth", "HTTP {status}: {auth}");
        assert_eq!(report["key"]["verified"], true, "HTTP {status}: {report}");
        assert!(!stdout(&output).contains(KEY), "API key leaked");
    }
}

#[test]
fn verify_implies_network_and_sends_one_minimal_chat_request() {
    let env = TestEnv::new();
    let server = network_server(
        reachable_ok(),
        auth_ok(),
        chat_ok().with_header("x-request-id", "req_verify"),
        messages_ok(),
    );
    env.login(&server.url(""), KEY);

    let output = run(&env, &["doctor", "--verify", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(report["network"], true);
    assert_eq!(report["verify"], true);
    assert_eq!(report["key"]["verified"], true);
    assert_eq!(check_status(&report, "network.reachable"), "pass");
    assert_eq!(check_status(&report, "network.auth"), "pass");
    assert_eq!(check_status(&report, "network.verify"), "pass");
    assert_eq!(check(&report, "network.verify")["request_id"], "req_verify");
    assert!(!stdout(&output).contains(KEY), "API key leaked");

    let requests = server.requests();
    assert!(
        requests
            .iter()
            .any(|request| request.path.starts_with("/v1/generation/")),
        "--verify must imply --network and run the auth probe"
    );
    let chat: Vec<_> = requests
        .iter()
        .filter(|request| request.path == "/v1/chat/completions")
        .collect();
    assert_eq!(chat.len(), 1, "exactly one real request: {chat:?}");
    assert_eq!(chat[0].method, "POST");
    assert_eq!(
        request_header(chat[0], "authorization"),
        Some(format!("Bearer {KEY}")).as_deref()
    );
    let body: Value = serde_json::from_str(&chat[0].body).expect("chat body is JSON");
    assert_eq!(body["model"], "zhipu/glm-5.3");
    assert_eq!(body["max_tokens"], 1);
    assert_eq!(body["stream"], false);
    let messages = body["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert!(
        !requests
            .iter()
            .any(|request| request.path == "/v1/messages"),
        "chat verification must not send /v1/messages"
    );
}

#[test]
fn verify_status_mapping_matches_the_contract() {
    let cases: &[(u16, &str, &str, Option<i32>)] = &[
        (
            200,
            r#"{"id":"chat","choices":[{"message":{"content":"pong"}}]}"#,
            "pass",
            Some(0),
        ),
        (
            401,
            r#"{"error":{"code":"authentication"}}"#,
            "fail",
            Some(1),
        ),
        (
            402,
            r#"{"error":{"code":"payment_required"}}"#,
            "fail",
            Some(1),
        ),
        (400, r#"{"error":{"code":"bad_request"}}"#, "fail", Some(1)),
        (404, MODEL_NOT_FOUND, "fail", Some(1)),
        (500, r#"{"error":{"code":"internal"}}"#, "fail", Some(1)),
        (403, r#"{"error":{"code":"forbidden"}}"#, "warn", Some(0)),
        (429, r#"{"error":{"code":"rate_limit"}}"#, "warn", Some(0)),
    ];

    for (status, body, expected, exit) in cases {
        let env = TestEnv::new();
        let server = network_server(
            reachable_ok(),
            auth_ok(),
            MockResponse::json(*status, *body).with_header("x-request-id", "req_chat"),
            messages_ok(),
        );
        env.login(&server.url(""), KEY);

        let output = run(&env, &["doctor", "--verify", "--json"]);

        assert_eq!(
            output.status.code(),
            *exit,
            "HTTP {status} stderr: {}",
            stderr(&output)
        );
        let report = json(&output);
        assert_eq!(
            check_status(&report, "network.verify"),
            *expected,
            "HTTP {status}: {report}"
        );
        assert_eq!(
            requests_to(&server, "POST", "/v1/chat/completions"),
            1,
            "HTTP {status} must not retry"
        );
        if *status == 404 {
            assert!(
                !check(&report, "network.verify")["hint"]
                    .as_str()
                    .expect("hint")
                    .contains("anthropic"),
                "chat 404 must not give the Anthropic deployment hint: {report}"
            );
        }
    }
}

fn configure_claude(env: &TestEnv, server: &MockServer) {
    env.login(&server.url(""), KEY);
    let output = run(env, &["claude", "--no-launch"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "configure stderr: {}",
        stderr(&output)
    );
}

#[test]
fn claude_verify_sends_one_messages_request_instead_of_chat() {
    let env = TestEnv::new();
    let server = network_server(
        reachable_ok(),
        auth_ok(),
        chat_ok(),
        messages_ok().with_header("x-request-id", "req_messages"),
    );
    configure_claude(&env, &server);

    let output = run(
        &env,
        &["doctor", "--verify", "--harness", "claude", "--json"],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(report["verify"], true);
    assert_eq!(check_status(&report, "network.verify"), "skip");
    assert_eq!(
        check(&report, "network.verify")["message"],
        "handled by harness.claude.messages"
    );
    assert_eq!(check_status(&report, "harness.claude.messages"), "pass");
    assert_eq!(
        check(&report, "harness.claude.messages")["request_id"],
        "req_messages"
    );
    assert!(!stdout(&output).contains(KEY), "API key leaked");

    let requests = server.requests();
    assert_eq!(requests_to(&server, "POST", "/v1/messages"), 1);
    assert_eq!(requests_to(&server, "POST", "/v1/chat/completions"), 0);
    let messages: Vec<_> = requests
        .iter()
        .filter(|request| request.path == "/v1/messages")
        .collect();
    assert_eq!(
        request_header(messages[0], "anthropic-version"),
        Some("2023-06-01")
    );
    assert_eq!(
        request_header(messages[0], "authorization"),
        Some(format!("Bearer {KEY}")).as_deref()
    );
    let body: Value = serde_json::from_str(&messages[0].body).expect("messages body is JSON");
    assert_eq!(body["model"], "zhipu/glm-5.3");
    assert_eq!(body["max_tokens"], 1);
    assert_eq!(body["stream"], false);
    assert_eq!(body["messages"].as_array().expect("messages").len(), 1);
}

#[test]
fn claude_messages_status_mapping_and_deployment_hint() {
    let cases: &[(u16, &str, &str, Option<i32>)] = &[
        (
            200,
            r#"{"id":"msg","type":"message","content":[]}"#,
            "pass",
            Some(0),
        ),
        (
            401,
            r#"{"error":{"code":"authentication"}}"#,
            "fail",
            Some(1),
        ),
        (
            402,
            r#"{"error":{"code":"payment_required"}}"#,
            "fail",
            Some(1),
        ),
        (400, r#"{"error":{"code":"bad_request"}}"#, "fail", Some(1)),
        (404, MODEL_NOT_FOUND, "fail", Some(1)),
        (500, r#"{"error":{"code":"internal"}}"#, "fail", Some(1)),
        (403, r#"{"error":{"code":"forbidden"}}"#, "warn", Some(0)),
        (429, r#"{"error":{"code":"rate_limit"}}"#, "warn", Some(0)),
    ];

    for (status, body, expected, exit) in cases {
        let env = TestEnv::new();
        let server = network_server(
            reachable_ok(),
            auth_ok(),
            chat_ok(),
            MockResponse::json(*status, *body),
        );
        configure_claude(&env, &server);

        let output = run(
            &env,
            &["doctor", "--verify", "--harness", "claude", "--json"],
        );

        assert_eq!(
            output.status.code(),
            *exit,
            "HTTP {status} stderr: {}",
            stderr(&output)
        );
        let report = json(&output);
        assert_eq!(
            check_status(&report, "harness.claude.messages"),
            *expected,
            "HTTP {status}: {report}"
        );
        if *status == 404 {
            let messages = check(&report, "harness.claude.messages");
            assert_eq!(messages["code"], "model_not_found", "{messages}");
            let hint = messages["hint"].as_str().expect("hint");
            assert!(hint.contains("provider_type: anthropic"), "{hint}");
            assert!(hint.contains("--harness pi"), "{hint}");
        }
        assert_eq!(
            requests_to(&server, "POST", "/v1/messages"),
            1,
            "HTTP {status} must not retry"
        );
    }
}

#[test]
fn selecting_all_harnesses_verifies_with_chat_and_leaves_messages_unverified() {
    let env = TestEnv::new();
    let server = network_server(reachable_ok(), auth_ok(), chat_ok(), messages_ok());
    configure_claude(&env, &server);

    let output = run(&env, &["doctor", "--verify", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(check_status(&report, "network.verify"), "pass");
    assert_eq!(check_status(&report, "harness.claude.messages"), "warn");
    assert_eq!(requests_to(&server, "POST", "/v1/chat/completions"), 1);
    assert_eq!(requests_to(&server, "POST", "/v1/messages"), 0);
}

#[test]
fn verify_without_a_key_skips_the_real_request() {
    let env = TestEnv::new();
    let server = default_server();
    configure_claude(&env, &server);
    let logout = run(&env, &["logout"]);
    assert_eq!(logout.status.code(), Some(0), "stderr: {}", stderr(&logout));
    let requests_before = server.requests().len();

    let output = env
        .command()
        .args(["doctor", "--verify", "--harness", "claude", "--json"])
        .env("TOKENCAMP_BASE_URL", server.url(""))
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(check_status(&report, "credential.key"), "fail");
    for id in ["network.reachable", "network.auth", "network.verify"] {
        assert_eq!(check_status(&report, id), "skip", "{id}: {report}");
    }
    assert_eq!(
        check_status(&report, "harness.claude.messages"),
        "skip",
        "{report}"
    );
    assert_eq!(
        server.requests().len(),
        requests_before,
        "no key must not send the real request"
    );
}

#[test]
fn online_model_checks_fail_on_missing_or_unverified_models() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(vec![
        ("/v1/models", reachable_ok()),
        ("/v1/generation/*", auth_ok()),
        (
            "/v1/models/zhipu/glm-5.3-flash",
            MockResponse::json(404, r#"{"error":{"code":"not_found"}}"#),
        ),
        ("/v1/models/*", model_ok()),
    ]);
    env.login(&server.url(""), KEY);

    let output = run(&env, &["doctor", "--network", "--json"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(
        check_status(&report, "harness.pi.models"),
        "fail",
        "{report}"
    );
    assert!(
        check(&report, "harness.pi.models")["message"]
            .as_str()
            .expect("message")
            .contains("zhipu/glm-5.3-flash"),
        "{report}"
    );

    let env = TestEnv::new();
    let server = MockServer::start_routes(vec![
        ("/v1/models", reachable_ok()),
        ("/v1/generation/*", auth_ok()),
        (
            "/v1/models/kimi/kimi-k2.7-code",
            MockResponse::json(500, r#"{"error":{"code":"internal"}}"#),
        ),
        ("/v1/models/*", model_ok()),
    ]);
    env.login(&server.url(""), KEY);

    let output = run(&env, &["doctor", "--network", "--json"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(
        check_status(&report, "harness.kimi.models"),
        "fail",
        "{report}"
    );
}

#[test]
fn reachable_timeout_fails_and_the_auth_probe_still_runs() {
    let env = TestEnv::new();
    let server = network_server(
        reachable_ok().with_delay(std::time::Duration::from_secs(11)),
        auth_ok(),
        chat_ok(),
        messages_ok(),
    );
    env.login(&server.url(""), KEY);
    let started = std::time::Instant::now();

    let output = run(
        &env,
        &["doctor", "--network", "--harness", "claude", "--json"],
    );
    let elapsed = started.elapsed();

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let report = json(&output);
    assert_eq!(check_status(&report, "network.reachable"), "fail");
    assert!(
        check(&report, "network.reachable")["message"]
            .as_str()
            .expect("message")
            .contains("timed out"),
        "{report}"
    );
    assert_eq!(check_status(&report, "network.auth"), "pass");
    assert!(
        elapsed >= std::time::Duration::from_secs(9),
        "the reachability check must wait for its timeout: {elapsed:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(20),
        "the auth probe must not wait for the stalled response: {elapsed:?}"
    );
}

#[test]
fn claude_online_model_checks_deduplicate_slots() {
    let env = TestEnv::new();
    let server = logged_in_server(&env);

    let output = run(
        &env,
        &["doctor", "--network", "--harness", "claude", "--json"],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path == "/v1/models/deepseek/deepseek-v4-pro")
            .count(),
        1,
        "opus and fable share a model and must be queried once: {requests:?}"
    );
    for model in [
        "zhipu/glm-5.3",
        "deepseek/deepseek-v4-pro",
        "zhipu/glm-5.3-flash",
    ] {
        assert!(
            requests
                .iter()
                .any(|request| request.path == format!("/v1/models/{model}")),
            "missing {model}: {requests:?}"
        );
    }
    assert!(
        !requests
            .iter()
            .any(|request| request.path == "/v1/models/tokencamp/auto"),
        "Claude must not check the Pi/Kimi auto model: {requests:?}"
    );
}

#[test]
fn network_harness_filter_and_usage_errors_behave_like_the_offline_doctor() {
    let env = TestEnv::new();
    let _server = logged_in_server(&env);

    let output = run(
        &env,
        &["doctor", "--network", "--harness", "claude", "--json"],
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}\nstdout: {}",
        stderr(&output),
        stdout(&output)
    );
    let report = json(&output);
    let harnesses = report["harnesses"].as_object().expect("harnesses");
    assert_eq!(harnesses.len(), 1);
    assert!(harnesses.contains_key("claude"));
    assert_eq!(check_status(&report, "network.reachable"), "pass");

    let cases: &[&[&str]] = &[
        &["doctor", "--network", "--harness", "nope", "--json"],
        &[
            "doctor",
            "--network",
            "--harness",
            "claude",
            "--harness",
            "pi",
            "--json",
        ],
    ];
    for args in cases {
        let output = run(&env, args);
        assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
        let value = json(&output);
        assert_eq!(value["error"]["code"], "usage");
        assert!(value.get("checks").is_none());
    }
}

#[test]
fn request_ids_are_verbose_only_in_human_output() {
    let env = TestEnv::new();
    let server = network_server(
        reachable_ok(),
        auth_ok().with_header("x-request-id", "req_auth"),
        chat_ok(),
        messages_ok(),
    );
    env.login(&server.url(""), KEY);

    let output = run(&env, &["doctor", "--network"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(!text.contains("request_id:"), "board: {text}");

    let output = run(&env, &["-v", "doctor", "--network"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("request_id: req_auth"), "board: {text}");
}
