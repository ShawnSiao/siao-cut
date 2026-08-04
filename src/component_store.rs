//! SiaoCut's product-facing adapter for the shared Siao Component Store.
//!
//! The adapter deliberately exposes component keys and verified entrypoints,
//! never catalog URLs, archive sizes, hashes, or Store-internal files.  The
//! same type is used by detached workers and by the desktop bridge when the
//! core process is embedded in a Tauri task.

#![allow(dead_code)]

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use siao_component_store_catalogs::CatalogBundle;
use siao_component_store_core::{
    Store, StoreError,
    catalog::{CatalogDocument, ComponentRef, ComponentRequirement},
    lease::Lease,
    store::{LeasedComponent, ResolveRequest},
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

pub const CONSUMER_ID: &str = "siaocut";
pub const LEASE_TTL_MS: u64 = 30_000;
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
pub const WHISPER_VERSION: &str = "1.9.1-siao.1";
pub const WHISPER_CPU_RUNTIME_ID: &str = "siao-whisper-cpu";
pub const WHISPER_VULKAN_RUNTIME_ID: &str = "siao-whisper-vulkan";

/// The repository's direct CLI fixtures intentionally run without a Store
/// package.  This switch is test-harness-only compatibility; desktop workers
/// never set it and therefore cannot execute a legacy path.
pub fn legacy_fixture_mode() -> bool {
    cfg!(test) || (cfg!(debug_assertions) && std::env::var_os("SIAOCUT_DIRECT").is_some())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedComponent {
    Ffmpeg,
    YtDlp,
    WhisperCpu,
    WhisperVulkan,
    Vad,
    ModelTiny,
    ModelBase,
    ModelSmall,
}

pub fn parse_component(value: &str) -> Result<SharedComponent> {
    match value {
        "ffmpeg" => Ok(SharedComponent::Ffmpeg),
        "yt-dlp" | "ytdlp" => Ok(SharedComponent::YtDlp),
        "whisper-cpu" => Ok(SharedComponent::WhisperCpu),
        "whisper-vulkan" => Ok(SharedComponent::WhisperVulkan),
        "vad" | "whisper-vad" => Ok(SharedComponent::Vad),
        "tiny" | "whisper-model-tiny" => Ok(SharedComponent::ModelTiny),
        "base" | "whisper-model-base" => Ok(SharedComponent::ModelBase),
        "small" | "whisper-model-small" => Ok(SharedComponent::ModelSmall),
        _ => bail!("component_store_unknown_component: {value}"),
    }
}

pub fn component_label(component: SharedComponent) -> &'static str {
    match component {
        SharedComponent::Ffmpeg => "ffmpeg",
        SharedComponent::YtDlp => "yt-dlp",
        SharedComponent::WhisperCpu => "whisper-cpu",
        SharedComponent::WhisperVulkan => "whisper-vulkan",
        SharedComponent::Vad => "whisper-vad",
        SharedComponent::ModelTiny => "tiny",
        SharedComponent::ModelBase => "base",
        SharedComponent::ModelSmall => "small",
    }
}

/// Stable product-local reference used by CLI/workflow records.  It is a
/// ComponentKey reference, not a filesystem path or distribution metadata.
pub fn component_reference(component: SharedComponent) -> String {
    format!("component:{}", component_label(component))
}

/// Parse the persisted component reference used by SiaoCut's transcription
/// commands.  Absolute paths are deliberately not accepted here; legacy
/// paths are migration input only and must be registered explicitly.
pub fn parse_component_reference(value: &str) -> Result<SharedComponent> {
    let value = value
        .strip_prefix("component:")
        .ok_or_else(|| anyhow!("component_store_model_reference_required"))?;
    let component = parse_component(value)?;
    if matches!(
        component,
        SharedComponent::ModelTiny | SharedComponent::ModelBase | SharedComponent::ModelSmall
    ) {
        Ok(component)
    } else {
        bail!("component_store_invalid_model_reference: 只能选择 tiny、base 或 small 模型")
    }
}

impl SharedComponent {
    fn component_id(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg",
            Self::YtDlp => "yt-dlp",
            Self::WhisperCpu | Self::WhisperVulkan => "whisper-runtime",
            Self::Vad => "whisper-vad",
            Self::ModelTiny | Self::ModelBase | Self::ModelSmall => "whisper-model",
        }
    }

    fn version(self) -> &'static str {
        match self {
            Self::Ffmpeg => "8.1",
            Self::YtDlp => "2026.06.09",
            Self::WhisperCpu | Self::WhisperVulkan => WHISPER_VERSION,
            Self::Vad => "6.2.0",
            Self::ModelTiny | Self::ModelBase | Self::ModelSmall => "1",
        }
    }

    fn required_capabilities(self) -> &'static [&'static str] {
        match self {
            Self::Ffmpeg => &["media_decode", "media_encode", "media_probe"],
            Self::YtDlp => &["url_import"],
            Self::WhisperCpu => &[
                "transcription",
                "vad_timeline.original_media",
                "runtime_metadata",
            ],
            Self::WhisperVulkan => &[
                "transcription",
                "vad_timeline.original_media",
                "runtime_metadata",
                "gpu_acceleration",
            ],
            Self::Vad => &["vad"],
            Self::ModelTiny | Self::ModelBase | Self::ModelSmall => &["transcription_model"],
        }
    }

    fn variant(self) -> BTreeMap<String, String> {
        let mut variant = BTreeMap::from([
            ("platform".into(), "windows".into()),
            ("architecture".into(), "x86_64".into()),
        ]);
        match self {
            Self::Ffmpeg => {
                variant.insert("flavor".into(), "lgpl-shared".into());
            }
            Self::WhisperCpu => {
                variant.insert("backend".into(), "cpu".into());
            }
            Self::WhisperVulkan => {
                variant.insert("backend".into(), "vulkan".into());
            }
            Self::Vad => {
                variant.insert("model".into(), "silero-v6.2".into());
            }
            Self::ModelTiny => {
                variant.insert("model".into(), "tiny".into());
            }
            Self::ModelBase => {
                variant.insert("model".into(), "base".into());
            }
            Self::ModelSmall => {
                variant.insert("model".into(), "small".into());
            }
            Self::YtDlp => {}
        }
        variant
    }

    pub fn key(self) -> ComponentKey {
        ComponentKey {
            component_id: self.component_id().into(),
            version: self.version().into(),
            variant: self.variant(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentKey {
    pub component_id: String,
    #[serde(default)]
    pub version: String,
    pub variant: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComponentPreferences {
    whisper: ComponentKey,
}

fn component_preferences_path() -> PathBuf {
    crate::db::home_dir().join("component-selection.json")
}

pub fn selected_whisper_component() -> SharedComponent {
    let Ok(bytes) = std::fs::read(component_preferences_path()) else {
        return SharedComponent::WhisperCpu;
    };
    let Ok(preferences) = serde_json::from_slice::<ComponentPreferences>(&bytes) else {
        return SharedComponent::WhisperCpu;
    };
    [SharedComponent::WhisperCpu, SharedComponent::WhisperVulkan]
        .into_iter()
        .find(|candidate| {
            let key = candidate.key();
            key.component_id == preferences.whisper.component_id
                && key.variant == preferences.whisper.variant
                && (preferences.whisper.version.is_empty()
                    || key.version == preferences.whisper.version)
        })
        .unwrap_or(SharedComponent::WhisperCpu)
}

pub fn select_whisper_component(component: SharedComponent) -> Result<()> {
    if !matches!(
        component,
        SharedComponent::WhisperCpu | SharedComponent::WhisperVulkan
    ) {
        bail!("component_store_invalid_whisper_selection: 只能选择 CPU 或 Vulkan Whisper")
    }
    let path = component_preferences_path();
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("component_store_preferences_path_invalid"))?;
    std::fs::create_dir_all(parent)?;
    let partial = path.with_extension("json.part");
    std::fs::write(
        &partial,
        serde_json::to_vec_pretty(&ComponentPreferences {
            whisper: component.key(),
        })?,
    )?;
    std::fs::rename(partial, path)?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ComponentManager {
    store: Store,
    catalog: CatalogDocument,
    consumer_id: String,
}

impl ComponentManager {
    pub fn open_default() -> Result<Self> {
        Self::open_for_consumer(CONSUMER_ID)
    }

    pub fn open_for_consumer(consumer_id: impl Into<String>) -> Result<Self> {
        let catalog = CatalogBundle::common_v2()
            .map_err(|error| anyhow!("component_store_catalog_invalid: {error}"))?;
        ensure_catalog_is_formal_v2(&catalog)?;
        let store = Store::open_default(catalog.clone()).map_err(map_store_error)?;
        Ok(Self {
            store,
            catalog,
            consumer_id: consumer_id.into(),
        })
    }

    pub fn from_store(store: Store, consumer_id: impl Into<String>) -> Self {
        let catalog = store.catalog().clone();
        Self {
            store,
            catalog,
            consumer_id: consumer_id.into(),
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn consumer_id(&self) -> &str {
        &self.consumer_id
    }

    pub fn catalog(&self) -> &CatalogDocument {
        &self.catalog
    }

    pub fn catalog_revision() -> &'static str {
        siao_component_store_catalogs::CANONICAL_SOURCE_TAG
    }

    pub fn requirement(&self, component: SharedComponent) -> Result<ComponentRequirement> {
        let key = component.key();
        if !self.catalog.components.iter().any(|candidate| {
            candidate.component_id == key.component_id
                && candidate.version == key.version
                && candidate.variant == key.variant
        }) {
            return Err(anyhow!(
                "component_store_catalog_incomplete: {} {} {:?} 尚未进入完整 common v2",
                key.component_id,
                key.version,
                key.variant
            ));
        }
        Ok(ComponentRequirement {
            component_id: key.component_id,
            version: key.version,
            variant: key.variant,
            capabilities: component
                .required_capabilities()
                .iter()
                .map(|capability| (*capability).into())
                .collect(),
            optional: false,
        })
    }

    pub fn resolve_and_acquire(&self, component: SharedComponent) -> Result<ComponentLease> {
        let requirement = self.requirement(component)?;
        let leased = self
            .store
            .resolve_and_acquire(ResolveRequest::new(requirement, self.consumer_id.clone()))
            .map_err(map_store_error)?;
        Ok(ComponentLease::new(self.store.clone(), leased))
    }

    pub fn resolve_model_and_acquire(&self, component: SharedComponent) -> Result<ComponentLease> {
        if !matches!(
            component,
            SharedComponent::ModelTiny | SharedComponent::ModelBase | SharedComponent::ModelSmall
        ) {
            bail!("component_store_invalid_model_reference: 不是 Whisper 模型")
        }
        self.resolve_and_acquire(component)
    }

    pub fn list_installations(&self) -> Result<Vec<siao_component_store_core::InstallationRecord>> {
        self.store.list_installations().map_err(map_store_error)
    }

    pub fn register_existing_for_consumer(
        &self,
        component: SharedComponent,
        path: impl Into<PathBuf>,
    ) -> Result<()> {
        let requirement = self.requirement(component)?;
        self.store
            .register_existing_for_consumer(
                &ComponentRef {
                    component_id: requirement.component_id,
                    version: requirement.version,
                    variant: requirement.variant,
                },
                path.into(),
                &self.consumer_id,
            )
            .map(|_| ())
            .map_err(map_store_error)
    }
}

#[derive(Debug)]
pub struct ComponentLease {
    store: Store,
    leased: Option<LeasedComponent>,
    heartbeat_signal: Arc<(Mutex<bool>, Condvar)>,
    heartbeat_lost: Arc<AtomicBool>,
    watched_process: Arc<Mutex<Option<u32>>>,
    heartbeat_thread: Option<JoinHandle<()>>,
}

impl ComponentLease {
    fn new(store: Store, leased: LeasedComponent) -> Self {
        let heartbeat_signal = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = Arc::clone(&heartbeat_signal);
        let heartbeat_lost = Arc::new(AtomicBool::new(false));
        let heartbeat_lost_for_thread = Arc::clone(&heartbeat_lost);
        let watched_process = Arc::new(Mutex::new(None));
        let watched_process_for_thread = Arc::clone(&watched_process);
        let heartbeat_store = store.clone();
        let lease_id = leased.lease.lease_id.clone();
        let heartbeat_thread = std::thread::Builder::new()
            .name("siao-component-lease-heartbeat".into())
            .spawn(move || {
                let (stop, wake) = &*signal;
                loop {
                    let guard = stop.lock().expect("lease heartbeat mutex poisoned");
                    let (guard, timeout) = wake
                        .wait_timeout(guard, HEARTBEAT_INTERVAL)
                        .expect("lease heartbeat condvar poisoned");
                    if *guard {
                        return;
                    }
                    drop(guard);
                    if timeout.timed_out() && heartbeat_store.heartbeat(&lease_id).is_err() {
                        if let Ok(process_id) = watched_process_for_thread.lock()
                            && let Some(process_id) = *process_id
                        {
                            crate::util::terminate_process_tree_by_id(process_id);
                        }
                        heartbeat_lost_for_thread.store(true, Ordering::Release);
                        return;
                    }
                }
            })
            .ok();
        Self {
            store,
            leased: Some(leased),
            heartbeat_signal,
            heartbeat_lost,
            watched_process,
            heartbeat_thread,
        }
    }

    /// Register the currently running component child.  If the heartbeat
    /// thread loses the lease, it terminates this process tree before
    /// returning the structured `component_lease_lost` error to the worker.
    pub fn watch_process(&self, child: &std::process::Child) {
        if let Ok(mut process_id) = self.watched_process.lock() {
            *process_id = Some(child.id());
        }
    }

    /// Stop watching a child after it has exited.  The process ID check keeps
    /// a late cleanup from clearing a newer child registered on the same
    /// lease.
    pub fn unwatch_process(&self, process_id: u32) {
        if let Ok(mut watched) = self.watched_process.lock()
            && *watched == Some(process_id)
        {
            *watched = None;
        }
    }

    pub fn ensure_healthy(&self) -> Result<()> {
        if self.heartbeat_lost() {
            bail!("component_lease_lost: heartbeat 已丢失")
        }
        Ok(())
    }

    /// Run a component child while retaining its stdout/stderr output.  The
    /// heartbeat thread can terminate the process tree if the lease expires;
    /// the caller then receives the same structured lease-loss error as the
    /// long-running worker loops.
    pub fn output_with_lease(
        command: &mut std::process::Command,
        lease: Option<&ComponentLease>,
    ) -> Result<std::process::Output> {
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let child = command.spawn()?;
        let process_id = child.id();
        if let Some(lease) = lease {
            lease.watch_process(&child);
        }
        let output = child.wait_with_output();
        if let Some(lease) = lease {
            lease.unwatch_process(process_id);
            lease.ensure_healthy()?;
        }
        Ok(output?)
    }

    fn stop_heartbeat(&mut self) {
        if let Ok(mut stopped) = self.heartbeat_signal.0.lock() {
            *stopped = true;
            self.heartbeat_signal.1.notify_all();
        }
        if let Some(thread) = self.heartbeat_thread.take() {
            let _ = thread.join();
        }
    }

    pub fn lease(&self) -> &Lease {
        &self.leased.as_ref().expect("lease is present").lease
    }

    pub fn component(&self) -> &LeasedComponent {
        self.leased.as_ref().expect("lease is present")
    }

    pub fn entrypoint(&self, name: &str) -> Result<PathBuf> {
        self.component()
            .entrypoints
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("component_store_entrypoint_missing: {name}"))
    }

    pub fn heartbeat(&mut self) -> Result<()> {
        let lease = self
            .leased
            .as_mut()
            .ok_or_else(|| anyhow!("component_store_lease_released"))?;
        lease.lease = self
            .store
            .heartbeat(&lease.lease.lease_id)
            .map_err(|error| {
                self.heartbeat_lost.store(true, Ordering::Release);
                if let Ok(process_id) = self.watched_process.lock()
                    && let Some(process_id) = *process_id
                {
                    crate::util::terminate_process_tree_by_id(process_id);
                }
                anyhow!("component_lease_lost: {}", map_store_error(error))
            })?;
        Ok(())
    }

    pub fn heartbeat_lost(&self) -> bool {
        self.heartbeat_lost.load(Ordering::Acquire)
    }

    pub fn release(&mut self) -> Result<()> {
        self.stop_heartbeat();
        let Some(leased) = self.leased.take() else {
            return Ok(());
        };
        let result = self
            .store
            .release(&leased.lease.lease_id)
            .map_err(map_store_error);
        if self.heartbeat_lost() {
            return result.and_then(|_| Err(anyhow!("component_lease_lost: heartbeat 已丢失")));
        }
        result
    }
}

impl Drop for ComponentLease {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

fn map_store_error(error: StoreError) -> anyhow::Error {
    match error {
        StoreError::AmbiguousSource(detail) => anyhow!("ambiguous_source: {detail}"),
        StoreError::OperationOwned(detail) => anyhow!("operation_owned: {detail}"),
        StoreError::ForeignOrIncompatibleOperation(detail) => {
            anyhow!("foreign_or_incompatible_operation: {detail}")
        }
        StoreError::StaleStoreRoot {
            opened_root,
            current_root,
        } => anyhow!(
            "stale_store_root: openedRoot={} currentRoot={}",
            opened_root.display(),
            current_root.display()
        ),
        other => anyhow!("component_store_error: {other}"),
    }
}

pub fn ensure_catalog_is_formal_v2(catalog: &CatalogDocument) -> Result<()> {
    if catalog.schema_version != 2 || catalog.protocol_version != 1 {
        bail!("component_store_catalog_incomplete: common v2 is required")
    }
    for component in [
        SharedComponent::Ffmpeg,
        SharedComponent::YtDlp,
        SharedComponent::WhisperCpu,
        SharedComponent::WhisperVulkan,
        SharedComponent::Vad,
        SharedComponent::ModelTiny,
        SharedComponent::ModelBase,
        SharedComponent::ModelSmall,
    ] {
        let key = component.key();
        if !catalog.components.iter().any(|candidate| {
            candidate.component_id == key.component_id
                && candidate.version == key.version
                && candidate.variant == key.variant
        }) {
            bail!(
                "component_store_catalog_incomplete: missing {} {:?}",
                key.component_id,
                key.variant
            )
        }
    }
    Ok(())
}

/// Returns a read-only status payload for the desktop health screen.  A
/// missing/incomplete catalog is reported as a gate, not converted into an
/// executable legacy fallback.
pub fn health() -> serde_json::Value {
    let catalog = match CatalogBundle::common_v2() {
        Ok(catalog) => catalog,
        Err(error) => {
            return json!({
                "status": "catalog_invalid",
                "errorCode": "component_store_catalog_invalid",
                "errorMessage": error.to_string(),
                "canonicalRevision": ComponentManager::catalog_revision(),
                "binaryReleaseRepository": siao_component_store_catalogs::BINARY_RELEASE_REPOSITORY,
            });
        }
    };
    if let Err(error) = ensure_catalog_is_formal_v2(&catalog) {
        return json!({
            "status": "catalog_incomplete",
            "errorCode": "component_store_catalog_incomplete",
            "errorMessage": error.to_string(),
            "canonicalRevision": ComponentManager::catalog_revision(),
            "schemaVersion": catalog.schema_version,
            "catalogId": catalog.catalog_id,
            "catalogDigest": catalog.digest().ok(),
            "binaryReleaseRepository": siao_component_store_catalogs::BINARY_RELEASE_REPOSITORY,
        });
    }
    match ComponentManager::open_default() {
        Ok(manager) => match manager.list_installations() {
            Ok(installations) => json!({
                "status": "ready",
                "canonicalRevision": ComponentManager::catalog_revision(),
                "schemaVersion": catalog.schema_version,
                "catalogId": catalog.catalog_id,
                "catalogDigest": catalog.digest().ok(),
                "binaryReleaseRepository": siao_component_store_catalogs::BINARY_RELEASE_REPOSITORY,
                "installations": installations,
            }),
            Err(error) => json!({
                "status": "store_unavailable",
                "errorCode": "component_store_list_installations_failed",
                "errorMessage": error.to_string(),
                "canonicalRevision": ComponentManager::catalog_revision(),
            }),
        },
        Err(error) => json!({
            "status": "store_unavailable",
            "errorCode": store_error_code(&error),
            "errorMessage": error.to_string(),
            "canonicalRevision": ComponentManager::catalog_revision(),
            "schemaVersion": catalog.schema_version,
            "catalogId": catalog.catalog_id,
            "catalogDigest": catalog.digest().ok(),
            "binaryReleaseRepository": siao_component_store_catalogs::BINARY_RELEASE_REPOSITORY,
        }),
    }
}

fn store_error_code(error: &anyhow::Error) -> &'static str {
    let message = error.to_string();
    if message.starts_with("stale_store_root:") {
        "stale_store_root"
    } else if message.starts_with("foreign_or_incompatible_operation:") {
        "foreign_or_incompatible_operation"
    } else if message.starts_with("ambiguous_source:") {
        "ambiguous_source"
    } else if message.starts_with("operation_owned:") {
        "operation_owned"
    } else {
        "component_store_open_failed"
    }
}

/// Returns the product-local Whisper preference without exposing a path or a
/// Store-internal manifest.  Availability is derived from the read-only Store
/// health payload, so an unverified or missing package never appears ready.
pub fn selection_status() -> serde_json::Value {
    let selected = selected_whisper_component();
    let key = selected.key();
    let variant = json!(key.variant.clone());
    let status = health();
    let available = status.get("status").and_then(serde_json::Value::as_str) == Some("ready")
        && status
            .get("installations")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|installations| {
                installations.iter().any(|installation| {
                    installation
                        .get("componentId")
                        .and_then(serde_json::Value::as_str)
                        == Some(key.component_id.as_str())
                        && installation.get("version") == Some(&json!(key.version.clone()))
                        && installation.get("variant") == Some(&variant)
                        && installation
                            .get("verificationStatus")
                            .and_then(serde_json::Value::as_str)
                            == Some("verified")
                })
            });
    json!({
        "backend": match selected {
            SharedComponent::WhisperVulkan => "vulkan",
            _ => "cpu",
        },
        "componentKey": key,
        "selected": true,
        "available": available,
        "verificationStatus": if available { "verified" } else { "not_configured" },
    })
}

/// Returns the model choices exposed to product health consumers.  The
/// response intentionally contains only ComponentKey identity and verified
/// state; distribution metadata remains owned by the canonical catalog.
pub fn model_statuses() -> serde_json::Value {
    let health = health();
    let installations = health
        .get("installations")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    [
        SharedComponent::ModelTiny,
        SharedComponent::ModelBase,
        SharedComponent::ModelSmall,
    ]
    .into_iter()
    .map(|component| {
        let key = component.key();
        let verified = installations.iter().any(|installation| {
            installation
                .get("componentId")
                .and_then(serde_json::Value::as_str)
                == Some(key.component_id.as_str())
                && installation.get("version") == Some(&json!(key.version.clone()))
                && installation.get("variant") == Some(&json!(key.variant.clone()))
                && installation
                    .get("verificationStatus")
                    .and_then(serde_json::Value::as_str)
                    == Some("verified")
        });
        json!({
            "componentKey": key,
            "available": verified,
            "verificationStatus": if verified { "verified" } else { "not_configured" },
        })
    })
    .collect::<Vec<_>>()
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_keys_do_not_contain_distribution_metadata() {
        let key = SharedComponent::ModelBase.key();
        assert_eq!(key.component_id, "whisper-model");
        assert_eq!(key.version, "1");
        assert_eq!(key.variant.get("model").map(String::as_str), Some("base"));
    }

    #[test]
    fn legacy_whisper_preference_without_version_remains_selectable() {
        let legacy = ComponentPreferences {
            whisper: ComponentKey {
                component_id: "whisper-runtime".into(),
                version: String::new(),
                variant: SharedComponent::WhisperVulkan.variant(),
            },
        };
        let selected = [SharedComponent::WhisperCpu, SharedComponent::WhisperVulkan]
            .into_iter()
            .find(|candidate| {
                let key = candidate.key();
                key.component_id == legacy.whisper.component_id
                    && key.variant == legacy.whisper.variant
                    && (legacy.whisper.version.is_empty() || key.version == legacy.whisper.version)
            });
        assert_eq!(selected, Some(SharedComponent::WhisperVulkan));
    }

    #[test]
    fn common_v2_catalog_contains_formal_whisper_identities() {
        let catalog = CatalogBundle::common_v2().unwrap();
        ensure_catalog_is_formal_v2(&catalog).expect("common v2 must be executable");
        assert_eq!(WHISPER_VERSION, "1.9.1-siao.1");
        for (backend, runtime_id) in [
            ("cpu", WHISPER_CPU_RUNTIME_ID),
            ("vulkan", WHISPER_VULKAN_RUNTIME_ID),
        ] {
            let runtime = catalog
                .components
                .iter()
                .find(|component| {
                    component.component_id == "whisper-runtime"
                        && component.variant.get("backend").map(String::as_str) == Some(backend)
                })
                .expect("formal whisper runtime should be present");
            assert_eq!(runtime.version, WHISPER_VERSION);
            assert_eq!(
                runtime
                    .metadata
                    .get("runtimeId")
                    .and_then(|value| value.as_str()),
                Some(runtime_id)
            );
        }
    }

    #[test]
    fn component_keys_match_formal_catalog_identity() {
        let catalog = CatalogBundle::common_v2().unwrap();
        for component in [
            SharedComponent::Ffmpeg,
            SharedComponent::YtDlp,
            SharedComponent::WhisperCpu,
            SharedComponent::WhisperVulkan,
            SharedComponent::Vad,
            SharedComponent::ModelTiny,
            SharedComponent::ModelBase,
            SharedComponent::ModelSmall,
        ] {
            let key = component.key();
            assert!(
                catalog.components.iter().any(|candidate| {
                    candidate.component_id == key.component_id
                        && candidate.version == key.version
                        && candidate.variant == key.variant
                }),
                "component key must identify an exact common v2 entry: {key:?}"
            );
        }
    }

    #[test]
    fn whisper_requirements_enforce_timeline_metadata_and_vulkan_capabilities() {
        let catalog = CatalogBundle::common_v2().unwrap();
        for component in [SharedComponent::WhisperCpu, SharedComponent::WhisperVulkan] {
            let key = component.key();
            let catalog_component = catalog
                .components
                .iter()
                .find(|candidate| {
                    candidate.component_id == key.component_id && candidate.variant == key.variant
                })
                .expect("formal whisper catalog entry should be present");
            for capability in component.required_capabilities() {
                assert!(
                    catalog_component
                        .capabilities
                        .iter()
                        .any(|candidate| candidate == capability),
                    "catalog must expose required capability {capability}"
                );
            }
        }
        assert!(
            !SharedComponent::WhisperCpu
                .required_capabilities()
                .contains(&"gpu_acceleration")
        );
        assert!(
            SharedComponent::WhisperVulkan
                .required_capabilities()
                .contains(&"gpu_acceleration")
        );
    }

    #[test]
    fn component_manager_requirements_carry_the_formal_whisper_contract() {
        let temp = tempfile::tempdir().expect("temporary Store root should be created");
        let store = Store::open(siao_component_store_core::store::StoreConfig::new(
            temp.path(),
            CatalogBundle::common_v2().expect("common v2 should parse"),
        ))
        .expect("test Store should open");
        let manager = ComponentManager::from_store(store, CONSUMER_ID);

        let cpu = manager
            .requirement(SharedComponent::WhisperCpu)
            .expect("CPU requirement should resolve");
        assert_eq!(cpu.version, WHISPER_VERSION);
        assert_eq!(
            cpu.capabilities,
            vec![
                "transcription".to_owned(),
                "vad_timeline.original_media".to_owned(),
                "runtime_metadata".to_owned(),
            ]
        );

        let vulkan = manager
            .requirement(SharedComponent::WhisperVulkan)
            .expect("Vulkan requirement should resolve");
        assert!(
            vulkan
                .capabilities
                .iter()
                .any(|capability| capability == "gpu_acceleration")
        );
    }

    #[test]
    fn component_references_are_keys_not_paths() {
        assert_eq!(
            component_reference(SharedComponent::ModelBase),
            "component:base"
        );
        assert_eq!(
            parse_component_reference("component:tiny").unwrap(),
            SharedComponent::ModelTiny
        );
        assert!(parse_component_reference("C:\\Models\\base.bin").is_err());
        assert!(parse_component_reference("component:ffmpeg").is_err());
    }

    #[test]
    fn model_statuses_expose_identity_without_distribution_metadata() {
        let statuses = model_statuses();
        let models = statuses.as_array().expect("model status array");
        assert_eq!(models.len(), 3);
        assert!(models.iter().all(|model| {
            model.get("componentKey").is_some()
                && model.get("url").is_none()
                && model.get("sha256").is_none()
                && model.get("size").is_none()
        }));
    }
}
