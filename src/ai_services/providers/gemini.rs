use std::time::Duration;

use reqwest::header::CONTENT_TYPE;
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
        .header("x-goog-api-key", &service.api_key)
        .send()
        .map_err(transport::send_error)?;
    let (payload, _) =
        transport::json_value(transport::checked(response, AiError::ProviderUnavailable)?)?;
    Ok(payload
        .get("models")
        .and_then(Value::as_array)
        .ok_or(AiError::InvalidResponse)?
        .iter()
        .filter(|model| {
            model
                .get("supportedGenerationMethods")
                .and_then(Value::as_array)
                .is_none_or(|methods| methods.iter().any(|method| method == "generateContent"))
        })
        .filter_map(|model| {
            let id = model
                .get("baseModelId")
                .or_else(|| model.get("name"))?
                .as_str()?
                .trim_start_matches("models/");
            Some(AiModelInfo {
                id: id.to_owned(),
                display_name: model
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_owned(),
            })
        })
        .collect())
}

pub fn generate(
    service: &ResolvedAiService,
    network: &NetworkStore,
    input: &GenerationInput,
) -> Result<ProviderOutput, ProviderFailure> {
    let body = json!({
        "contents":[{"role":"user","parts":[{"text":format!("{}\n\n{}", input.system, input.prompt)}]}],
        "generationConfig":{"responseMimeType":"application/json","responseJsonSchema":input.schema}
    });
    let path = format!("/models/{}:generateContent", input.model_id);
    let response = transport::client(network, Duration::from_secs(120))?
        .post(transport::endpoint(&service.base_url, &path))
        .header("x-goog-api-key", &service.api_key)
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .map_err(transport::send_error)?;
    let (payload, request_id) =
        transport::json_value(transport::checked(response, AiError::ModelNotFound)?)?;
    let output_text = payload
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(Value::as_str)
        .ok_or(AiError::InvalidResponse)?
        .to_owned();
    Ok(ProviderOutput {
        output_text,
        provider_request_id: request_id,
        usage: payload.get("usageMetadata").cloned(),
        retry_count: 0,
    })
}
