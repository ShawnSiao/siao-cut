use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{Value, json};

use super::{GenerationInput, ProviderFailure, ProviderOutput, transport};
use crate::ai_services::{
    error::AiError,
    network::NetworkStore,
    types::{AiModelInfo, ResolvedAiService},
};

pub fn list_models(
    service: &ResolvedAiService,
    network: &NetworkStore,
) -> Result<Vec<AiModelInfo>, ProviderFailure> {
    let response = transport::client(network, Duration::from_secs(20))?
        .get(transport::endpoint(&service.base_url, "/models"))
        .header(AUTHORIZATION, format!("Bearer {}", service.api_key))
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
        "model": input.model_id,
        "instructions": input.system,
        "input": [{"role":"user","content":[{"type":"input_text","text":input.prompt}]}],
        "store": false,
        "text": {"format":{"type":"json_schema","name":input.schema_name,"strict":true,"schema":input.schema}}
    });
    let response = transport::client(network, Duration::from_secs(120))?
        .post(transport::endpoint(&service.base_url, "/responses"))
        .header(AUTHORIZATION, format!("Bearer {}", service.api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .map_err(transport::send_error)?;
    let (payload, request_id) =
        transport::json_value(transport::checked(response, AiError::ModelNotFound)?)?;
    let output_text = payload
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .find_map(|content| {
            (content.get("type").and_then(Value::as_str) == Some("output_text"))
                .then(|| content.get("text").and_then(Value::as_str))
                .flatten()
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
    })
}
