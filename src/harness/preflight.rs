use std::time::Duration;

use reqwest::header::ACCEPT;

use crate::warnings::Warning;

const MODEL_CHECK_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub struct ModelCheck {
    pub missing: Vec<String>,
    pub warnings: Vec<Warning>,
}

enum Outcome {
    Found,
    Missing,
    Unverified(String),
}

pub fn check_models(base_url: &str, api_key: &str, ids: &[String]) -> ModelCheck {
    let mut check = ModelCheck::default();
    if ids.is_empty() {
        return check;
    }

    let outcomes: Vec<(String, Outcome)> = std::thread::scope(|scope| {
        let handles: Vec<_> = ids
            .iter()
            .map(|id| {
                let handle = scope.spawn(move || check_model(base_url, api_key, id));
                (id, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(id, handle)| match handle.join() {
                Ok(outcome) => (id.clone(), outcome),
                Err(_) => (
                    id.clone(),
                    Outcome::Unverified("the model check worker failed".to_string()),
                ),
            })
            .collect()
    });

    for (id, outcome) in outcomes {
        match outcome {
            Outcome::Found => {}
            Outcome::Missing => check.missing.push(id),
            Outcome::Unverified(detail) => check.warnings.push(Warning::new(
                "model_check_failed",
                format!(
                    "could not verify model {id} at {base_url}: {detail}; continuing without the check"
                ),
            )),
        }
    }

    check
}

fn check_model(base_url: &str, api_key: &str, id: &str) -> Outcome {
    let client = match crate::http::blocking_client(MODEL_CHECK_TIMEOUT) {
        Ok(client) => client,
        Err(_) => return Outcome::Unverified("could not initialize the HTTP client".to_string()),
    };

    let url = format!("{base_url}/v1/models/{id}");
    let response = match client
        .get(&url)
        .bearer_auth(api_key)
        .header(ACCEPT, "application/json")
        .send()
    {
        Ok(response) => response,
        Err(error) if error.is_timeout() => {
            return Outcome::Unverified(format!("the request to {url} timed out"));
        }
        Err(_) => return Outcome::Unverified(format!("could not reach {url}")),
    };

    match response.status().as_u16() {
        200..=299 => Outcome::Found,
        404 => Outcome::Missing,
        status => Outcome::Unverified(format!("received HTTP {status} from {url}")),
    }
}
