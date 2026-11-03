mod support;

use std::fs;

use serde_json::Value;
use support::TestEnv;

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";
const BASE_URL: &str = "https://tokencamp.example";

fn configure(env: &TestEnv) {
    env.login(BASE_URL, KEY);
    env.command()
        .args(["claude", "--no-launch"])
        .assert()
        .success();
}

fn overlay(env: &TestEnv) -> Value {
    let path = env
        .xdg_config
        .join("tokencamp/harnesses/claude/settings.json");
    serde_json::from_str(&fs::read_to_string(path).expect("read overlay")).expect("overlay is JSON")
}

#[test]
fn claude_uses_the_static_slot_mapping() {
    let env = TestEnv::new();
    configure(&env);
    let overlay = overlay(&env);
    let values = overlay["env"].as_object().expect("env object");

    assert_eq!(values["ANTHROPIC_MODEL"], "zhipu/glm-5.3");
    assert_eq!(
        values["ANTHROPIC_DEFAULT_OPUS_MODEL"],
        "deepseek/deepseek-v4-pro"
    );
    assert_eq!(values["ANTHROPIC_DEFAULT_SONNET_MODEL"], "zhipu/glm-5.3");
    assert_eq!(
        values["ANTHROPIC_DEFAULT_HAIKU_MODEL"],
        "zhipu/glm-5.3-flash"
    );
    assert_eq!(
        values["ANTHROPIC_DEFAULT_FABLE_MODEL"],
        "deepseek/deepseek-v4-pro"
    );
    assert_eq!(values["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "1048576");
    assert_eq!(values["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], "32768");
}

#[test]
fn claude_does_not_write_unsupported_or_secret_keys() {
    let env = TestEnv::new();
    configure(&env);
    let overlay = overlay(&env);
    let values = overlay["env"].as_object().expect("env object");
    let rendered = overlay.to_string();

    for forbidden in [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_SMALL_FAST_MODEL",
        "ANTHROPIC_CUSTOM_MODEL_OPTION",
        "ANTHROPIC_DEFAULT_OPUS_MODEL_SUPPORTED_CAPABILITIES",
        "CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY",
        "modelPicker",
    ] {
        assert!(
            !values.contains_key(forbidden) && !rendered.contains(forbidden),
            "{forbidden} must not be written"
        );
    }
    assert!(!rendered.contains("@preset"));
    assert!(!rendered.contains("~family"));
    assert!(!rendered.contains(KEY));
    assert_eq!(values["ANTHROPIC_API_KEY"], "");
    assert_eq!(values["ANTHROPIC_BASE_URL"], BASE_URL);
}

#[test]
fn claude_writes_the_overlay_marker_and_schema() {
    let env = TestEnv::new();
    configure(&env);
    let overlay = overlay(&env);

    assert_eq!(
        overlay["$schema"],
        "https://json.schemastore.org/claude-code-settings.json"
    );
    assert_eq!(overlay["_tcamp"]["version"], 1);
    assert_eq!(overlay["_tcamp"]["harness"], "claude");
    assert_eq!(
        overlay
            .as_object()
            .expect("overlay object")
            .keys()
            .collect::<Vec<_>>(),
        vec!["$schema", "_tcamp", "env"]
    );
}
