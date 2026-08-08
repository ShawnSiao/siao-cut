use crate::{db, media::hash_file, util};
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMigration {
    pub source_root: Option<PathBuf>,
    pub target_root: PathBuf,
    pub source_available: bool,
    pub source_removed: bool,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub status: LocalResourceStatus,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCleanup {
    pub files_removed: u64,
    pub directories_removed: u64,
    pub bytes_reclaimed: u64,
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

pub(crate) fn configured_root() -> Result<Option<PathBuf>> {
    Ok(read_config_at(&config_path())?.map(|config| config.root))
}

pub(crate) fn managed_entrypoint(key: &str) -> Result<Option<PathBuf>> {
    Ok(read_config_at(&config_path())?.and_then(|config| path_for(&config, key)))
}

pub(crate) fn activate_managed_resource(
    component: &str,
    version: &str,
    entrypoints: &[(&str, &Path)],
    capability: Option<&str>,
    profile: Option<&str>,
) -> Result<()> {
    validate_config_segment(component)?;
    validate_config_segment(version)?;
    if let Some(capability) = capability {
        capability_name(capability)?;
    }
    if let Some(profile) = profile {
        validate_profile(profile)?;
    }
    let locator = config_path();
    let mut config = read_config_at(&locator)?
        .ok_or_else(|| anyhow!("resource_setup_required: 请先选择并确认本地资源保存位置"))?;
    for (key, path) in entrypoints {
        if !path.is_file() {
            bail!("resource_activation_failed: 本地资源入口不存在")
        }
        let relative = path
            .strip_prefix(&config.root)
            .map_err(|_| anyhow!("resource_activation_failed: 本地资源入口超出所选保存位置"))?;
        validate_relative_path(relative)?;
        config
            .active_entrypoints
            .insert((*key).to_owned(), relative.to_string_lossy().into_owned());
    }
    config
        .active_versions
        .insert(component.to_owned(), version.to_owned());
    if let Some(capability) = capability
        && !config
            .enabled_capabilities
            .iter()
            .any(|enabled| enabled == capability)
    {
        config.enabled_capabilities.push(capability.to_owned());
        config.enabled_capabilities.sort();
    }
    if let Some(profile) = profile {
        config.transcription_profile = profile.to_owned();
    }
    config.updated_at = util::now();
    write_config_at(&locator, &config)
}

fn ready_file(config: &LocalResourceConfig, key: &str) -> bool {
    path_for(config, key).is_some_and(|path| path.is_file())
}

fn selected_model_ready(config: &LocalResourceConfig) -> bool {
    let Ok(model_id) = model_id_for_profile(&config.transcription_profile) else {
        return false;
    };
    ready_file(config, "default_model")
        && config
            .active_versions
            .get("transcription-model")
            .is_some_and(|version| version.starts_with(&format!("{model_id}-")))
}

fn capability_ready(config: &LocalResourceConfig, capability: &str) -> bool {
    let basic = ready_file(config, "ffmpeg") && ready_file(config, "ffprobe");
    match capability {
        "basic_media" => basic,
        "url_import" => basic && ready_file(config, "yt_dlp"),
        "local_transcription" => {
            basic && ready_file(config, "whisper") && selected_model_ready(config)
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
    ensure_no_active_jobs(database)?;
    configure_at(&config_path(), root, &db::home_dir())
}

fn ensure_no_active_jobs(database: &Connection) -> Result<()> {
    let active: bool = database.query_row(
        "SELECT
             EXISTS(SELECT 1 FROM resource_jobs WHERE status IN ('queued','running'))
             OR EXISTS(SELECT 1 FROM model_downloads WHERE status IN ('queued','running'))
             OR EXISTS(SELECT 1 FROM speaker_jobs WHERE status IN ('queued','running'))",
        [],
        |row| row.get(0),
    )?;
    if active {
        bail!("resource_job_active: 本地资源准备期间不能更改保存位置")
    }
    Ok(())
}

fn directory_files(root: &Path) -> Result<Vec<(PathBuf, u64)>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| anyhow!("resource_move_failed: 无法读取待迁移资源：{error}"))?
        {
            let entry = entry
                .map_err(|error| anyhow!("resource_move_failed: 无法读取待迁移资源：{error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| anyhow!("resource_move_failed: 无法检查待迁移资源：{error}"))?;
            if file_type.is_symlink() {
                bail!("resource_move_failed: 本地资源目录包含不支持的链接")
            }
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                files.push((path, entry.metadata()?.len()));
            }
        }
    }
    Ok(files)
}

fn managed_files(root: &Path) -> Result<Vec<(PathBuf, PathBuf, u64)>> {
    let mut files = Vec::new();
    for directory in ["packages", "models", "downloads"] {
        let source = root.join(directory);
        for (path, size) in directory_files(&source)? {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| anyhow!("resource_move_failed: 待迁移资源路径无效"))?
                .to_path_buf();
            files.push((path, relative, size));
        }
    }
    Ok(files)
}

fn copy_managed_files(files: &[(PathBuf, PathBuf, u64)], workspace: &Path) -> Result<(u64, u64)> {
    let mut bytes = 0_u64;
    for (source, relative, expected_size) in files {
        let target = workspace.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| anyhow!("resource_move_failed: 无法创建迁移目录：{error}"))?;
        }
        let copied = fs::copy(source, &target)
            .map_err(|error| anyhow!("resource_move_failed: 无法复制本地资源：{error}"))?;
        if copied != *expected_size
            || fs::metadata(&target)?.len() != *expected_size
            || hash_file(source)? != hash_file(&target)?
        {
            bail!("resource_move_verification_failed: 本地资源复制后校验失败")
        }
        bytes = bytes.saturating_add(copied);
    }
    Ok((files.len() as u64, bytes))
}

fn prepare_empty_migration_target(target: &Path) -> Result<(PathBuf, PathBuf, bool)> {
    validate_root_shape(target)?;
    let created = !target.exists();
    if !created {
        if !target.is_dir() {
            bail!("resource_move_target_not_empty: 新的保存位置不是文件夹")
        }
        if fs::read_dir(target)?.next().is_some() {
            bail!("resource_move_target_not_empty: 请选择空文件夹作为新的保存位置")
        }
    } else {
        fs::create_dir_all(target).map_err(|error| {
            anyhow!("resource_root_not_writable: 无法创建新的资源目录：{error}")
        })?;
    }
    let canonical = target
        .canonicalize()
        .map_err(|error| anyhow!("resource_root_invalid: 无法解析新的资源目录：{error}"))?;
    let staging = canonical.join("staging");
    fs::create_dir_all(&staging)?;
    write_probe(&canonical)?;
    let workspace = staging.join(format!("move-{}", util::new_id("resource")));
    fs::create_dir_all(&workspace)?;
    Ok((canonical, workspace, created))
}

fn cleanup_migration_target(target: &Path, remove_empty_root: bool) {
    for directory in ["packages", "models", "downloads", "staging"] {
        let path = target.join(directory);
        if path.is_dir() {
            let _ = fs::remove_dir_all(path);
        }
    }
    if remove_empty_root {
        let _ = fs::remove_dir(target);
    }
}

fn cleanup_migrated_source(source: &Path) -> bool {
    let mut removed = true;
    for directory in ["packages", "models", "downloads", "staging"] {
        let path = source.join(directory);
        if path.is_dir() && fs::remove_dir_all(path).is_err() {
            removed = false;
        }
    }
    removed && fs::remove_dir(source).is_ok()
}

fn migrate_at(
    locator: &Path,
    target: &Path,
    data_home: &Path,
    available_override: Option<u64>,
) -> Result<ResourceMigration> {
    let Some(current) = read_config_at(locator)? else {
        let status = configure_at(locator, target, data_home)?;
        let target_root = status
            .root
            .clone()
            .ok_or_else(|| anyhow!("resource_root_invalid: 新的资源目录无效"))?;
        return Ok(ResourceMigration {
            source_root: None,
            target_root,
            source_available: false,
            source_removed: false,
            files_copied: 0,
            bytes_copied: 0,
            status,
        });
    };
    if target.exists() && same_path(&current.root, target) {
        let status = status_at(locator)?;
        return Ok(ResourceMigration {
            source_root: Some(current.root.clone()),
            target_root: current.root,
            source_available: true,
            source_removed: false,
            files_copied: 0,
            bytes_copied: 0,
            status,
        });
    }

    let source_available = current.root.is_dir();
    let files = if source_available {
        managed_files(&current.root)?
    } else {
        Vec::new()
    };
    let required_bytes = files
        .iter()
        .map(|(_, _, size)| *size)
        .sum::<u64>()
        .saturating_add(128 * 1024 * 1024);
    let (target_root, workspace, target_created) = prepare_empty_migration_target(target)?;
    if same_path(&target_root, data_home) {
        cleanup_migration_target(&target_root, target_created);
        bail!("resource_root_contains_user_data: 本地资源不能直接保存到项目数据目录")
    }
    if source_available {
        let source_root = current.root.canonicalize()?;
        if target_root.starts_with(&source_root) || source_root.starts_with(&target_root) {
            cleanup_migration_target(&target_root, target_created);
            bail!("resource_move_target_invalid: 新旧资源目录不能互相包含")
        }
    }
    let available = available_override.unwrap_or(util::available_space(&target_root)?);
    if available < required_bytes {
        cleanup_migration_target(&target_root, target_created);
        bail!("resource_insufficient_space: 新的保存位置空间不足")
    }

    let copied = copy_managed_files(&files, &workspace).inspect_err(|_| {
        cleanup_migration_target(&target_root, target_created);
    })?;
    let activation = (|| -> Result<()> {
        for directory in ["packages", "models", "downloads"] {
            let staged = workspace.join(directory);
            let final_path = target_root.join(directory);
            if staged.is_dir() {
                fs::rename(&staged, &final_path)
                    .map_err(|error| anyhow!("resource_move_failed: 无法启用迁移资源：{error}"))?;
            } else {
                fs::create_dir_all(&final_path)?;
            }
        }
        Ok(())
    })();
    if let Err(error) = activation {
        cleanup_migration_target(&target_root, target_created);
        return Err(error);
    }
    let _ = fs::remove_dir_all(&workspace);

    let mut next = current.clone();
    next.root = target_root.clone();
    next.updated_at = util::now();
    if source_available
        && next.active_entrypoints.values().any(|relative| {
            resolve_entrypoint(&target_root, relative).is_none_or(|path| !path.is_file())
        })
    {
        cleanup_migration_target(&target_root, target_created);
        bail!("resource_move_verification_failed: 迁移后的资源不完整，仍保留原位置")
    }
    if let Err(error) = write_config_at(locator, &next) {
        cleanup_migration_target(&target_root, target_created);
        return Err(error);
    }
    let source_removed = source_available && cleanup_migrated_source(&current.root);
    let status = status_at(locator)?;
    Ok(ResourceMigration {
        source_root: Some(current.root),
        target_root,
        source_available,
        source_removed,
        files_copied: copied.0,
        bytes_copied: copied.1,
        status,
    })
}

pub fn migrate(database: &Connection, target: &Path) -> Result<ResourceMigration> {
    ensure_no_active_jobs(database)?;
    migrate_at(&config_path(), target, &db::home_dir(), None)
}

fn path_size(path: &Path) -> Result<(u64, u64)> {
    let files = directory_files(path)?;
    Ok((
        files.len() as u64,
        files.iter().map(|(_, size)| *size).sum(),
    ))
}

fn cleanup_at(database: &Connection, locator: &Path) -> Result<ResourceCleanup> {
    ensure_no_active_jobs(database)?;
    let config = read_config_at(locator)?
        .ok_or_else(|| anyhow!("resource_setup_required: 请先选择并确认本地资源保存位置"))?;
    if !config.root.is_dir() {
        bail!("resource_root_unavailable: 已选择的本地资源保存位置当前不可用")
    }
    let mut report = ResourceCleanup {
        files_removed: 0,
        directories_removed: 0,
        bytes_reclaimed: 0,
    };
    for category in ["packages", "models"] {
        let category_root = config.root.join(category);
        if !category_root.is_dir() {
            continue;
        }
        for component in fs::read_dir(&category_root)? {
            let component = component?;
            if !component.file_type()?.is_dir() {
                continue;
            }
            let component_id = component.file_name().to_string_lossy().into_owned();
            let active = config.active_versions.get(&component_id);
            for version in fs::read_dir(component.path())? {
                let version = version?;
                if !version.file_type()?.is_dir()
                    || active.is_some_and(|active| version.file_name() == active.as_str())
                {
                    continue;
                }
                let (files, bytes) = path_size(&version.path())?;
                fs::remove_dir_all(version.path())
                    .map_err(|error| anyhow!("resource_cleanup_failed: 无法清理旧资源：{error}"))?;
                report.files_removed += files;
                report.directories_removed += 1;
                report.bytes_reclaimed = report.bytes_reclaimed.saturating_add(bytes);
            }
        }
    }
    let downloads = config.root.join("downloads");
    if downloads.is_dir() {
        for (path, size) in directory_files(&downloads)? {
            if path
                .extension()
                .is_some_and(|extension| extension == "part")
            {
                continue;
            }
            fs::remove_file(&path).map_err(|error| {
                anyhow!("resource_cleanup_failed: 无法清理已完成的下载文件：{error}")
            })?;
            report.files_removed += 1;
            report.bytes_reclaimed = report.bytes_reclaimed.saturating_add(size);
        }
    }
    let staging = config.root.join("staging");
    if staging.is_dir() {
        for entry in fs::read_dir(&staging)? {
            let entry = entry?;
            let (files, bytes) = if entry.file_type()?.is_dir() {
                path_size(&entry.path())?
            } else {
                (1, entry.metadata()?.len())
            };
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
                report.directories_removed += 1;
            } else {
                fs::remove_file(entry.path())?;
            }
            report.files_removed += files;
            report.bytes_reclaimed = report.bytes_reclaimed.saturating_add(bytes);
        }
    }
    Ok(report)
}

pub fn cleanup(database: &Connection) -> Result<ResourceCleanup> {
    cleanup_at(database, &config_path())
}

pub(crate) fn validate_profile(profile: &str) -> Result<()> {
    if !matches!(profile, "fast" | "standard" | "quality") {
        bail!("resource_profile_invalid: 未知的本地转录方案");
    }
    Ok(())
}

pub(crate) fn model_id_for_profile(profile: &str) -> Result<&'static str> {
    validate_profile(profile)?;
    Ok(match profile {
        "fast" => "tiny",
        "standard" => "base",
        "quality" => "small",
        _ => unreachable!("profile was validated"),
    })
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
    let whisper = component_size(&catalog, "whisper-cpu-upstream").unwrap_or(0);
    let model_id = model_id_for_profile(profile)?;
    let model = model_size(&catalog, model_id).unwrap_or(0);
    let speaker = catalog
        .get("speakerPackages")
        .and_then(Value::as_array)
        .and_then(|packages| packages.first())
        .and_then(|package| package.get("downloadSize"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let config = read_config_at(&config_path())?;
    let needs_basic = config
        .as_ref()
        .is_none_or(|config| !capability_ready(config, "basic_media"));
    let needs_url = config
        .as_ref()
        .is_none_or(|config| !ready_file(config, "yt_dlp"));
    let needs_whisper = config
        .as_ref()
        .is_none_or(|config| !ready_file(config, "whisper"));
    let needs_model = config.as_ref().is_none_or(|config| {
        config.transcription_profile != profile || !selected_model_ready(config)
    });
    let needs_speaker = config
        .as_ref()
        .is_none_or(|config| !ready_file(config, "speaker"));
    let basic_bytes = if needs_basic { ffmpeg } else { 0 };
    let (download_bytes, unknown_size, transcription_profile) = match capability {
        "basic_media" => (basic_bytes, false, None),
        "url_import" => (
            basic_bytes.saturating_add(if needs_url { yt_dlp } else { 0 }),
            false,
            None,
        ),
        "local_transcription" => (
            basic_bytes
                .saturating_add(if needs_whisper { whisper } else { 0 })
                .saturating_add(if needs_model { model } else { 0 }),
            false,
            Some(profile.to_owned()),
        ),
        "speaker_identity" => (
            basic_bytes.saturating_add(if needs_speaker { speaker } else { 0 }),
            false,
            None,
        ),
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
    fn migrates_verified_resources_before_switching_the_active_root() {
        let temp = tempdir().unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let configured = configure_at(&locator, &first, &data).unwrap();
        let first = configured.root.unwrap();
        let ffmpeg = first.join("packages/ffmpeg-cpu/test/ffmpeg.exe");
        let ffprobe = first.join("packages/ffmpeg-cpu/test/ffprobe.exe");
        for path in [&ffmpeg, &ffprobe] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }
        let mut config = read_config_at(&locator).unwrap().unwrap();
        config.enabled_capabilities.push("basic_media".into());
        config
            .active_versions
            .insert("ffmpeg-cpu".into(), "test".into());
        config.active_entrypoints.insert(
            "ffmpeg".into(),
            ffmpeg
                .strip_prefix(&first)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
        config.active_entrypoints.insert(
            "ffprobe".into(),
            ffprobe
                .strip_prefix(&first)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
        write_config_at(&locator, &config).unwrap();

        let migration = migrate_at(&locator, &second, &data, Some(u64::MAX)).unwrap();

        assert!(migration.source_available);
        assert!(migration.source_removed);
        assert_eq!(migration.files_copied, 2);
        assert!(migration.status.capabilities[0].enabled);
        assert_eq!(migration.status.capabilities[0].state, "ready");
        assert!(!first.exists());
        let stored = read_config_at(&locator).unwrap().unwrap();
        assert!(same_path(&stored.root, &second));
        assert!(
            stored
                .root
                .join("packages/ffmpeg-cpu/test/ffmpeg.exe")
                .is_file()
        );
    }

    #[test]
    fn low_space_migration_keeps_the_original_configuration_active() {
        let temp = tempdir().unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let configured = configure_at(&locator, &first, &data).unwrap();
        let first = configured.root.unwrap();
        let asset = first.join("downloads/existing.bin.part");
        fs::write(&asset, b"partial download").unwrap();
        fs::create_dir_all(&second).unwrap();

        let error = migrate_at(&locator, &second, &data, Some(0)).unwrap_err();

        assert!(
            error
                .to_string()
                .starts_with("resource_insufficient_space:")
        );
        let stored = read_config_at(&locator).unwrap().unwrap();
        assert!(same_path(&stored.root, &first));
        assert!(asset.is_file());
        assert!(second.is_dir());
        assert!(fs::read_dir(&second).unwrap().next().is_none());
    }

    #[test]
    fn migration_rejects_a_target_nested_inside_the_current_root() {
        let temp = tempdir().unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let first = temp.path().join("first");
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let configured = configure_at(&locator, &first, &data).unwrap();
        let first = configured.root.unwrap();
        let nested = first.join("nested/new-location");

        let error = migrate_at(&locator, &nested, &data, Some(u64::MAX)).unwrap_err();

        assert!(
            error
                .to_string()
                .starts_with("resource_move_target_invalid:")
        );
        assert!(same_path(
            &read_config_at(&locator).unwrap().unwrap().root,
            &first
        ));
        assert!(!nested.exists());
    }

    #[test]
    fn migration_never_deletes_unknown_files_from_the_old_root() {
        let temp = tempdir().unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let configured = configure_at(&locator, &first, &data).unwrap();
        let first = configured.root.unwrap();
        fs::write(first.join("keep-me.txt"), b"not managed by SiaoCut").unwrap();
        fs::write(first.join("downloads/managed.bin"), b"managed").unwrap();

        let migration = migrate_at(&locator, &second, &data, Some(u64::MAX)).unwrap();

        assert!(!migration.source_removed);
        assert!(first.join("keep-me.txt").is_file());
        assert!(!first.join("downloads").exists());
        assert!(
            migration
                .target_root
                .join("downloads/managed.bin")
                .is_file()
        );
    }

    #[test]
    fn missing_resource_root_can_be_reselected_and_marked_for_repair() {
        let temp = tempdir().unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let first = temp.path().join("missing");
        let second = temp.path().join("replacement");
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        configure_at(&locator, &first, &data).unwrap();
        let mut config = read_config_at(&locator).unwrap().unwrap();
        config.enabled_capabilities.push("basic_media".into());
        config
            .active_versions
            .insert("ffmpeg-cpu".into(), "missing".into());
        config
            .active_entrypoints
            .insert("ffmpeg".into(), "packages/missing/ffmpeg.exe".into());
        config
            .active_entrypoints
            .insert("ffprobe".into(), "packages/missing/ffprobe.exe".into());
        write_config_at(&locator, &config).unwrap();
        fs::remove_dir_all(&first).unwrap();

        let migration = migrate_at(&locator, &second, &data, Some(u64::MAX)).unwrap();

        assert!(!migration.source_available);
        assert_eq!(migration.files_copied, 0);
        assert_eq!(migration.status.capabilities[0].state, "needs_repair");
        assert!(same_path(
            &read_config_at(&locator).unwrap().unwrap().root,
            &second
        ));
    }

    #[test]
    fn cleanup_preserves_active_versions_and_resumable_partials() {
        let temp = tempdir().unwrap();
        let database = db::open_at(&temp.path().join("siaocut.db")).unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let root = temp.path().join("resources");
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let configured = configure_at(&locator, &root, &data).unwrap();
        let root = configured.root.unwrap();
        let active = root.join("packages/ffmpeg-cpu/current/ffmpeg.exe");
        let old = root.join("packages/ffmpeg-cpu/old/ffmpeg.exe");
        let completed = root.join("downloads/completed.zip");
        let partial = root.join("downloads/resumable.zip.part");
        let orphan = root.join("staging/orphan/file.bin");
        for (path, bytes) in [
            (&active, b"active".as_slice()),
            (&old, b"old".as_slice()),
            (&completed, b"complete".as_slice()),
            (&partial, b"partial".as_slice()),
            (&orphan, b"orphan".as_slice()),
        ] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let mut config = read_config_at(&locator).unwrap().unwrap();
        config
            .active_versions
            .insert("ffmpeg-cpu".into(), "current".into());
        write_config_at(&locator, &config).unwrap();

        let report = cleanup_at(&database, &locator).unwrap();

        assert!(report.files_removed >= 3);
        assert!(report.bytes_reclaimed > 0);
        assert!(active.is_file());
        assert!(!old.exists());
        assert!(!completed.exists());
        assert!(partial.is_file());
        assert!(!orphan.exists());
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
        assert!(!fast.unknown_size);
        assert_eq!(fast.transcription_profile.as_deref(), Some("fast"));
        assert!(quality.download_bytes > fast.download_bytes);
        assert_eq!(url.capability_name, "URL 导入");
        assert!(!url.unknown_size);
        assert!(url.download_bytes > 0);
    }

    #[test]
    fn transcription_readiness_binds_the_selected_profile_to_its_active_model() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("resources");
        let ffmpeg = root.join("packages/media/ffmpeg.exe");
        let ffprobe = root.join("packages/media/ffprobe.exe");
        let whisper = root.join("packages/transcription/whisper-cli.exe");
        let model = root.join("models/transcription-model/base-test/ggml-base.bin");
        for path in [&ffmpeg, &ffprobe, &whisper, &model] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }
        let mut config = default_config(root.clone());
        config
            .enabled_capabilities
            .push("local_transcription".into());
        config
            .active_versions
            .insert("transcription-model".into(), "base-60ed5bc3dd14".into());
        for (key, path) in [
            ("ffmpeg", &ffmpeg),
            ("ffprobe", &ffprobe),
            ("whisper", &whisper),
            ("default_model", &model),
        ] {
            config.active_entrypoints.insert(
                key.into(),
                path.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }

        assert!(capability_ready(&config, "local_transcription"));
        config.transcription_profile = "quality".into();
        assert!(!capability_ready(&config, "local_transcription"));
    }
}
