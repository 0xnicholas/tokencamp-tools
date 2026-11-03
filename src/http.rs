use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::redirect::Policy;

pub fn blocking_client(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .timeout(timeout)
        .redirect(Policy::none())
        .user_agent(concat!("tcamp/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "could not initialize the HTTP client".to_string())
}
