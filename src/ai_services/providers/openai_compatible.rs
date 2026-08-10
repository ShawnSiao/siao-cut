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
        "model":input.model_id,
        "messages":[
            {"role":"system","content":format!("{}\n只返回符合 JSON Schema `{}` 的 JSON：{}", input.system, input.schema_name, input.schema)},
            {"role":"user","content":input.prompt}
        ],
        "response_format":{"type":"json_object"},
        "stream":false
    });
    let response = transport::client(network, Duration::from_secs(120))?
        .post(transport::endpoint(&service.base_url, "/chat/completions"))
        .header(AUTHORIZATION, format!("Bearer {}", service.api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .map_err(transport::send_error)?;
    let (payload, request_id) =
        transport::json_value(transport::checked(response, AiError::ModelNotFound)?)?;
    let output_text = payload
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
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
