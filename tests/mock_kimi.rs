mod support;

use serde_json::{Value, json};
use support::{MockResponse, MockServer, TestEnv};
use toml_edit::{DocumentMut, Item};

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";

fn mock_server() -> MockServer {
    MockServer::start_routes(vec![
        (
            "/v1/models/zhipu/glm-5.3",
            MockResponse::json(200, r#"{"id":"model"}"#),
        ),
        (
            "/v1/chat/completions",
            MockResponse::json(
                200,
                r#"{"id":"chatcmpl-1","object":"chat.completion","choices":[{"index":0,"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"read","arguments":"{\"path\":\"foo\"}"}}]}}]}"#,
            ),
        ),
    ])
}

#[test]
fn the_configured_endpoint_completes_a_two_round_tool_loop() {
    let env = TestEnv::new();
    let server = mock_server();
    env.login(&server.url(""), KEY);
    env.command()
        .args(["kimi", "--no-launch"])
        .assert()
        .success();

    let document: DocumentMut = std::fs::read_to_string(env.kimi_config_path())
        .expect("read config.toml")
        .parse()
        .expect("config.toml is valid TOML");
    let provider = document["providers"]
        .as_table()
        .expect("providers table")
        .get("tokencamp")
        .and_then(Item::as_table)
        .expect("providers.tokencamp table");
    assert_eq!(
        provider.get("type").and_then(Item::as_str),
        Some("openai"),
        "type = anthropic would send x-api-key instead of Bearer"
    );
    let base_url = provider
        .get("base_url")
        .and_then(Item::as_str)
        .expect("base_url");
    assert_eq!(base_url, format!("{}/v1", server.url("")));

    let alias = document["models"]
        .as_table()
        .expect("models table")
        .get("tokencamp/glm-5.3")
        .and_then(Item::as_table)
        .expect("default alias");
    let canonical = alias
        .get("model")
        .and_then(Item::as_str)
        .expect("canonical model");
    assert_eq!(canonical, "zhipu/glm-5.3");
    let max_output_size = alias
        .get("max_output_size")
        .and_then(Item::as_integer)
        .expect("max_output_size");
    assert_eq!(max_output_size, 8192);

    let client = reqwest::blocking::Client::builder()
        .build()
        .expect("HTTP client");
    let first = client
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(KEY)
        .json(&json!({
            "model": canonical,
            "messages": [{"role": "user", "content": "read foo"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "read",
                    "description": "read a file",
                    "parameters": {
                        "type": "object",
                        "properties": {"path": {"type": "string"}},
                        "required": ["path"]
                    }
                }
            }],
            "max_tokens": max_output_size,
            "stream": false
        }))
        .send()
        .expect("round one");
    assert!(first.status().is_success(), "round one: {}", first.status());
    let payload: Value = first.json().expect("round one JSON");
    let tool_call = payload["choices"][0]["message"]["tool_calls"][0].clone();
    assert_eq!(tool_call["id"], "call_1");

    let second = client
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(KEY)
        .json(&json!({
            "model": canonical,
            "messages": [
                {"role": "user", "content": "read foo"},
                {"role": "assistant", "content": null, "tool_calls": [tool_call]},
                {"role": "tool", "tool_call_id": "call_1", "content": "file contents"}
            ],
            "max_tokens": max_output_size,
            "stream": false
        }))
        .send()
        .expect("round two");
    assert!(
        second.status().is_success(),
        "round two: {}",
        second.status()
    );

    let requests = server.requests();
    let chats: Vec<_> = requests
        .iter()
        .filter(|request| request.path == "/v1/chat/completions")
        .collect();
    assert_eq!(
        chats.len(),
        2,
        "the tool loop must use chat completions: {requests:?}"
    );
    for request in &chats {
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("authorization")
                    && value == &format!("Bearer {KEY}")),
            "every round must carry the Bearer key: {request:?}"
        );
        let body: Value = serde_json::from_str(&request.body).expect("request body is JSON");
        assert_eq!(body["model"], "zhipu/glm-5.3");
        assert_eq!(
            body["max_tokens"], 8192,
            "max_output_size must become max_tokens"
        );
    }

    let second_request: Value =
        serde_json::from_str(&chats[1].body).expect("round two body is JSON");
    assert_eq!(second_request["messages"][1]["role"], "assistant");
    assert_eq!(
        second_request["messages"][1]["tool_calls"][0]["id"],
        "call_1"
    );
    assert_eq!(second_request["messages"][2]["role"], "tool");
    assert_eq!(second_request["messages"][2]["tool_call_id"], "call_1");
}
