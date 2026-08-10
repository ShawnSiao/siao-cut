use std::time::Duration;

use reqwest::{
    StatusCode,
    blocking::{Client, Response},
};
use serde_json::Value;

use super::ProviderFailure;
use crate::ai_services::{
    error::AiError,
    network::{self, NetworkStore},
};

pub fn client(network_store: &NetworkStore, timeout: Duration) -> Result<Client, ProviderFailure> {
    let settings = network_store.snapshot()?;
    network::build_client(
        Client::builder()
            .user_agent(format!("SiaoCut/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(timeout.min(Duration::from_secs(30)))
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::limited(3)),
        &settings,
    )
    .map_err(ProviderFailure::from)
}

pub fn endpoint(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

pub fn checked(response: Response, not_found: AiError) -> Result<Response, ProviderFailure> {
    if response.status().is_success() {
        return Ok(response);
    }
    let request_id = response_request_id(&response);
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs);
    let error = match response.status() {
        StatusCode::UNAUTHORIZED => AiError::Unauthorized,
        StatusCode::FORBIDDEN => AiError::Forbidden,
        StatusCode::NOT_FOUND => not_found,
        StatusCode::TOO_MANY_REQUESTS => AiError::RateLimited,
        status if status.is_server_error() => AiError::ProviderUnavailable,
        _ => AiError::InvalidResponse,
    };
    Err(ProviderFailure {
        error,
        provider_request_id: request_id,
        retry_after,
    })
}

pub fn send_error(error: reqwest::Error) -> ProviderFailure {
    ProviderFailure::from(if error.is_timeout() {
        AiError::Timeout
    } else {
        AiError::ProviderUnavailable
    })
}

pub fn response_request_id(response: &Response) -> Option<String> {
    ["x-request-id", "request-id", "x-goog-request-id"]
        .iter()
        .find_map(|name| response.headers().get(*name))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

pub fn json_value(response: Response) -> Result<(Value, Option<String>), ProviderFailure> {
    let request_id = response_request_id(&response);
    let payload = response.json::<Value>().map_err(|_| ProviderFailure {
        error: AiError::InvalidResponse,
        provider_request_id: request_id.clone(),
        retry_after: None,
    })?;
    Ok((payload, request_id))
}
