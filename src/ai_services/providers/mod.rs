mod anthropic;
mod gemini;
mod openai;
mod openai_compatible;
mod transport;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod test_support;

use std::{thread, time::Duration};

use serde_json::Value;

use super::{
    network::NetworkStore,
    types::{AiModelInfo, AiProtocol, ResolvedAiService},
};
use crate::ai_services::error::AiError;

pub struct GenerationInput {
    pub model_id: String,
    pub system: String,
    pub prompt: String,
    pub schema_name: String,
    pub schema: Value,
}

#[derive(Clone, Debug)]
pub struct ProviderOutput {
    pub output_text: String,
    pub provider_request_id: Option<String>,
    pub usage: Option<Value>,
}

#[derive(Debug)]
pub struct ProviderFailure {
    pub error: AiError,
    pub provider_request_id: Option<String>,
    pub retry_after: Option<Duration>,
}

impl From<AiError> for ProviderFailure {
    fn from(error: AiError) -> Self {
        Self {
            error,
            provider_request_id: None,
            retry_after: None,
        }
    }
}

pub fn list_models(
    service: &ResolvedAiService,
    network: &NetworkStore,
) -> Result<Vec<AiModelInfo>, ProviderFailure> {
    let _provider_id = service.provider_id;
    match service.protocol {
        AiProtocol::OpenaiResponses => openai::list_models(service, network),
        AiProtocol::AnthropicMessages => anthropic::list_models(service, network),
        AiProtocol::GeminiGenerateContent => gemini::list_models(service, network),
        AiProtocol::OpenaiChatCompletions => openai_compatible::list_models(service, network),
    }
}

pub fn generate(
    service: &ResolvedAiService,
    network: &NetworkStore,
    input: &GenerationInput,
) -> Result<ProviderOutput, ProviderFailure> {
    for attempt in 0..=2 {
        match generate_once(service, network, input) {
            Ok(output) => return Ok(output),
            Err(failure) if failure.error.retryable() && attempt < 2 => {
                let fallback = Duration::from_secs(1_u64 << attempt);
                thread::sleep(
                    failure
                        .retry_after
                        .unwrap_or(fallback)
                        .min(Duration::from_secs(30)),
                );
            }
            Err(failure) => return Err(failure),
        }
    }
    unreachable!("retry loop always returns")
}

fn generate_once(
    service: &ResolvedAiService,
    network: &NetworkStore,
    input: &GenerationInput,
) -> Result<ProviderOutput, ProviderFailure> {
    match service.protocol {
        AiProtocol::OpenaiResponses => openai::generate(service, network, input),
        AiProtocol::AnthropicMessages => anthropic::generate(service, network, input),
        AiProtocol::GeminiGenerateContent => gemini::generate(service, network, input),
        AiProtocol::OpenaiChatCompletions => openai_compatible::generate(service, network, input),
    }
}
