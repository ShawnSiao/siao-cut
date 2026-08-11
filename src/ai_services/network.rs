use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use reqwest::{
    Proxy,
    blocking::{Client, ClientBuilder},
};

use super::{
    endpoint::normalize_proxy_url,
    error::AiError,
    storage,
    types::{NetworkSettings, NetworkSettingsFile, SetNetworkSettingsInput},
};

const SETTINGS_FILE_NAME: &str = "ai-network.json";
const SETTINGS_SCHEMA_VERSION: u32 = 1;

pub struct NetworkStore {
    path: PathBuf,
    mutation_lock: Mutex<()>,
}

impl NetworkStore {
    pub fn for_home(home: &Path) -> Self {
        Self {
            path: home.join(SETTINGS_FILE_NAME),
            mutation_lock: Mutex::new(()),
        }
    }

    fn load(&self) -> Result<NetworkSettingsFile, AiError> {
        let value = storage::read_json(&self.path)
            .map_err(|_| AiError::ConfigurationRead)?
            .unwrap_or(NetworkSettingsFile {
                schema_version: SETTINGS_SCHEMA_VERSION,
                revision: 0,
                custom_proxy_url: None,
            });
        if value.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(AiError::ConfigurationRead);
        }
        Ok(value)
    }

    pub fn snapshot(&self) -> Result<NetworkSettings, AiError> {
        let settings = self.load()?;
        let (proxy, source) = effective_proxy_from(settings.custom_proxy_url.clone());
        Ok(NetworkSettings {
            schema_version: settings.schema_version,
            revision: settings.revision,
            custom_proxy_url: settings.custom_proxy_url,
            effective_mode: if proxy.is_some() || source == "environment" {
                "proxy".to_owned()
            } else {
                "direct".to_owned()
            },
            effective_source: source.to_owned(),
            effective_proxy_address: proxy,
        })
    }

    pub fn set(&self, input: SetNetworkSettingsInput) -> Result<NetworkSettings, AiError> {
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| AiError::ConfigurationWrite)?;
        let mut settings = self.load()?;
        if settings.revision != input.expected_revision {
            return Err(AiError::RevisionConflict);
        }
        settings.custom_proxy_url = normalize_proxy_url(input.custom_proxy_url.as_deref())?;
        settings.revision += 1;
        storage::write_json_atomic(&self.path, &settings)
            .map_err(|_| AiError::ConfigurationWrite)?;
        drop(_guard);
        self.snapshot()
    }

    pub fn remove_file(&self) -> Result<(), AiError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(AiError::ConfigurationWrite),
        }
    }
}

pub fn build_client(builder: ClientBuilder, settings: &NetworkSettings) -> Result<Client, AiError> {
    let builder = match settings.effective_source.as_str() {
        "custom" | "windows_system" => builder.proxy(
            Proxy::all(
                settings
                    .effective_proxy_address
                    .as_deref()
                    .unwrap_or_default(),
            )
            .map_err(|_| AiError::ProviderUnavailable)?,
        ),
        "environment" => builder,
        _ => builder.no_proxy(),
    };
    builder.build().map_err(|_| AiError::ProviderUnavailable)
}

fn effective_proxy_from(custom: Option<String>) -> (Option<String>, &'static str) {
    if let Some(proxy) = custom {
        return (Some(proxy), "custom");
    }
    if environment_proxy_configured() {
        return (None, "environment");
    }
    if let Some(proxy) = windows_system_proxy() {
        return (Some(proxy), "windows_system");
    }
    (None, "direct")
}

fn environment_proxy_configured() -> bool {
    [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

#[cfg(windows)]
fn windows_system_proxy() -> Option<String> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    let settings = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    if settings.get_value::<u32, _>("ProxyEnable").ok()? == 0 {
        return None;
    }
    parse_windows_proxy_server(&settings.get_value::<String, _>("ProxyServer").ok()?)
}

#[cfg(not(windows))]
fn windows_system_proxy() -> Option<String> {
    None
}

fn parse_windows_proxy_server(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let candidate = if raw.contains('=') {
        let entries = raw
            .split(';')
            .filter_map(|entry| entry.split_once('='))
            .map(|(scheme, address)| (scheme.trim().to_ascii_lowercase(), address.trim()))
            .collect::<Vec<_>>();
        entries
            .iter()
            .find(|(scheme, _)| scheme == "https")
            .or_else(|| entries.iter().find(|(scheme, _)| scheme == "http"))
            .map(|(_, address)| *address)?
    } else {
        raw
    };
    let value = if candidate.contains("://") {
        candidate.to_owned()
    } else {
        format!("http://{candidate}")
    };
    normalize_proxy_url(Some(&value)).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_proxy_parser_prefers_https() {
        assert_eq!(
            parse_windows_proxy_server("http=127.0.0.1:8080;https=127.0.0.1:7897"),
            Some("http://127.0.0.1:7897".to_owned())
        );
    }
}
