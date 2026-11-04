mod support;

use std::process::Output;

use serde_json::Value;
use support::TestEnv;

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

fn check_fields() -> [&'static str; 8] {
    [
        "id",
        "scope",
        "harness",
        "status",
        "message",
        "hint",
        "code",
        "request_id",
    ]
}

#[test]
fn doctor_json_has_the_stable_offline_shape() {
    let env = TestEnv::new();

    let output = run(&env, &["doctor", "--json"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let value = json(&output);
    assert_eq!(value["ok"], false);
    assert_eq!(value["base_url"], "https://api.tokencamp.com");
    assert!(value["key"].is_null());
    assert_eq!(value["network"], false);
    assert_eq!(value["verify"], false);

    for harness in ["claude", "pi", "kimi"] {
        let report = &value["harnesses"][harness];
        assert!(report["installed"].is_null(), "{harness}: {report}");
        assert_eq!(report["configured"], false, "{harness}: {report}");
        assert!(report["target"].is_null(), "{harness}: {report}");
    }

    let checks = value["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty(), "checks must not be empty");
    for check in checks {
        for field in check_fields() {
            assert!(
                check.get(field).is_some(),
                "check is missing {field:?}: {check}"
            );
        }
        assert!(
            check["request_id"].is_null(),
            "offline checks have no request id: {check}"
        );
        assert!(
            ["connection", "network", "harness"]
                .contains(&check["scope"].as_str().expect("check scope")),
            "unexpected scope: {check}"
        );
    }

    let summary = &value["summary"];
    let total = ["pass", "warn", "fail", "skip"]
        .iter()
        .map(|key| summary[key].as_u64().expect("summary count"))
        .sum::<u64>();
    assert_eq!(
        total as usize,
        checks.len(),
        "summary counts only emitted checks"
    );
    assert_eq!(value["ok"], summary["fail"] == 0);
    assert!(summary["fail"].as_u64().unwrap() >= 1);

    let warn_ids: Vec<&str> = checks
        .iter()
        .filter(|check| check["status"] == "warn")
        .map(|check| check["id"].as_str().expect("check id"))
        .collect();
    let warnings: Vec<&str> = value["warnings"]
        .as_array()
        .expect("warnings array")
        .iter()
        .map(|warning| warning.as_str().expect("warning id"))
        .collect();
    assert_eq!(warnings, warn_ids);
    assert!(warnings.contains(&"credential.file"));
    assert!(!stdout(&output).contains(KEY));
}

#[test]
fn doctor_json_with_a_key_reports_source_and_offline_verification() {
    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    env.fake_harness("claude", "2.1.316");
    env.fake_harness("pi", "1.4.0");
    env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["doctor", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json(&output);
    assert_eq!(value["ok"], true);
    assert_eq!(value["key"]["masked"], MASKED);
    assert_eq!(value["key"]["source"], "file");
    assert!(
        value["key"]["verified"].is_null(),
        "offline doctor does not verify: {value}"
    );
    assert!(value["key"]["verified_at"].is_null());
    assert_eq!(value["summary"]["fail"], 0);
    assert_eq!(value["harnesses"]["claude"]["installed"], "2.1.316");
    assert_eq!(value["harnesses"]["pi"]["installed"], "1.4.0");
    assert_eq!(value["harnesses"]["kimi"]["installed"], "2.1.1");
    assert!(value["warnings"].as_array().expect("warnings").is_empty());
    assert!(!stdout(&output).contains(KEY), "full API key leaked");
}

#[test]
fn harness_filter_only_filters_the_harness_blocks() {
    let env = TestEnv::new();
    env.login("https://example.test", KEY);

    let output = run(&env, &["doctor", "--harness", "claude", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let value = json(&output);
    let harnesses = value["harnesses"].as_object().expect("harnesses object");
    assert_eq!(harnesses.len(), 1);
    assert!(harnesses.contains_key("claude"));

    let checks = value["checks"].as_array().expect("checks array");
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "connection.base_url")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "network.reachable")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "harness.claude.messages")
    );
    for check in checks {
        let id = check["id"].as_str().expect("check id");
        assert!(
            !id.contains(".pi.") && !id.contains(".kimi."),
            "filter leaked another harness: {check}"
        );
        if check["scope"] == "harness" {
            assert_eq!(check["harness"], "claude");
        }
    }

    let human = run(&env, &["doctor", "--harness", "claude"]);
    assert_eq!(human.status.code(), Some(0), "stderr: {}", stderr(&human));
    let text = stdout(&human);
    assert!(text.contains("  claude"), "board: {text}");
    assert!(!text.contains("  pi\n"), "board leaked pi: {text}");
    assert!(!text.contains("  kimi\n"), "board leaked kimi: {text}");
}

#[test]
fn invalid_and_repeated_harness_filters_are_usage_errors() {
    let env = TestEnv::new();

    let cases: &[&[&str]] = &[
        &["doctor", "--harness", "nope", "--json"],
        &["doctor", "--harness", "claude", "--harness", "pi", "--json"],
    ];
    for args in cases {
        let output = run(&env, args);

        assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
        let value = json(&output);
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "usage");
        assert!(
            !value["error"]["hint"].as_str().expect("hint").is_empty(),
            "usage errors must carry a hint: {value}"
        );
        assert!(
            value.get("checks").is_none(),
            "usage errors are not reports"
        );
    }

    let output = run(&env, &["doctor", "--harness", "nope"]);
    assert_eq!(output.status.code(), Some(2));
    let text = stderr(&output);
    assert!(text.contains("[usage]"), "stderr: {text}");
    assert!(text.contains("hint:"), "stderr: {text}");
}
