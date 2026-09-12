use crate::agent::execution::ExecutionTarget;
use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

pub(super) struct ApprovalTarget {
    pub revision: String,
    pub receiver: String,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub verified: bool,
}

pub(super) fn configuration(target: &ExecutionTarget) -> Result<ApprovalTarget> {
    let home = crate::db::home_dir();
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(std::path::PathBuf::from)
                .unwrap_or_default()
                .join(".codex")
        });
    configuration_at(target, &home, &codex_home)
}

fn configuration_at(
    target: &ExecutionTarget,
    home: &std::path::Path,
    codex_home: &std::path::Path,
) -> Result<ApprovalTarget> {
    target.validate()?;
    let network = crate::ai_services::network::NetworkStore::for_home(home).snapshot()?;
    let network_fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&network)?));
    match target {
        ExecutionTarget::Codex => {
            // Only fingerprint configuration; never retain or disclose its contents.
            let bytes = match std::fs::read(codex_home.join("config.toml")) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(_) => {
                    bail!("ai_approval_configuration_unavailable: 无法核对 Codex 配置，请重试")
                }
            };
            Ok(ApprovalTarget {
                revision: format!(
                    "{}:{}:{:x}",
                    network.revision,
                    network_fingerprint,
                    Sha256::digest(bytes)
                ),
                receiver: "Codex：接收方未核实，可能使用远程模型".into(),
                endpoint: None,
                model: None,
                verified: false,
            })
        }
        ExecutionTarget::Api {
            service_config_id,
            service_revision,
            network_revision,
            model_id,
        } => {
            let service = crate::ai_services::config::AiServiceStore::for_home(home)
                .configured_service(service_config_id)?;
            if service.revision != *service_revision || network.revision != *network_revision {
                bail!("ai_approval_stale: 服务或网络配置已变化，请重新预检并确认")
            }
            // Show only the origin. Paths may contain tenant identifiers or tokens.
            let url = url::Url::parse(&service.base_url)?;
            Ok(ApprovalTarget {
                revision: format!(
                    "{}:{}:{}",
                    service.revision, network.revision, network_fingerprint
                ),
                receiver: format!(
                    "{} / {}",
                    service.display_name,
                    service.provider_id.as_str()
                ),
                endpoint: Some(url.origin().ascii_serialization()),
                model: Some(model_id.clone()),
                verified: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_services::{
        config::AiServiceStore,
        credentials::tests_support::MemoryCredentialStore,
        network::NetworkStore,
        types::{AiProtocol, AiProviderId, SaveAiServiceInput, SetNetworkSettingsInput},
    };
    use std::sync::Arc;

    #[test]
    fn configured_receiver_is_sanitized_and_service_network_revisions_are_enforced() {
        let temp = tempfile::tempdir().unwrap();
        let store = AiServiceStore::new(
            temp.path().join("ai-services.json"),
            Arc::new(MemoryCredentialStore::default()),
        );
        let settings = store
            .save(SaveAiServiceInput {
                expected_revision: 0,
                id: None,
                provider_id: AiProviderId::Custom,
                display_name: "Private service".into(),
                protocol: AiProtocol::OpenaiChatCompletions,
                base_url: Some("https://example.com/private-tenant/v1".into()),
                model_id: Some("model".into()),
                api_key: Some("secret".into()),
            })
            .unwrap();
        let service = &settings.services[0];
        let target = ExecutionTarget::Api {
            service_config_id: service.id.clone(),
            service_revision: service.revision,
            network_revision: 0,
            model_id: "model".into(),
        };
        let result = configuration_at(&target, temp.path(), temp.path()).unwrap();
        assert_eq!(result.endpoint.as_deref(), Some("https://example.com"));
        assert_eq!(result.model.as_deref(), Some("model"));
        let stale = ExecutionTarget::Api {
            service_config_id: service.id.clone(),
            service_revision: service.revision + 1,
            network_revision: 0,
            model_id: "model".into(),
        };
        assert!(configuration_at(&stale, temp.path(), temp.path()).is_err());
        NetworkStore::for_home(temp.path())
            .set(SetNetworkSettingsInput {
                expected_revision: 0,
                custom_proxy_url: Some("http://127.0.0.1:8080".into()),
            })
            .unwrap();
        assert!(configuration_at(&target, temp.path(), temp.path()).is_err());
    }

    #[test]
    fn codex_remains_unverified_and_changed_local_configuration_invalidates_fingerprint() {
        let temp = tempfile::tempdir().unwrap();
        let before = configuration_at(&ExecutionTarget::Codex, temp.path(), temp.path()).unwrap();
        assert!(!before.verified);
        std::fs::write(temp.path().join("config.toml"), "model = 'changed-model'").unwrap();
        let after = configuration_at(&ExecutionTarget::Codex, temp.path(), temp.path()).unwrap();
        assert!(!after.verified);
        assert_ne!(before.revision, after.revision);
        assert!(!after.revision.contains("changed-model"));
    }
}
