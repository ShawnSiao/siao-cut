use std::time::Duration;

use reqwest::header::CONTENT_TYPE;
use serde_json::{Value, json};

use super::{GenerationInput, ProviderFailure, ProviderOutput, transport};
use crate::ai_services::{
    error::AiError,
    network::NetworkStore,
    types::{AiModelInfo, ResolvedAiService},
};

fn request(
    service: &ResolvedAiService,
    network: &NetworkStore,
    timeout: Duration,
    method: reqwest::Method,
    path: &str,
) -> Result<reqwest::blocking::RequestBuilder, ProviderFailure> {
    Ok(transport::client(network, timeout)?
        .request(method, transport::endpoint(&service.base_url, path))
        .header("x-api-key", &service.api_key)
        .header("anthropic-version", "2023-06-01"))
}

pub fn list_models(
    service: &ResolvedAiService,
    network: &NetworkStore,
) -> Result<Vec<AiModelInfo>, ProviderFailure> {
    let response = request(
        service,
        network,
        Duration::from_secs(20),
        reqwest::Method::GET,
        "/v1/models",
    )?
    .send()
    .map_err(transport::send_error)?;
    let (payload, _) =
        transport::json_value(transport::checked(response, AiError::ProviderUnavailable)?)?;
    Ok(payload
        .get("data")
        .and_then(Value::as_array)
        .ok_or(AiError::InvalidResponse)?
        .iter()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .map(|id| AiModelInfo {
            id: id.to_owned(),
            display_name: id.to_owned(),
        })
        .collect())
}

pub fn generate(
    service: &ResolvedAiService,
    network: &NetworkStore,
    input: &GenerationInput,
) -> Result<ProviderOutput, ProviderFailure> {
    let body = json!({
        "model":input.model_id,
        "max_tokens":4096,
        "system":format!("{}\n只返回符合此 JSON Schema 的 JSON：{}", input.system, input.schema),
        "messages":[{"role":"user","content":[{"type":"text","text":input.prompt}]}]
    });
    let response = request(
        service,
        network,
        Duration::from_secs(120),
        reqwest::Method::POST,
        "/v1/messages",
    )?
    .header(CONTENT_TYPE, "application/json")
    .json(&body)
    .send()
    .map_err(transport::send_error)?;
    let (payload, request_id) =
        transport::json_value(transport::checked(response, AiError::ModelNotFound)?)?;
    let output_text = payload
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                (item.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| item.get("text").and_then(Value::as_str))
                    .flatten()
            })
        })
        .ok_or(AiError::InvalidResponse)?
        .to_owned();
    Ok(ProviderOutput {
        output_text,
        provider_request_id: payload
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or(request_id),
        usage: payload.get("usage").cloned(),
        retry_count: 0,
    })
}
