use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    catalog,
    config::AiServiceStore,
    endpoint::normalize_service_endpoint,
    error::{AiCommandError, AiError},
    network::NetworkStore,
    providers::{self, GenerationInput, ProviderFailure},
    types::{
        AiEnvironmentSettings, AiModelList, AiServiceProbeInput, AiServiceTestResult,
        ConnectionState, ResolvedAiService, SaveAiServiceInput, ServiceMutationInput,
        SetDefaultAiServiceInput, SetNetworkSettingsInput,
    },
};

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AiRequest {
    Snapshot,
    Save { input: SaveAiServiceInput },
    Delete { input: ServiceMutationInput },
    DeleteCredential { input: ServiceMutationInput },
    SetDefault { input: SetDefaultAiServiceInput },
    SetNetwork { input: SetNetworkSettingsInput },
    ListModels { input: AiServiceProbeInput },
    TestService { input: AiServiceProbeInput },
    Purge { confirm: bool },
}

pub fn execute_request(home: &Path, payload: &[u8]) -> Result<Value, AiCommandError> {
    if payload.len() > 16 * 1024 {
        return Err(AiError::PayloadTooLarge.into());
    }
    let request: AiRequest = serde_json::from_slice(payload)
        .map_err(|_| AiError::Validation("AI 服务请求 JSON 无效".to_owned()))?;
    let services = AiServiceStore::for_home(home);
    let network = NetworkStore::for_home(home);
    match request {
        AiRequest::Snapshot => Ok(json!({"aiEnvironment": snapshot(&services, &network)?})),
        AiRequest::Save { input } => Ok(json!({"aiServices": services.save(input)?})),
        AiRequest::Delete { input } => Ok(json!({"aiServices": services.delete(input)?})),
        AiRequest::DeleteCredential { input } => {
            Ok(json!({"aiServices": services.delete_credential(input)?}))
        }
        AiRequest::SetDefault { input } => Ok(json!({"aiServices": services.set_default(input)?})),
        AiRequest::SetNetwork { input } => Ok(json!({"network": network.set(input)?})),
        AiRequest::ListModels { input } => {
            let service = resolve_probe(&services, &input)?;
            let models = providers::list_models(&service, &network).map_err(command_error)?;
            Ok(json!({"models": AiModelList { models, manual_entry_allowed: true }}))
        }
        AiRequest::TestService { input } => {
            let service = resolve_probe(&services, &input)?;
            match test_service(&service, &network) {
                Ok(result) => {
                    if let Some(id) = service.service_config_id.as_deref() {
                        services.mark_test_result(id, Ok(()))?;
                    }
                    Ok(json!({"testResult": result}))
                }
                Err(failure) => {
                    if let Some(id) = service.service_config_id.as_deref() {
                        let _ = services.mark_test_result(id, Err(&failure.error));
                    }
                    Err(command_error(failure))
                }
            }
        }
        AiRequest::Purge { confirm } => {
            if !confirm {
                return Err(AiError::Validation("删除 AI 服务配置需要显式确认".to_owned()).into());
            }
            services.purge()?;
            network.remove_file()?;
            Ok(json!({"purged": true}))
        }
    }
}

fn snapshot(
    services: &AiServiceStore,
    network: &NetworkStore,
) -> Result<AiEnvironmentSettings, AiError> {
    Ok(AiEnvironmentSettings {
        ai_services: services.snapshot()?,
        network: network.snapshot()?,
    })
}

fn resolve_probe(
    store: &AiServiceStore,
    input: &AiServiceProbeInput,
) -> Result<ResolvedAiService, AiError> {
    if let Some(id) = input.service_config_id.as_deref() {
        let configured = store.configured_service(id)?;
        let api_key = input
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or(store.stored_credential(id)?)
            .ok_or(AiError::CredentialMissing)?;
        return Ok(ResolvedAiService {
            service_config_id: Some(id.to_owned()),
            provider_id: configured.provider_id,
            protocol: configured.protocol,
            base_url: configured.base_url,
            model_id: normalized_model(input.model_id.as_deref()).or(configured.model_id),
            api_key,
        });
    }
    let provider = catalog::provider(input.provider_id)?;
    if input.protocol != provider.protocol {
        return Err(AiError::Validation("服务协议与厂商不匹配".to_owned()));
    }
    let base_url = if input.provider_id == super::types::AiProviderId::Custom {
        input
            .base_url
            .as_deref()
            .ok_or_else(|| AiError::Validation("请输入服务地址".to_owned()))?
    } else {
        provider
            .official_base_url
            .as_deref()
            .ok_or(AiError::ConfigurationRead)?
    };
    let api_key = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AiError::CredentialMissing)?
        .to_owned();
    Ok(ResolvedAiService {
        service_config_id: None,
        provider_id: input.provider_id,
        protocol: input.protocol,
        base_url: normalize_service_endpoint(base_url)?,
        model_id: normalized_model(input.model_id.as_deref()),
        api_key,
    })
}

fn test_service(
    service: &ResolvedAiService,
    network: &NetworkStore,
) -> Result<AiServiceTestResult, ProviderFailure> {
    match providers::list_models(service, network) {
        Ok(models) => {
            if service.model_id.as_ref().is_some_and(|selected| {
                !models.is_empty() && !models.iter().any(|model| &model.id == selected)
            }) {
                return Err(AiError::ModelNotFound.into());
            }
            Ok(AiServiceTestResult {
                state: ConnectionState::Ready,
                models,
                selected_model_id: service.model_id.clone(),
                minimal_request_used: false,
                may_incur_usage: false,
                provider_request_id: None,
            })
        }
        Err(failure)
            if matches!(
                failure.error,
                AiError::ProviderUnavailable | AiError::InvalidResponse
            ) && service.model_id.is_some() =>
        {
            let output = providers::generate(
                service,
                network,
                &GenerationInput {
                    model_id: service.model_id.clone().unwrap_or_default(),
                    system: "这是连接测试。不要使用任何外部材料。".to_owned(),
                    prompt: "返回 JSON：{\"ok\":true}".to_owned(),
                    schema_name: "connection_test".to_owned(),
                    schema: json!({"type":"object","properties":{"ok":{"type":"boolean","const":true}},"required":["ok"],"additionalProperties":false}),
                },
            )?;
            let valid = serde_json::from_str::<Value>(&output.output_text)
                .ok()
                .and_then(|value| value.get("ok").and_then(Value::as_bool))
                == Some(true);
            if !valid {
                return Err(AiError::InvalidResponse.into());
            }
            let _usage = output.usage;
            Ok(AiServiceTestResult {
                state: ConnectionState::Ready,
                models: Vec::new(),
                selected_model_id: service.model_id.clone(),
                minimal_request_used: true,
                may_incur_usage: true,
                provider_request_id: output.provider_request_id,
            })
        }
        Err(failure) => Err(failure),
    }
}

fn normalized_model(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn command_error(failure: ProviderFailure) -> AiCommandError {
    AiCommandError::from(failure.error).with_request_id(failure.provider_request_id)
}
