use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

use crate::ai_services::{
    config::AiServiceStore,
    network::NetworkStore,
    providers::{self, GenerationInput},
    types::ResolvedAiService,
};

use super::execution::ExecutionTarget;

#[derive(Debug)]
pub struct ApiBatchResult {
    pub value: Value,
    pub provider_request_id: Option<String>,
    pub usage: Option<Value>,
    pub retry_count: u32,
}

pub fn validate_target(target: &ExecutionTarget) -> Result<Option<String>> {
    let home = crate::db::home_dir();
    validate_target_with(
        target,
        &AiServiceStore::for_home(&home),
        &NetworkStore::for_home(&home),
    )
}

fn validate_target_with(
    target: &ExecutionTarget,
    services: &AiServiceStore,
    network: &NetworkStore,
) -> Result<Option<String>> {
    let ExecutionTarget::Api {
        service_config_id,
        service_revision,
        network_revision,
        ..
    } = target
    else {
        return Ok(None);
    };
    let service = resolve_service(
        services,
        network,
        service_config_id,
        *service_revision,
        *network_revision,
    )?;
    if services
        .stored_credential(service_config_id)
        .map_err(ai_error)?
        .is_none()
    {
        bail!("credential_missing: 尚未保存 API Key")
    }
    Ok(Some(service.provider_id.as_str().to_owned()))
}

pub fn execute(
    target: &ExecutionTarget,
    payload: &Value,
    schema: &Value,
) -> Result<ApiBatchResult> {
    let home = crate::db::home_dir();
    execute_with(
        target,
        payload,
        schema,
        &AiServiceStore::for_home(&home),
        &NetworkStore::for_home(&home),
    )
}

fn execute_with(
    target: &ExecutionTarget,
    payload: &Value,
    schema: &Value,
    services: &AiServiceStore,
    network: &NetworkStore,
) -> Result<ApiBatchResult> {
    let ExecutionTarget::Api {
        service_config_id,
        service_revision,
        network_revision,
        model_id,
    } = target
    else {
        bail!("invalid_request: API 执行器收到非 API 目标")
    };
    let service = resolve_service(
        services,
        network,
        service_config_id,
        *service_revision,
        *network_revision,
    )?;
    let api_key = services
        .stored_credential(service_config_id)
        .map_err(ai_error)?
        .ok_or_else(|| anyhow!("credential_missing: 尚未保存 API Key"))?;
    let resolved = ResolvedAiService {
        service_config_id: Some(service_config_id.clone()),
        provider_id: service.provider_id,
        protocol: service.protocol,
        base_url: service.base_url,
        model_id: Some(model_id.clone()),
        api_key,
    };
    let remote = remote_payload(payload)?;
    let kind = remote
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("polish");
    let output = providers::generate(&resolved, network, &GenerationInput {
        model_id: model_id.clone(),
        system: "只处理已明确授权的字幕文本和结构约束。不要推断、请求或返回本机文件、媒体、数据库或凭据。所有结果仅供人工审核。".to_owned(),
        prompt: serde_json::to_string(&json!({
            "protocol":"siaocut-agent-v1",
            "task":remote,
            "completionRule":"processedSegmentIds 必须恰好包含全部字幕段 ID；建议不得超出任务范围。"
        }))?,
        schema_name: format!("siaocut_{kind}_result"),
        schema: schema.clone(),
    }).map_err(|failure| {
        let request = failure.provider_request_id.as_deref().map(|id| format!("；厂商请求 ID：{id}")).unwrap_or_default();
        anyhow!("{}: {}{}", failure.error.code(), failure.error, request)
    })?;
    let value = serde_json::from_str(&output.output_text)
        .map_err(|_| anyhow!("invalid_response: AI 服务返回的结果不是有效 JSON"))?;
    Ok(ApiBatchResult {
        value,
        provider_request_id: output.provider_request_id,
        usage: output.usage,
        retry_count: output.retry_count,
    })
}

fn resolve_service(
    services: &AiServiceStore,
    network: &NetworkStore,
    service_config_id: &str,
    service_revision: u64,
    network_revision: u64,
) -> Result<crate::ai_services::types::AiServiceConfig> {
    let service = services
        .configured_service(service_config_id)
        .map_err(ai_error)?;
    if service.revision != service_revision
        || network.snapshot().map_err(ai_error)?.revision != network_revision
    {
        bail!("service_revision_changed: AI 服务配置已经改变，请重新确认")
    }
    Ok(service)
}

fn ai_error(error: crate::ai_services::error::AiError) -> anyhow::Error {
    anyhow!("{}: {}", error.code(), error)
}

fn remote_payload(payload: &Value) -> Result<Value> {
    let mut remote = payload
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("agent_output_invalid: Agent 任务载荷无效"))?;
    for forbidden in ["taskId", "projectId", "leaseId", "attemptCount"] {
        remote.remove(forbidden);
    }
    let serialized = serde_json::to_string(&remote)?;
    for forbidden in ["mediaPath", "databasePath", "apiKey", "credential"] {
        if serialized
            .to_ascii_lowercase()
            .contains(&forbidden.to_ascii_lowercase())
        {
            bail!("agent_output_invalid: AI 请求包含不允许的本机字段")
        }
    }
    Ok(Value::Object(remote))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    use crate::ai_services::{
        credentials::tests_support::MemoryCredentialStore,
        providers::test_support::serve_once,
        types::{AiProtocol, AiProviderId, SaveAiServiceInput},
    };

    #[test]
    fn remote_projection_removes_local_identifiers() {
        let value = remote_payload(&json!({"taskId":"t","projectId":"p","leaseId":"l","attemptCount":2,"kind":"polish","segments":[{"id":"s","text":"safe"}]})).unwrap();
        assert!(value.get("taskId").is_none());
        assert!(value.get("projectId").is_none());
        assert_eq!(value["segments"][0]["text"], "safe");
    }

    #[test]
    fn api_executor_uses_memory_credentials_and_sends_only_projected_text() {
        let temp = tempdir().unwrap();
        let server = serve_once(
            200,
            r#"{"id":"chat_1","choices":[{"message":{"content":"{\"baseVersionId\":\"v1\",\"processedSegmentIds\":[\"s1\"],\"patches\":[]}"}}],"usage":{"total_tokens":9}}"#,
        );
        let credentials = Arc::new(MemoryCredentialStore::default());
        let services = AiServiceStore::new(temp.path().join("ai.json"), credentials);
        let settings = services
            .save(SaveAiServiceInput {
                expected_revision: 0,
                id: None,
                provider_id: AiProviderId::Custom,
                display_name: "Mock compatible".into(),
                protocol: AiProtocol::OpenaiChatCompletions,
                base_url: Some(server.url.clone()),
                model_id: Some("test-model".into()),
                api_key: Some("memory-only-secret".into()),
            })
            .unwrap();
        let service = settings.services.first().unwrap();
        let target = ExecutionTarget::Api {
            service_config_id: service.id.clone(),
            service_revision: service.revision,
            network_revision: 0,
            model_id: "test-model".into(),
        };
        let result = execute_with(
            &target,
            &json!({
                "taskId":"task-local","projectId":"project-local","leaseId":"lease-local",
                "kind":"polish","baseVersionId":"v1",
                "segments":[{"id":"s1","start":0,"end":1,"text":"safe subtitle"}]
            }),
            &json!({"type":"object"}),
            &services,
            &NetworkStore::for_home(temp.path()),
        )
        .unwrap();
        assert_eq!(result.value["baseVersionId"], "v1");
        let request = server.finish();
        let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
        assert!(body.contains("safe subtitle"));
        for forbidden in [
            "task-local",
            "project-local",
            "lease-local",
            "memory-only-secret",
            "mediaPath",
            "databasePath",
        ] {
            assert!(!body.contains(forbidden), "request body leaked {forbidden}");
        }
    }
}
