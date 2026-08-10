use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    catalog,
    credentials::{CredentialStore, WindowsCredentialStore},
    endpoint::normalize_service_endpoint,
    error::AiError,
    storage,
    types::{
        AiProviderId, AiServiceConfig, AiServiceSettings, AiServiceSummary, ConnectionState,
        CredentialState, SaveAiServiceInput, ServiceMutationInput, SetDefaultAiServiceInput,
    },
};

const SETTINGS_FILE_NAME: &str = "ai-services.json";
const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiSettingsFile {
    schema_version: u32,
    revision: u64,
    services: Vec<AiServiceConfig>,
    default_service_id: Option<String>,
}

impl Default for AiSettingsFile {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            revision: 0,
            services: Vec::new(),
            default_service_id: None,
        }
    }
}

pub struct AiServiceStore {
    path: PathBuf,
    credentials: Arc<dyn CredentialStore>,
    mutation_lock: Mutex<()>,
}

impl AiServiceStore {
    pub fn for_home(home: &Path) -> Self {
        Self::new(
            home.join(SETTINGS_FILE_NAME),
            Arc::new(WindowsCredentialStore),
        )
    }

    pub fn new(path: PathBuf, credentials: Arc<dyn CredentialStore>) -> Self {
        Self {
            path,
            credentials,
            mutation_lock: Mutex::new(()),
        }
    }

    fn load(&self) -> Result<AiSettingsFile, AiError> {
        let settings: AiSettingsFile = storage::read_json(&self.path)
            .map_err(|_| AiError::ConfigurationRead)?
            .unwrap_or_default();
        if settings.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(AiError::ConfigurationRead);
        }
        Ok(settings)
    }

    fn persist(&self, settings: &AiSettingsFile) -> Result<(), AiError> {
        storage::write_json_atomic(&self.path, settings).map_err(|_| AiError::ConfigurationWrite)
    }

    pub fn snapshot(&self) -> Result<AiServiceSettings, AiError> {
        let settings = self.load()?;
        let services = settings
            .services
            .iter()
            .map(|service| self.summary(service, settings.default_service_id.as_deref()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AiServiceSettings {
            schema_version: settings.schema_version,
            revision: settings.revision,
            provider_catalog: catalog::catalog()?.clone(),
            services,
            default_service_id: settings.default_service_id,
        })
    }

    pub fn save(&self, input: SaveAiServiceInput) -> Result<AiServiceSettings, AiError> {
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| AiError::ConfigurationWrite)?;
        let mut settings = self.load()?;
        ensure_revision(settings.revision, input.expected_revision)?;
        let service = normalize_service(&settings, &input)?;
        let existing_secret = self.credentials.read(&service.id)?;
        if let Some(secret) = input.api_key.as_deref() {
            let secret = secret.trim();
            if secret.is_empty() {
                return Err(AiError::Validation("API Key 不能为空".to_owned()));
            }
            self.credentials.write(&service.id, secret)?;
        }
        if let Some(index) = settings
            .services
            .iter()
            .position(|item| item.id == service.id)
        {
            settings.services[index] = service.clone();
        } else {
            settings.services.push(service.clone());
        }
        settings.revision += 1;
        if let Err(error) = self.persist(&settings) {
            restore_secret(
                self.credentials.as_ref(),
                &service.id,
                existing_secret.as_deref(),
            );
            return Err(error);
        }
        drop(_guard);
        self.snapshot()
    }

    pub fn delete(&self, input: ServiceMutationInput) -> Result<AiServiceSettings, AiError> {
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| AiError::ConfigurationWrite)?;
        let mut settings = self.load()?;
        ensure_revision(settings.revision, input.expected_revision)?;
        let index = settings
            .services
            .iter()
            .position(|service| service.id == input.id)
            .ok_or(AiError::ServiceNotFound)?;
        let existing_secret = self.credentials.read(&input.id)?;
        self.credentials.delete(&input.id)?;
        settings.services.remove(index);
        if settings.default_service_id.as_deref() == Some(input.id.as_str()) {
            settings.default_service_id = None;
        }
        settings.revision += 1;
        if let Err(error) = self.persist(&settings) {
            restore_secret(
                self.credentials.as_ref(),
                &input.id,
                existing_secret.as_deref(),
            );
            return Err(error);
        }
        drop(_guard);
        self.snapshot()
    }

    pub fn delete_credential(
        &self,
        input: ServiceMutationInput,
    ) -> Result<AiServiceSettings, AiError> {
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| AiError::ConfigurationWrite)?;
        let mut settings = self.load()?;
        ensure_revision(settings.revision, input.expected_revision)?;
        let service = settings
            .services
            .iter_mut()
            .find(|service| service.id == input.id)
            .ok_or(AiError::ServiceNotFound)?;
        let existing_secret = self.credentials.read(&input.id)?;
        self.credentials.delete(&input.id)?;
        service.connection_state = ConnectionState::Untested;
        service.last_tested_at = None;
        service.last_error_code = None;
        service.revision += 1;
        settings.revision += 1;
        if let Err(error) = self.persist(&settings) {
            restore_secret(
                self.credentials.as_ref(),
                &input.id,
                existing_secret.as_deref(),
            );
            return Err(error);
        }
        drop(_guard);
        self.snapshot()
    }

    pub fn set_default(
        &self,
        input: SetDefaultAiServiceInput,
    ) -> Result<AiServiceSettings, AiError> {
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| AiError::ConfigurationWrite)?;
        let mut settings = self.load()?;
        ensure_revision(settings.revision, input.expected_revision)?;
        if let Some(id) = input.id.as_deref()
            && !settings.services.iter().any(|service| service.id == id)
        {
            return Err(AiError::ServiceNotFound);
        }
        settings.default_service_id = input.id;
        settings.revision += 1;
        self.persist(&settings)?;
        drop(_guard);
        self.snapshot()
    }

    pub fn configured_service(&self, id: &str) -> Result<AiServiceConfig, AiError> {
        self.load()?
            .services
            .into_iter()
            .find(|service| service.id == id)
            .ok_or(AiError::ServiceNotFound)
    }

    pub fn stored_credential(&self, id: &str) -> Result<Option<String>, AiError> {
        self.credentials.read(id)
    }

    pub fn mark_test_result(
        &self,
        service_id: &str,
        result: Result<(), &AiError>,
    ) -> Result<(), AiError> {
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| AiError::ConfigurationWrite)?;
        let mut settings = self.load()?;
        let service = settings
            .services
            .iter_mut()
            .find(|service| service.id == service_id)
            .ok_or(AiError::ServiceNotFound)?;
        service.last_tested_at = Some(Utc::now().to_rfc3339());
        match result {
            Ok(()) => {
                service.connection_state = ConnectionState::Ready;
                service.last_error_code = None;
            }
            Err(error) => {
                service.connection_state = ConnectionState::Error;
                service.last_error_code = Some(error.code().to_owned());
            }
        }
        settings.revision += 1;
        self.persist(&settings)
    }

    pub fn purge(&self) -> Result<(), AiError> {
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| AiError::ConfigurationWrite)?;
        let settings = self.load()?;
        for service in settings.services {
            self.credentials.delete(&service.id)?;
        }
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(AiError::ConfigurationWrite),
        }
    }

    fn summary(
        &self,
        service: &AiServiceConfig,
        default_id: Option<&str>,
    ) -> Result<AiServiceSummary, AiError> {
        Ok(AiServiceSummary {
            id: service.id.clone(),
            provider_id: service.provider_id,
            display_name: service.display_name.clone(),
            protocol: service.protocol,
            base_url: service.base_url.clone(),
            model_id: service.model_id.clone(),
            credential_state: if self.credentials.read(&service.id)?.is_some() {
                CredentialState::Stored
            } else {
                CredentialState::Missing
            },
            connection_state: service.connection_state,
            last_tested_at: service.last_tested_at.clone(),
            last_error_code: service.last_error_code.clone(),
            is_default: default_id == Some(service.id.as_str()),
            revision: service.revision,
        })
    }
}

fn normalize_service(
    settings: &AiSettingsFile,
    input: &SaveAiServiceInput,
) -> Result<AiServiceConfig, AiError> {
    let provider = catalog::provider(input.provider_id)?;
    if input.protocol != provider.protocol {
        return Err(AiError::Validation("服务协议与厂商不匹配".to_owned()));
    }
    let existing = input
        .id
        .as_deref()
        .and_then(|id| settings.services.iter().find(|service| service.id == id));
    if input.id.is_some() && existing.is_none() {
        return Err(AiError::ServiceNotFound);
    }
    if existing.is_some_and(|service| service.provider_id != input.provider_id) {
        return Err(AiError::Validation("不能更改已有服务的厂商".to_owned()));
    }
    if input.provider_id != AiProviderId::Custom
        && settings.services.iter().any(|service| {
            service.provider_id == input.provider_id
                && Some(service.id.as_str()) != input.id.as_deref()
        })
    {
        return Err(AiError::Validation("该内置服务已经配置".to_owned()));
    }
    let display_name = if input.display_name.trim().is_empty() {
        provider.display_name.clone()
    } else {
        input.display_name.trim().to_owned()
    };
    if display_name.chars().count() > 64 {
        return Err(AiError::Validation("服务名称过长".to_owned()));
    }
    let base_url = if input.provider_id == AiProviderId::Custom {
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
    let model_id = input
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if model_id.as_ref().is_some_and(|model| model.len() > 200) {
        return Err(AiError::Validation("模型名称过长".to_owned()));
    }
    Ok(AiServiceConfig {
        id: existing
            .map(|service| service.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        provider_id: input.provider_id,
        display_name,
        protocol: input.protocol,
        base_url: normalize_service_endpoint(base_url)?,
        model_id,
        connection_state: ConnectionState::Untested,
        last_tested_at: None,
        last_error_code: None,
        revision: existing.map_or(1, |service| service.revision + 1),
    })
}

fn ensure_revision(actual: u64, expected: u64) -> Result<(), AiError> {
    if actual == expected {
        Ok(())
    } else {
        Err(AiError::RevisionConflict)
    }
}

fn restore_secret(credentials: &dyn CredentialStore, id: &str, secret: Option<&str>) {
    let _ = match secret {
        Some(secret) => credentials.write(id, secret),
        None => credentials.delete(id),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_services::credentials::tests_support::MemoryCredentialStore;
    use tempfile::tempdir;

    #[test]
    fn saves_secret_outside_json_and_supports_explicit_deletion() {
        let temp = tempdir().unwrap();
        let store = AiServiceStore::new(
            temp.path().join("ai-services.json"),
            Arc::new(MemoryCredentialStore::default()),
        );
        let settings = store
            .save(SaveAiServiceInput {
                expected_revision: 0,
                id: None,
                provider_id: AiProviderId::Openai,
                display_name: "OpenAI".into(),
                protocol: super::super::types::AiProtocol::OpenaiResponses,
                base_url: None,
                model_id: Some("gpt-test".into()),
                api_key: Some("secret-value".into()),
            })
            .unwrap();
        let id = settings.services[0].id.clone();
        assert!(
            !std::fs::read_to_string(temp.path().join("ai-services.json"))
                .unwrap()
                .contains("secret-value")
        );
        let settings = store
            .delete_credential(ServiceMutationInput {
                expected_revision: settings.revision,
                id,
            })
            .unwrap();
        assert_eq!(
            settings.services[0].credential_state,
            CredentialState::Missing
        );
    }
}
