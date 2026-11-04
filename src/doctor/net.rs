use std::time::Duration;

use reqwest::blocking::Response;
use reqwest::header::ACCEPT;
use serde_json::json;

use crate::credentials::BaseUrl;
use crate::credentials::probe::{self, ProbeResult, error_code, request_id, warning_for_status};
use crate::models;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(10);
const VERIFY_TIMEOUT: Duration = Duration::from_secs(30);

const MESSAGES_DEPLOYMENT_HINT: &str = "requires a `provider_type: anthropic` deployment at this base URL; ask the tokencamp operator to add one, then retry. Run `tcamp doctor --verify --harness pi` to verify the API key with chat completions";

pub(super) enum Outcome {
    Pass {
        message: String,
        request_id: Option<String>,
    },
    Warn {
        code: String,
        message: String,
        hint: Option<String>,
        request_id: Option<String>,
    },
    Fail {
        code: String,
        message: String,
        hint: String,
        request_id: Option<String>,
    },
}

#[derive(Debug)]
pub(super) enum ModelOutcome {
    Found,
    Missing {
        code: Option<String>,
        request_id: Option<String>,
    },
    Unverified {
        detail: String,
        code: Option<String>,
        request_id: Option<String>,
    },
}

impl ModelOutcome {
    pub(super) fn detail(&self) -> Option<String> {
        match self {
            Self::Found => None,
            Self::Missing { .. } => Some("not found".to_string()),
            Self::Unverified { detail, .. } => Some(detail.clone()),
        }
    }

    pub(super) fn code(&self) -> Option<&str> {
        match self {
            Self::Found => None,
            Self::Missing { code, .. } | Self::Unverified { code, .. } => code.as_deref(),
        }
    }

    pub(super) fn request_id(&self) -> Option<&str> {
        match self {
            Self::Found => None,
            Self::Missing { request_id, .. } | Self::Unverified { request_id, .. } => {
                request_id.as_deref()
            }
        }
    }
}

pub(super) fn reachable(base: &BaseUrl, api_key: &str) -> Outcome {
    let client = match crate::http::blocking_client(NETWORK_TIMEOUT) {
        Ok(client) => client,
        Err(message) => return client_failure(message),
    };
    let url = base.join("/v1/models");
    let response = match client
        .get(&url)
        .bearer_auth(api_key)
        .header(ACCEPT, "application/json")
        .send()
    {
        Ok(response) => response,
        Err(error) => return transport_failure(&url, &error),
    };

    let status = response.status().as_u16();
    let request_id = request_id(&response, api_key);
    if (200..=299).contains(&status) {
        Outcome::Pass {
            message: format!("GET {url} returned HTTP {status}"),
            request_id,
        }
    } else {
        let body = response.text().unwrap_or_default();
        let code = error_code(&body, api_key).unwrap_or_else(|| "network".to_string());
        Outcome::Fail {
            code,
            message: format!("GET {url} returned HTTP {status}"),
            hint: "check the base URL and network connectivity, then run `tcamp doctor --network` again"
                .to_string(),
            request_id,
        }
    }
}

pub(super) fn auth(base: &BaseUrl, api_key: &str) -> Outcome {
    match probe::probe(base, api_key) {
        ProbeResult::Verified {
            warning: None,
            request_id,
        } => Outcome::Pass {
            message: format!("the API key was accepted by {base}"),
            request_id,
        },
        ProbeResult::Verified {
            warning: Some(warning),
            request_id,
        } => Outcome::Warn {
            code: warning.code,
            message: warning.message,
            hint: Some(
                "the API key is valid; resolve the account, permission, or rate-limit condition, then retry"
                    .to_string(),
            ),
            request_id,
        },
        ProbeResult::Rejected { code, request_id } => Outcome::Fail {
            code: code.unwrap_or_else(|| "authentication".to_string()),
            message: format!("the API key was rejected by {base}"),
            hint: auth_hint(base),
            request_id,
        },
        ProbeResult::Indeterminate { detail, request_id } => Outcome::Fail {
            code: "network".to_string(),
            message: detail,
            hint: "check connectivity, then run `tcamp doctor --network` again".to_string(),
            request_id,
        },
    }
}

pub(super) fn verify_chat(base: &BaseUrl, api_key: &str) -> Outcome {
    send_verification(base, api_key, VerificationRequest::Chat)
}

pub(super) fn verify_messages(base: &BaseUrl, api_key: &str) -> Outcome {
    send_verification(base, api_key, VerificationRequest::Messages)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VerificationRequest {
    Chat,
    Messages,
}

impl VerificationRequest {
    fn path(self) -> &'static str {
        match self {
            Self::Chat => "/v1/chat/completions",
            Self::Messages => "/v1/messages",
        }
    }

    fn success_message(url: &str) -> String {
        format!("POST {url} succeeded")
    }
}

fn send_verification(base: &BaseUrl, api_key: &str, request: VerificationRequest) -> Outcome {
    let client = match crate::http::blocking_client(VERIFY_TIMEOUT) {
        Ok(client) => client,
        Err(message) => return client_failure(message),
    };
    let url = base.join(request.path());
    let body = json!({
        "model": models::DEFAULT_MAIN_MODEL,
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "ping"}],
        "stream": false,
    });

    let builder = client
        .post(&url)
        .bearer_auth(api_key)
        .header(ACCEPT, "application/json");
    let builder = match request {
        VerificationRequest::Chat => builder,
        VerificationRequest::Messages => builder.header("anthropic-version", "2023-06-01"),
    };
    let response = match builder.json(&body).send() {
        Ok(response) => response,
        Err(error) => return transport_failure(&url, &error),
    };

    classify_verification(base, api_key, &url, request, response)
}

fn classify_verification(
    base: &BaseUrl,
    api_key: &str,
    url: &str,
    request: VerificationRequest,
    response: Response,
) -> Outcome {
    let status = response.status().as_u16();
    let request_id = request_id(&response, api_key);
    let body = response.text().unwrap_or_default();
    let code = error_code(&body, api_key);

    match status {
        200..=299 => Outcome::Pass {
            message: VerificationRequest::success_message(url),
            request_id,
        },
        401 => Outcome::Fail {
            code: code.unwrap_or_else(|| "authentication".to_string()),
            message: format!("the API key was rejected by {base}"),
            hint: auth_hint(base),
            request_id,
        },
        403 | 429 => {
            let warning = warning_for_status(
                status,
                code.unwrap_or_else(|| format!("verify_{status}")),
                base,
            );
            Outcome::Warn {
                code: warning.code,
                message: warning.message,
                hint: Some(
                    "the API key is valid; resolve the permission or rate-limit condition, then retry"
                        .to_string(),
                ),
                request_id,
            }
        }
        402 => {
            let warning = warning_for_status(
                402,
                code.unwrap_or_else(|| "payment_required".to_string()),
                base,
            );
            let hint = if base.is_official_cloud() {
                "check billing and usage at https://tokencamp.com, then retry".to_string()
            } else {
                format!("check the workspace budget for {base}, then retry")
            };
            Outcome::Fail {
                code: warning.code,
                message: warning.message,
                hint,
                request_id,
            }
        }
        404 if request == VerificationRequest::Messages
            && code.as_deref() == Some("model_not_found") =>
        {
            Outcome::Fail {
                code: "model_not_found".to_string(),
                message: format!(
                    "POST {url} returned HTTP 404 (model_not_found) for {}",
                    models::DEFAULT_MAIN_MODEL
                ),
                hint: MESSAGES_DEPLOYMENT_HINT.to_string(),
                request_id,
            }
        }
        _ => Outcome::Fail {
            code: code.unwrap_or_else(|| format!("verify_{status}")),
            message: format!("POST {url} returned HTTP {status}"),
            hint: "check the model mapping, the account, and the base URL, then retry".to_string(),
            request_id,
        },
    }
}

pub(super) fn check_models(
    base: &BaseUrl,
    api_key: &str,
    ids: &[String],
) -> Vec<(String, ModelOutcome)> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = ids
            .iter()
            .map(|id| {
                let handle = scope.spawn(move || check_model(base, api_key, id));
                (id, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(id, handle)| match handle.join() {
                Ok(outcome) => (id.clone(), outcome),
                Err(_) => (
                    id.clone(),
                    ModelOutcome::Unverified {
                        detail: "the model check worker failed".to_string(),
                        code: None,
                        request_id: None,
                    },
                ),
            })
            .collect()
    })
}

fn check_model(base: &BaseUrl, api_key: &str, id: &str) -> ModelOutcome {
    let client = match crate::http::blocking_client(NETWORK_TIMEOUT) {
        Ok(client) => client,
        Err(message) => return unverified(message, None, None),
    };
    let url = base.join(&format!("/v1/models/{id}"));
    let response = match client
        .get(&url)
        .bearer_auth(api_key)
        .header(ACCEPT, "application/json")
        .send()
    {
        Ok(response) => response,
        Err(error) if error.is_timeout() => {
            return unverified(format!("the request to {url} timed out"), None, None);
        }
        Err(_) => return unverified(format!("could not reach {url}"), None, None),
    };

    let status = response.status().as_u16();
    let request_id = request_id(&response, api_key);
    let body = response.text().unwrap_or_default();
    let code = error_code(&body, api_key);
    match status {
        200 => ModelOutcome::Found,
        404 => ModelOutcome::Missing { code, request_id },
        status => unverified(
            format!("received HTTP {status} from {url}"),
            code,
            request_id,
        ),
    }
}

fn unverified(detail: String, code: Option<String>, request_id: Option<String>) -> ModelOutcome {
    ModelOutcome::Unverified {
        detail,
        code,
        request_id,
    }
}

fn client_failure(message: String) -> Outcome {
    Outcome::Fail {
        code: "network".to_string(),
        message,
        hint: "check the system network stack and try again".to_string(),
        request_id: None,
    }
}

fn transport_failure(url: &str, error: &reqwest::Error) -> Outcome {
    let message = if error.is_timeout() {
        format!("the request to {url} timed out")
    } else {
        format!("could not reach {url}")
    };
    Outcome::Fail {
        code: "network".to_string(),
        message,
        hint: "check connectivity and the base URL, then retry".to_string(),
        request_id: None,
    }
}

fn auth_hint(base: &BaseUrl) -> String {
    match base.portal_keys_url() {
        Some(portal) => format!("create or rotate an API key at {portal}"),
        None => format!("check the API key for {base}"),
    }
}
