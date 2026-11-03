use std::time::Duration;

use reqwest::blocking::Response;
use reqwest::header::ACCEPT;
use serde_json::json;
use uuid::Uuid;

use crate::credentials::Warning;
use crate::credentials::base_url::BaseUrl;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const VERIFY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum ProbeResult {
    Verified {
        warning: Option<Warning>,
    },
    Rejected {
        code: Option<String>,
        request_id: Option<String>,
    },
    Indeterminate {
        detail: String,
        request_id: Option<String>,
    },
}

#[derive(Debug)]
pub enum VerifyResult {
    Verified,
    Warning(Warning),
    Rejected {
        code: Option<String>,
        request_id: Option<String>,
    },
    Indeterminate {
        detail: String,
        request_id: Option<String>,
    },
}

pub fn probe(base: &BaseUrl, api_key: &str) -> ProbeResult {
    let client = match crate::http::blocking_client(PROBE_TIMEOUT) {
        Ok(client) => client,
        Err(detail) => {
            return ProbeResult::Indeterminate {
                detail,
                request_id: None,
            };
        }
    };
    let url = base.join(&format!("/v1/generation/{}", Uuid::new_v4()));
    let response = match client
        .get(&url)
        .bearer_auth(api_key)
        .header(ACCEPT, "application/json")
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            return ProbeResult::Indeterminate {
                detail: transport_detail(base, &error),
                request_id: None,
            };
        }
    };

    let status = response.status().as_u16();
    let request_id = request_id(&response, api_key);
    let body = response.text().unwrap_or_default();
    match status {
        200 => ProbeResult::Verified { warning: None },
        401 => ProbeResult::Rejected {
            code: error_code(&body, api_key),
            request_id,
        },
        404 => {
            let code = error_code(&body, api_key);
            if code.as_deref() == Some("generation_not_found") {
                ProbeResult::Verified { warning: None }
            } else {
                ProbeResult::Indeterminate {
                    detail: format!(
                        "received an unexpected HTTP 404 from {base} (expected generation_not_found)"
                    ),
                    request_id,
                }
            }
        }
        400..=499 => {
            let code = warning_code(&body, &format!("probe_{status}"), api_key);
            ProbeResult::Verified {
                warning: Some(warning_for_status(status, code, base)),
            }
        }
        _ => ProbeResult::Indeterminate {
            detail: format!("received unexpected HTTP {status} from {base}"),
            request_id,
        },
    }
}

pub fn verify_chat(base: &BaseUrl, api_key: &str) -> VerifyResult {
    let client = match crate::http::blocking_client(VERIFY_TIMEOUT) {
        Ok(client) => client,
        Err(detail) => {
            return VerifyResult::Indeterminate {
                detail,
                request_id: None,
            };
        }
    };
    let body = json!({
        "model": "auto",
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "ping"}],
        "stream": false,
    });
    let response = match client
        .post(base.join("/v1/chat/completions"))
        .bearer_auth(api_key)
        .json(&body)
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            return VerifyResult::Indeterminate {
                detail: transport_detail(base, &error),
                request_id: None,
            };
        }
    };

    let status = response.status().as_u16();
    let request_id = request_id(&response, api_key);
    let response_body = response.text().unwrap_or_default();
    match status {
        200..=299 => VerifyResult::Verified,
        401 => VerifyResult::Rejected {
            code: error_code(&response_body, api_key),
            request_id,
        },
        400..=499 => {
            let code = warning_code(&response_body, &format!("verify_{status}"), api_key);
            VerifyResult::Warning(warning_for_status(status, code, base))
        }
        _ => VerifyResult::Indeterminate {
            detail: format!("received unexpected HTTP {status} from {base}"),
            request_id,
        },
    }
}

pub fn warning_for_status(status: u16, code: String, base: &BaseUrl) -> Warning {
    let message = match status {
        402 => {
            if base.is_official_cloud() {
                "the API key is valid but the account has a billing problem; check billing and usage at https://tokencamp.com".to_string()
            } else {
                format!(
                    "the API key is valid but the request failed with HTTP 402; check the workspace budget for {base}"
                )
            }
        }
        403 => "the API key is valid but lacks permission for this request".to_string(),
        429 => "the API key is valid but the account is rate limited; retry later".to_string(),
        _ => format!("the API key was accepted but the request failed with HTTP {status}"),
    };
    Warning { code, message }
}

fn transport_detail(base: &BaseUrl, error: &reqwest::Error) -> String {
    if error.is_timeout() {
        format!("the request to {base} timed out")
    } else {
        format!("could not reach {base}")
    }
}

fn request_id(response: &Response, api_key: &str) -> Option<String> {
    response
        .headers()
        .get("x-request-id")
        .or_else(|| response.headers().get("request-id"))
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value.len() <= 128 && !value.contains(api_key) && !value.chars().any(char::is_control)
        })
        .map(str::to_string)
}

fn error_code(body: &str, api_key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("error")
        .and_then(|error| error.get("code"))
        .or_else(|| value.get("code"))
        .and_then(serde_json::Value::as_str)
        .and_then(|code| safe_code(code, api_key))
}

fn warning_code(body: &str, fallback: &str, api_key: &str) -> String {
    error_code(body, api_key).unwrap_or_else(|| fallback.to_string())
}

fn safe_code(code: &str, api_key: &str) -> Option<String> {
    if (1..=64).contains(&code.len())
        && !code.contains(api_key)
        && code.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
    {
        Some(code.to_string())
    } else {
        None
    }
}
