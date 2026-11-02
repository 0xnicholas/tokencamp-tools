mod support;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;

use support::{MockResponse, MockServer, TestEnv};

#[test]
fn test_env_isolates_home_and_xdg_directories() {
    let first = TestEnv::new();
    let second = TestEnv::new();

    assert_ne!(first.home, second.home);
    assert_ne!(first.xdg_config, second.xdg_config);
    for dir in [
        &first.home,
        &first.xdg_config,
        &first.xdg_state,
        &first.claude_config,
        &first.pi_agent,
        &first.kimi_home,
        &first.bin_dir,
    ] {
        assert!(dir.is_dir(), "{dir:?} should exist");
    }
    assert_ne!(first.claude_config, second.claude_config);
    assert_ne!(first.pi_agent, second.pi_agent);
    assert_ne!(first.kimi_home, second.kimi_home);
}

#[test]
fn fake_harness_reports_its_version_and_records_argv_and_env() {
    let env = TestEnv::new();
    let harness = env.fake_harness("pi", "1.4.0");

    let version = Command::new(harness.path())
        .env_clear()
        .env("PATH", &env.bin_dir)
        .arg("--version")
        .output()
        .expect("run fake harness");
    assert_eq!(String::from_utf8_lossy(&version.stdout), "pi 1.4.0\n");

    let launch = Command::new(harness.path())
        .env_clear()
        .env("PATH", &env.bin_dir)
        .args(["--provider", "tokencamp", "--model", "zhipu/glm-5.3"])
        .env("TOKENCAMP_API_KEY", "sk-sentinel")
        .output()
        .expect("run fake harness");
    assert!(launch.status.success());

    assert_eq!(
        harness.args(),
        ["--provider", "tokencamp", "--model", "zhipu/glm-5.3"]
    );
    assert_eq!(
        harness.env_value("TOKENCAMP_API_KEY").as_deref(),
        Some("sk-sentinel")
    );
}

#[test]
fn all_three_fake_harnesses_are_detected_by_the_binary() {
    let env = TestEnv::new();
    env.fake_harness("claude", "2.1.316");
    env.fake_harness("pi", "1.4.0");
    env.fake_harness("kimi", "2.1.1");

    let output = env.command().output().expect("run tcamp");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("Detected harnesses: claude, pi, kimi"));
}

#[test]
fn mock_server_answers_on_loopback_and_records_requests() {
    let server = MockServer::start(MockResponse::json(200, r#"{"ok":true}"#));
    let url = server.url("/v1/models");
    assert!(url.starts_with("http://127.0.0.1:"));
    let address = server.address();

    let mut get = TcpStream::connect(&address).expect("connect to mock server");
    write!(
        get,
        "GET /v1/models HTTP/1.1\r\nHost: {address}\r\nAccept: application/json\r\n\r\n"
    )
    .expect("write GET");
    let mut get_response = String::new();
    get.read_to_string(&mut get_response)
        .expect("read GET response");
    assert!(get_response.starts_with("HTTP/1.1 200 OK"));
    assert!(get_response.contains(r#"{"ok":true}"#));

    let body = r#"{"prompt":"hi"}"#;
    let mut post = TcpStream::connect(&address).expect("connect to mock server");
    write!(
        post,
        "POST /v1/chat/completions HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("write POST");
    let mut post_response = String::new();
    post.read_to_string(&mut post_response)
        .expect("read POST response");
    assert!(post_response.starts_with("HTTP/1.1 200 OK"));

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/v1/models");
    assert!(requests[0].headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("accept") && value == "application/json"
    }));
    assert_eq!(requests[1].method, "POST");
    assert_eq!(requests[1].path, "/v1/chat/completions");
    assert_eq!(requests[1].body, body);
}
