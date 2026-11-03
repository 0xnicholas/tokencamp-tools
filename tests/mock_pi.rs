mod support;

use serde_json::{Value, json};
use support::{MockResponse, MockServer, TestEnv};

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
    env.command().args(["pi", "--no-launch"]).assert().success();

    let models: Value = jsonc_parser::parse_to_serde_value(
        &std::fs::read_to_string(env.pi_models_path()).expect("read models.json"),
        &Default::default(),
    )
    .expect("models.json is valid JSONC");
    let provider = &models["providers"]["tokencamp"];
    assert_eq!(provider["api"], "openai-completions");
    assert_ne!(provider["api"], "openai-responses");
    let base_url = provider["baseUrl"].as_str().expect("baseUrl");

    let client = reqwest::blocking::Client::builder()
        .build()
        .expect("HTTP client");
    let first = client
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(KEY)
        .json(&json!({
            "model": "zhipu/glm-5.3",
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
            "model": "zhipu/glm-5.3",
            "messages": [
                {"role": "user", "content": "read foo"},
                {"role": "assistant", "content": null, "tool_calls": [tool_call]},
                {"role": "tool", "tool_call_id": "call_1", "content": "file contents"}
            ],
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
    assert_eq!(chats.len(), 2, "the tool loop must use chat completions");
    for request in &chats {
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("authorization")
                    && value == &format!("Bearer {KEY}")),
            "every round must carry the Bearer key: {request:?}"
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
