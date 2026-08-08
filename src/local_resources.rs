use crate::{db, util};
use anyhow::{Result, anyhow, bail};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Component, Path, PathBuf},
};

const SCHEMA_VERSION: u32 = 1;
const DEFAULT_PROFILE: &str = "standard";
const CATALOG: &str = include_str!("../release/runtime-manifest.json");
const CAPABILITIES: [(&str, &str); 4] = [
    ("basic_media", "基础媒体处理"),
    ("url_import", "URL 导入"),
    ("local_transcription", "本地转录"),
    ("speaker_identity", "说话人识别"),
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalResourceConfig {
    pub schema_version: u32,
    pub root: PathBuf,
    pub transcription_profile: String,
    #[serde(default)]
    pub enabled_capabilities: Vec<String>,
    #[serde(default)]
    pub active_entrypoints: BTreeMap<String, String>,
    #[serde(default)]
    pub active_versions: BTreeMap<String, String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityStatus {
    pub id: &'static str,
    pub name: &'static str,
    pub state: &'static str,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalResourceStatus {
    pub configured: bool,
    pub root: Option<PathBuf>,
    pub root_available: bool,
    pub writable: bool,
    pub available_bytes: Option<u64>,
    pub transcription_profile: String,
    pub capabilities: Vec<CapabilityStatus>,
    pub needs_setup: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePlan {
    pub capability_id: String,
    pub capability_name: &'static str,
    pub transcription_profile: Option<String>,
    pub download_bytes: u64,
    pub unknown_size: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceHealth {
    pub status: LocalResourceStatus,
    pub healthy: bool,
    pub reason_code: Option<&'static str>,
}

fn local_app_data() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().expect("current directory is available"))
}

pub fn config_path() -> PathBuf {
    env::var_os("SIAOCUT_RESOURCE_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| local_app_data().join("SiaoCut").join("config"))
        .join("local-resources.json")
}

fn default_config(root: PathBuf) -> LocalResourceConfig {
    LocalResourceConfig {
        schema_version: SCHEMA_VERSION,
        root,
        transcription_profile: DEFAULT_PROFILE.to_owned(),
        enabled_capabilities: Vec::new(),
        active_entrypoints: BTreeMap::new(),
        active_versions: BTreeMap::new(),
        updated_at: util::now(),
    }
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

pub(crate) fn read_config_at(path: &Path) -> Result<Option<LocalResourceConfig>> {
    let source = if path.is_file() {
        Some(path.to_path_buf())
    } else {
        let backup = backup_path(path);
        backup.is_file().then_some(backup)
    };
    let Some(source) = source else {
        return Ok(None);
    };
    let bytes = fs::read(&source)
        .map_err(|error| anyhow!("resource_config_invalid: 无法读取本地资源配置：{error}"))?;
    let config: LocalResourceConfig = serde_json::from_slice(&bytes)
        .map_err(|error| anyhow!("resource_config_invalid: 本地资源配置格式有误：{error}"))?;
    validate_config(&config)?;
    Ok(Some(config))
}

fn validate_config(config: &LocalResourceConfig) -> Result<()> {
    if config.schema_version != SCHEMA_VERSION {
        bail!(
            "resource_config_version_unsupported: 本地资源配置版本 {} 暂不支持",
            config.schema_version
        );
    }
    validate_root_shape(&config.root)?;
    validate_profile(&config.transcription_profile)?;
    for capability in &config.enabled_capabilities {
        capability_name(capability)?;
    }
    for relative in config.active_entrypoints.values() {
        validate_relative_path(Path::new(relative))?;
    }
    for (component, version) in &config.active_versions {
        validate_config_segment(component)?;
        validate_config_segment(version)?;
    }
    Ok(())
}

fn validate_config_segment(value: &str) -> Result<()> {
    let mut components = Path::new(value).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        bail!("resource_config_invalid: 本地资源版本路径无效")
    }
    Ok(())
}

fn validate_root_shape(root: &Path) -> Result<()> {
    if !root.is_absolute() || root.parent().is_none() {
        bail!("resource_root_invalid: 请选择本机磁盘中的具体文件夹");
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("resource_config_invalid: 本地资源入口路径无效");
    }
    Ok(())
}

pub fn resolve_entrypoint(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    validate_relative_path(relative).ok()?;
    Some(root.join(relative))
}

fn path_for(config: &LocalResourceConfig, key: &str) -> Option<PathBuf> {
    config
        .active_entrypoints
        .get(key)
        .and_then(|relative| resolve_entrypoint(&config.root, relative))
}

fn ready_file(config: &LocalResourceConfig, key: &str) -> bool {
    path_for(config, key).is_some_and(|path| path.is_file())
}

fn capability_ready(config: &LocalResourceConfig, capability: &str) -> bool {
    let basic = ready_file(config, "ffmpeg") && ready_file(config, "ffprobe");
    match capability {
        "basic_media" => basic,
        "url_import" => basic && ready_file(config, "yt_dlp"),
        "local_transcription" => {
            basic
                && ready_file(config, "whisper")
                && ready_file(config, "whisper_vad_model")
                && ready_file(config, "default_model")
        }
        "speaker_identity" => basic && ready_file(config, "speaker"),
        _ => false,
    }
}

fn nearest_existing(path: &Path) -> Option<&Path> {
    path.ancestors().find(|candidate| candidate.exists())
}

fn status_at(config_path: &Path) -> Result<LocalResourceStatus> {
    let Some(config) = read_config_at(config_path)? else {
        return Ok(LocalResourceStatus {
            configured: false,
            root: None,
            root_available: false,
            writable: false,
            available_bytes: None,
            transcription_profile: DEFAULT_PROFILE.to_owned(),
            capabilities: CAPABILITIES
                .iter()
                .map(|(id, name)| CapabilityStatus {
                    id,
                    name,
                    state: "not_ready",
                    enabled: false,
                })
                .collect(),
            needs_setup: true,
        });
    };
    let root_available = config.root.is_dir();
    let writable = root_available
        && fs::metadata(&config.root)
            .map(|metadata| !metadata.permissions().readonly())
            .unwrap_or(false);
    let available_bytes =
        nearest_existing(&config.root).and_then(|path| util::available_space(path).ok());
    let capabilities = CAPABILITIES
        .iter()
        .map(|(id, name)| {
            let enabled = config
                .enabled_capabilities
                .iter()
                .any(|enabled| enabled == id);
            CapabilityStatus {
                id,
                name,
                state: if capability_ready(&config, id) {
                    "ready"
                } else if enabled {
                    "needs_repair"
                } else {
                    "not_ready"
                },
                enabled,
            }
        })
        .collect::<Vec<_>>();
    let needs_setup = !root_available;
    Ok(LocalResourceStatus {
        configured: true,
        root: Some(config.root),
        root_available,
        writable,
        available_bytes,
        transcription_profile: config.transcription_profile,
        capabilities,
        needs_setup,
    })
}

pub fn status() -> Result<LocalResourceStatus> {
    status_at(&config_path())
}

pub(crate) fn write_probe(root: &Path) -> Result<()> {
    let staging = root.join("staging");
    let probe = staging.join(format!(".write-probe-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
        .map_err(|error| anyhow!("resource_root_not_writable: 无法写入所选资源目录：{error}"))?;
    if let Err(error) = file.write_all(b"SiaoCut local resource write probe") {
        let _ = fs::remove_file(&probe);
        return Err(anyhow!(
            "resource_root_not_writable: 无法写入所选资源目录：{error}"
        ));
    }
    drop(file);
    fs::remove_file(&probe).map_err(|error| {
        anyhow!("resource_root_not_writable: 无法清理资源目录检查文件：{error}")
    })?;
    Ok(())
}

fn prepare_root(root: &Path) -> Result<PathBuf> {
    validate_root_shape(root)?;
    for directory in ["packages", "models", "downloads", "staging"] {
        fs::create_dir_all(root.join(directory)).map_err(|error| {
            anyhow!("resource_root_not_writable: 无法创建本地资源目录：{error}")
        })?;
    }
    write_probe(root)?;
    root.canonicalize()
        .map_err(|error| anyhow!("resource_root_invalid: 无法解析本地资源目录：{error}"))
}

pub(crate) fn same_path(first: &Path, second: &Path) -> bool {
    let first = first.canonicalize().unwrap_or_else(|_| first.to_path_buf());
    let second = second
        .canonicalize()
        .unwrap_or_else(|_| second.to_path_buf());
    first == second
}

pub(crate) fn write_config_at(path: &Path, config: &LocalResourceConfig) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("resource_config_write_failed: 本地资源配置目录无效"))?;
    fs::create_dir_all(parent).map_err(|error| {
        anyhow!("resource_config_write_failed: 无法创建本地资源配置目录：{error}")
    })?;
    let partial = path.with_extension("json.partial");
    let backup = backup_path(path);
    if partial.is_file() {
        fs::remove_file(&partial).map_err(|error| {
            anyhow!("resource_config_write_failed: 无法清理未完成配置：{error}")
        })?;
    }
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| anyhow!("resource_config_write_failed: 无法生成本地资源配置：{error}"))?;
    fs::write(&partial, bytes)
        .map_err(|error| anyhow!("resource_config_write_failed: 无法写入本地资源配置：{error}"))?;
    if path.is_file() {
        if backup.is_file() {
            fs::remove_file(&backup).map_err(|error| {
                anyhow!("resource_config_write_failed: 无法清理旧配置备份：{error}")
            })?;
        }
        fs::rename(path, &backup).map_err(|error| {
            anyhow!("resource_config_write_failed: 无法备份当前资源配置：{error}")
        })?;
    }
    if let Err(error) = fs::rename(&partial, path) {
        if backup.is_file() && !path.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(anyhow!(
            "resource_config_write_failed: 无法启用新的资源配置：{error}"
        ));
    }
    if backup.is_file() {
        fs::remove_file(&backup).map_err(|error| {
            anyhow!("resource_config_write_failed: 无法清理资源配置备份：{error}")
        })?;
    }
    Ok(())
}

fn configure_at(config_path: &Path, root: &Path, data_home: &Path) -> Result<LocalResourceStatus> {
    let prepared = prepare_root(root)?;
    if same_path(&prepared, data_home) {
        bail!("resource_root_contains_user_data: 本地资源不能直接保存到项目数据目录");
    }
    let existing = read_config_at(config_path)?;
    if existing.as_ref().is_some_and(|config| {
        !same_path(&config.root, &prepared)
            && (!config.active_entrypoints.is_empty() || !config.enabled_capabilities.is_empty())
    }) {
        bail!("resource_move_required: 当前资源需要通过「更改保存位置」迁移");
    }
    let mut config = existing.unwrap_or_else(|| default_config(prepared.clone()));
    config.root = prepared;
    config.updated_at = util::now();
    write_config_at(config_path, &config)?;
    status_at(config_path)
}

pub fn configure(database: &Connection, root: &Path) -> Result<LocalResourceStatus> {
    let active: bool = database.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM resource_jobs WHERE status IN ('queued','running')
         )",
        [],
        |row| row.get(0),
    )?;
    if active {
        bail!("resource_job_active: 本地资源准备期间不能更改保存位置")
    }
    configure_at(&config_path(), root, &db::home_dir())
}

fn validate_profile(profile: &str) -> Result<()> {
    if !matches!(profile, "fast" | "standard" | "quality") {
        bail!("resource_profile_invalid: 未知的本地转录方案");
    }
    Ok(())
}

fn capability_name(capability: &str) -> Result<&'static str> {
    CAPABILITIES
        .iter()
        .find_map(|(id, name)| (*id == capability).then_some(*name))
        .ok_or_else(|| anyhow!("resource_capability_invalid: 未知的本地能力"))
}

fn component_size(catalog: &Value, id: &str) -> Option<u64> {
    catalog
        .get("components")?
        .as_array()?
        .iter()
        .find(|component| component.get("id").and_then(Value::as_str) == Some(id))?
        .get("size")?
        .as_u64()
}

fn model_size(catalog: &Value, id: &str) -> Option<u64> {
    catalog
        .get("models")?
        .as_array()?
        .iter()
        .find(|model| model.get("id").and_then(Value::as_str) == Some(id))?
        .get("size")?
        .as_u64()
}

pub fn plan(capability: &str, profile: Option<&str>) -> Result<ResourcePlan> {
    let capability_name = capability_name(capability)?;
    let profile = profile.unwrap_or(DEFAULT_PROFILE);
    validate_profile(profile)?;
    let catalog: Value = serde_json::from_str(CATALOG)
        .map_err(|error| anyhow!("resource_catalog_invalid: 内置资源清单无效：{error}"))?;
    let ffmpeg = component_size(&catalog, "ffmpeg-cpu").unwrap_or(0);
    let yt_dlp = component_size(&catalog, "yt-dlp").unwrap_or(0);
    let vad = component_size(&catalog, "whisper-vad-silero-6.2").unwrap_or(0);
    let model_id = match profile {
        "fast" => "tiny",
        "standard" => "base",
        "quality" => "small",
        _ => unreachable!("profile was validated"),
    };
    let model = model_size(&catalog, model_id).unwrap_or(0);
    let speaker = catalog
        .get("speakerPackages")
        .and_then(Value::as_array)
        .and_then(|packages| packages.first())
        .and_then(|package| package.get("downloadSize"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let (download_bytes, unknown_size, transcription_profile) = match capability {
        "basic_media" => (ffmpeg, false, None),
        "url_import" => (ffmpeg.saturating_add(yt_dlp), false, None),
        "local_transcription" => (
            ffmpeg.saturating_add(vad).saturating_add(model),
            true,
            Some(profile.to_owned()),
        ),
        "speaker_identity" => (ffmpeg.saturating_add(speaker), false, None),
        _ => unreachable!("capability was validated"),
    };
    Ok(ResourcePlan {
        capability_id: capability.to_owned(),
        capability_name,
        transcription_profile,
        download_bytes,
        unknown_size,
    })
}

pub fn health() -> Result<ResourceHealth> {
    let config_path = config_path();
    let status = status_at(&config_path)?;
    if !status.configured {
        return Ok(ResourceHealth {
            status,
            healthy: false,
            reason_code: Some("resource_setup_required"),
        });
    }
    if !status.root_available {
        return Ok(ResourceHealth {
            status,
            healthy: false,
            reason_code: Some("resource_root_unavailable"),
        });
    }
    let config = read_config_at(&config_path)?
        .ok_or_else(|| anyhow!("resource_setup_required: 请先选择本地资源保存位置"))?;
    write_probe(&config.root)?;
    Ok(ResourceHealth {
        status,
        healthy: true,
        reason_code: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn status_does_not_create_a_default_configuration() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config/local-resources.json");
        let root = temp.path().join("unused-root");

        let status = status_at(&config).unwrap();

        assert!(!status.configured);
        assert!(status.needs_setup);
        assert_eq!(status.root, None);
        assert!(!config.exists());
        assert!(!root.exists());
    }

    #[test]
    fn configures_a_separate_writable_resource_root() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config/local-resources.json");
        let root = temp.path().join("resources");
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();

        let status = configure_at(&config, &root, &data).unwrap();

        assert!(status.configured);
        assert!(status.root_available);
        assert!(status.writable);
        assert!(!status.needs_setup);
        let configured_root = status.root.as_ref().unwrap();
        for directory in ["packages", "models", "downloads", "staging"] {
            assert!(configured_root.join(directory).is_dir());
        }
        let stored = read_config_at(&config).unwrap().unwrap();
        assert_eq!(stored.transcription_profile, "standard");
        assert_eq!(&stored.root, configured_root);
    }

    #[test]
    fn rejects_relative_and_drive_root_locations() {
        assert!(validate_root_shape(Path::new("relative/resources")).is_err());
        assert!(validate_root_shape(Path::new(r"C:\")).is_err());
    }

    #[test]
    fn refuses_to_use_the_project_data_root_directly() {
        let temp = tempdir().unwrap();
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let error = configure_at(
            &temp.path().join("config/local-resources.json"),
            &data,
            &data,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("resource_root_contains_user_data:")
        );
    }

    #[test]
    fn active_resources_require_the_migration_command_to_change_roots() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config/local-resources.json");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        configure_at(&config_path, &first, &data).unwrap();
        let mut config = read_config_at(&config_path).unwrap().unwrap();
        config.enabled_capabilities.push("basic_media".into());
        write_config_at(&config_path, &config).unwrap();

        let error = configure_at(&config_path, &second, &data).unwrap_err();

        assert!(error.to_string().starts_with("resource_move_required:"));
    }

    #[test]
    fn active_download_prevents_changing_the_confirmed_root() {
        let temp = tempdir().unwrap();
        let database = db::open_at(&temp.path().join("siaocut.db")).unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        configure_at(&locator, &first, &data).unwrap();
        database
            .execute(
                "INSERT INTO resource_jobs(
                     id,capability_id,status,stage,total_bytes,target_root,created_at,updated_at
                 ) VALUES('resource-active','basic_media','queued','queued',1,?1,'now','now')",
                [first.to_string_lossy().as_ref()],
            )
            .unwrap();

        let error = configure(&database, &second).unwrap_err();

        assert!(error.to_string().starts_with("resource_job_active:"));
        assert!(!second.exists());
    }

    #[test]
    fn resolves_only_relative_managed_entrypoints() {
        let root = Path::new(r"D:\SiaoCut\LocalResources");
        assert_eq!(
            resolve_entrypoint(root, r"packages\media\8.1\ffmpeg.exe"),
            Some(root.join(r"packages\media\8.1\ffmpeg.exe"))
        );
        assert!(resolve_entrypoint(root, r"..\outside.exe").is_none());
        assert!(resolve_entrypoint(root, r"C:\outside.exe").is_none());
    }

    #[test]
    fn plans_product_capabilities_without_exposing_component_details() {
        let fast = plan("local_transcription", Some("fast")).unwrap();
        let quality = plan("local_transcription", Some("quality")).unwrap();
        let url = plan("url_import", None).unwrap();

        assert_eq!(fast.capability_name, "本地转录");
        assert!(fast.unknown_size);
        assert!(quality.download_bytes > fast.download_bytes);
        assert_eq!(url.capability_name, "URL 导入");
        assert!(!url.unknown_size);
        assert!(url.download_bytes > 0);
    }
}
