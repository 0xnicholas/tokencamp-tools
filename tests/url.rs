mod support;

use std::fs;
use std::process::Output;

use serde_json::Value;
use support::TestEnv;
use toml_edit::DocumentMut;

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";

fn run(env: &TestEnv, args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = env.command();
    command.args(args);
    if let Some(input) = stdin {
        command.write_stdin(input);
    }
    command.output().expect("run tcamp")
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is one JSON object")
}

fn credentials(env: &TestEnv) -> DocumentMut {
    fs::read_to_string(env.credentials_path())
        .expect("read credentials file")
        .parse()
        .expect("parse credentials file")
}

fn warning(value: &Value, code: &str) -> bool {
    value["warnings"]
        .as_array()
        .is_some_and(|warnings| warnings.iter().any(|warning| warning["code"] == code))
}

fn login(env: &TestEnv, base_url: &str) -> Output {
    run(
        env,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            base_url,
            "--json",
        ],
        Some(KEY),
    )
}

#[test]
fn base_urls_are_normalized_before_storage() {
    for (input, expected) in [
        ("https://host/v1/", "https://host"),
        ("https://host/prefix/v1", "https://host/prefix"),
        ("https://host/prefix/v1/", "https://host/prefix"),
        ("https://host/", "https://host"),
        ("https://host/prefix", "https://host/prefix"),
        ("https://host/v1/v1", "https://host/v1"),
        ("https://HOST/v1", "https://host"),
    ] {
        let env = TestEnv::new();
        let output = login(&env, input);
        assert_eq!(
            output.status.code(),
            Some(0),
            "input {input} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(json_output(&output)["base_url"], expected, "input {input}");
        assert_eq!(
            credentials(&env)["active"].as_str(),
            Some(expected),
            "input {input}"
        );
    }
}

#[test]
fn base_url_precedence_is_flag_env_active_default() {
    let env = TestEnv::new();

    let output = run(&env, &["status", "--json"], None);
    assert_eq!(
        json_output(&output)["base_url"],
        "https://api.tokencamp.com"
    );

    let output = login(&env, "https://active.test");
    assert_eq!(output.status.code(), Some(0));
    let output = run(&env, &["status", "--json"], None);
    assert_eq!(json_output(&output)["base_url"], "https://active.test");

    let mut command = env.command();
    command
        .env("TOKENCAMP_BASE_URL", "https://env.test")
        .args(["status", "--json"]);
    let output = command.output().expect("run status with env base url");
    assert_eq!(json_output(&output)["base_url"], "https://env.test");

    let mut command = env.command();
    command
        .env("TOKENCAMP_BASE_URL", "https://env.test")
        .args([
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            "https://flag.test",
            "--json",
        ])
        .write_stdin(KEY);
    let output = command.output().expect("run login with flag and env");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json_output(&output)["base_url"], "https://flag.test");

    let output = run(&env, &["status", "--json"], None);
    assert_eq!(json_output(&output)["base_url"], "https://flag.test");

    let mut command = env.command();
    command
        .env("TOKENCAMP_BASE_URL", "https://env-only.test")
        .args(["login", "--key-stdin", "--no-verify", "--json"])
        .write_stdin(KEY);
    let output = command.output().expect("run login from env base url");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json_output(&output)["base_url"], "https://env-only.test");

    let mut command = env.command();
    command
        .env("TOKENCAMP_BASE_URL", "")
        .args(["status", "--json"]);
    let output = command
        .output()
        .expect("run status with empty env base url");
    assert_eq!(json_output(&output)["base_url"], "https://env-only.test");
}

#[test]
fn active_connection_follows_successful_logins_and_never_auto_switches() {
    let env = TestEnv::new();
    login(&env, "https://a.test");
    login(&env, "https://b.test");
    let document = credentials(&env);
    assert_eq!(document["active"].as_str(), Some("https://b.test"));
    assert_eq!(document["connections"].as_table().unwrap().len(), 2);

    let output = run(&env, &["logout", "--json"], None);
    assert_eq!(output.status.code(), Some(0));
    let document = credentials(&env);
    assert!(document.get("active").is_none());
    assert!(
        document["connections"]
            .as_table()
            .unwrap()
            .contains_key("https://a.test")
    );

    login(&env, "https://b.test");
    let output = run(
        &env,
        &["logout", "--base-url", "https://a.test", "--json"],
        None,
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(credentials(&env)["active"].as_str(), Some("https://b.test"));

    let output = run(
        &env,
        &["logout", "--base-url", "https://b.test", "--json"],
        None,
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(credentials(&env).get("active").is_none());
}

#[test]
fn public_plain_http_is_rejected() {
    for input in [
        "http://example.com",
        "http://8.8.8.8",
        "http://127.0.0.1.evil.com",
    ] {
        let env = TestEnv::new();
        let output = login(&env, input);
        assert_eq!(
            output.status.code(),
            Some(2),
            "input {input} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(json_output(&output)["error"]["code"], "usage");
        assert!(!env.credentials_path().exists());
    }
}

#[test]
fn private_and_loopback_plain_http_is_allowed_with_a_warning() {
    for input in [
        "http://127.0.0.1:9",
        "http://localhost:9",
        "http://10.1.2.3",
        "http://172.16.0.1",
        "http://192.168.1.1",
        "http://169.254.10.10",
        "http://[::1]:9",
        "http://[fe80::1]:9",
        "http://[fd00::1]:9",
    ] {
        let env = TestEnv::new();
        let output = login(&env, input);
        assert_eq!(
            output.status.code(),
            Some(0),
            "input {input} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value = json_output(&output);
        assert!(warning(&value, "insecure_http"), "input {input}");
        assert!(env.credentials_path().exists(), "input {input}");
    }
}

#[test]
fn https_is_allowed_without_an_insecure_http_warning() {
    let env = TestEnv::new();
    let output = login(&env, "https://example.test");
    assert_eq!(output.status.code(), Some(0));
    assert!(!warning(&json_output(&output), "insecure_http"));
}

#[test]
fn status_applies_the_http_policy_without_network_requests() {
    let env = TestEnv::new();
    let mut command = env.command();
    command
        .env("TOKENCAMP_BASE_URL", "http://example.com")
        .args(["status", "--json"]);
    let output = command.output().expect("run status");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(json_output(&output)["error"]["code"], "usage");
}
