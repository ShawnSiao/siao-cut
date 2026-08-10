use std::sync::OnceLock;

use super::{
    error::AiError,
    types::{AiProviderCatalog, AiProviderCatalogEntry, AiProviderId},
};

const CATALOG_JSON: &str = include_str!("../../resources/ai-provider-catalog.json");
static CATALOG: OnceLock<AiProviderCatalog> = OnceLock::new();

pub fn catalog() -> Result<&'static AiProviderCatalog, AiError> {
    if let Some(catalog) = CATALOG.get() {
        return Ok(catalog);
    }
    let parsed = serde_json::from_str::<AiProviderCatalog>(CATALOG_JSON)
        .map_err(|_| AiError::ConfigurationRead)?;
    validate(&parsed)?;
    let _ = CATALOG.set(parsed);
    CATALOG.get().ok_or(AiError::ConfigurationRead)
}

pub fn provider(id: AiProviderId) -> Result<&'static AiProviderCatalogEntry, AiError> {
    catalog()?
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| AiError::Validation("不支持该 AI 服务厂商".to_owned()))
}

fn validate(catalog: &AiProviderCatalog) -> Result<(), AiError> {
    if catalog.schema_version != 1 || catalog.providers.len() != 7 {
        return Err(AiError::ConfigurationRead);
    }
    for provider in &catalog.providers {
        if provider.display_name.trim().is_empty() || !provider.models_path.starts_with('/') {
            return Err(AiError::ConfigurationRead);
        }
        if provider.id != AiProviderId::Custom && provider.official_base_url.is_none() {
            return Err(AiError::ConfigurationRead);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_contains_all_supported_providers() {
        let catalog = catalog().expect("catalog");
        assert_eq!(catalog.providers.len(), 7);
        assert!(
            catalog
                .providers
                .iter()
                .all(|entry| !entry.models_path.is_empty())
        );
    }
}
