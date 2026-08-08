use crate::{
    db,
    local_resources::{
        LocalResourceConfig, config_path, model_id_for_profile, read_config_at, same_path,
        validate_profile, write_config_at, write_probe,
    },
    media::hash_file,
    speaker,
    util::{new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{StatusCode, blocking::Client, header::RANGE};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use zip::ZipArchive;

const CATALOG: &str = include_str!("../release/runtime-manifest.json");
const SPACE_RESERVE: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    components: Vec<CatalogComponent>,
    models: Vec<CatalogModel>,
    speaker_packages: Vec<CatalogSpeakerPackage>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogComponent {
    id: String,
    version: String,
    url: Option<String>,
    size: Option<u64>,
    sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogModel {
    id: String,
    file_name: String,
    url: String,
    size: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogSpeakerPackage {
    id: String,
    version: String,
    downloads: Vec<CatalogDownload>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogDownload {
    id: String,
    url: String,
    size: u64,
    sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SpeakerAssetKind {
    Runtime,
    Segmentation,
    Embedding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InstallKind {
    ZipPackage,
    Executable,
    Model { file_name: String },
    Speaker(SpeakerAssetKind),
}

#[derive(Clone, Debug)]
struct InstallSpec {
    id: String,
    activation_id: String,
    version: String,
    url: String,
    size: u64,
    sha256: String,
    kind: InstallKind,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceJob {
    pub id: String,
    pub capability_id: String,
    pub status: String,
    pub stage: String,
    pub progress: f64,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub target_root: String,
    pub cancel_requested_at: Option<String>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub worker_pid: Option<u32>,
    pub attempt_count: u32,
}

#[derive(Debug)]
struct ComponentActivation {
    id: String,
    version: String,
    entrypoints: BTreeMap<String, PathBuf>,
}

fn parse_catalog() -> Result<Catalog> {
    serde_json::from_str(CATALOG)
        .map_err(|error| anyhow!("resource_catalog_invalid: 内置资源清单无效：{error}"))
}

fn component_spec(catalog: &Catalog, id: &str, kind: InstallKind) -> Result<InstallSpec> {
    let component = catalog
        .components
        .iter()
        .find(|component| component.id == id)
        .ok_or_else(|| anyhow!("resource_catalog_invalid: 内置资源清单缺少 {id}"))?;
    let spec = InstallSpec {
        id: component.id.clone(),
        activation_id: component.id.clone(),
        version: component.version.clone(),
        url: component
            .url
            .clone()
            .ok_or_else(|| anyhow!("resource_catalog_invalid: 内置资源清单缺少 {id} 下载地址"))?,
        size: component
            .size
            .ok_or_else(|| anyhow!("resource_catalog_invalid: 内置资源清单缺少 {id} 文件大小"))?,
        sha256: component
            .sha256
            .clone()
            .ok_or_else(|| anyhow!("resource_catalog_invalid: 内置资源清单缺少 {id} 校验值"))?,
        kind,
    };
    validate_install_spec(&spec)?;
    Ok(spec)
}

fn specs_for_capability(capability: &str, profile: &str) -> Result<Vec<InstallSpec>> {
    let catalog = parse_catalog()?;
    let mut specs = vec![component_spec(
        &catalog,
        "ffmpeg-cpu",
        InstallKind::ZipPackage,
    )?];
    match capability {
        "basic_media" => {}
        "url_import" => specs.push(component_spec(&catalog, "yt-dlp", InstallKind::Executable)?),
        "local_transcription" => {
            validate_profile(profile)?;
            specs.push(component_spec(
                &catalog,
                "whisper-cpu-upstream",
                InstallKind::ZipPackage,
            )?);
            let model_id = model_id_for_profile(profile)?;
            let model = catalog
                .models
                .iter()
                .find(|model| model.id == model_id)
                .ok_or_else(|| anyhow!("resource_catalog_invalid: 内置资源清单缺少转录模型"))?;
            validate_catalog_segment(&model.id)?;
            validate_catalog_segment(&model.file_name)?;
            if model.sha256.len() != 64
                || !model.sha256.chars().all(|value| value.is_ascii_hexdigit())
            {
                bail!("resource_catalog_invalid: 内置转录模型包含无效校验值")
            }
            let spec = InstallSpec {
                id: format!("transcription-model-{}", model.id),
                activation_id: "transcription-model".into(),
                version: format!("{}-{}", model.id, &model.sha256[..12]),
                url: model.url.clone(),
                size: model.size,
                sha256: model.sha256.clone(),
                kind: InstallKind::Model {
                    file_name: model.file_name.clone(),
                },
            };
            validate_install_spec(&spec)?;
            specs.push(spec);
        }
        "speaker_identity" => {
            let package = catalog
                .speaker_packages
                .iter()
                .find(|package| package.id == speaker::PACKAGE_ID)
                .ok_or_else(|| anyhow!("resource_catalog_invalid: 内置资源清单缺少说话人资源包"))?;
            let roles = [
                ("sherpa-onnx-runtime", SpeakerAssetKind::Runtime),
                ("pyannote-segmentation-3.0", SpeakerAssetKind::Segmentation),
                ("3dspeaker-eres2net-base-16k", SpeakerAssetKind::Embedding),
            ];
            for (id, role) in roles {
                let download = package
                    .downloads
                    .iter()
                    .find(|item| item.id == id)
                    .ok_or_else(|| anyhow!("resource_catalog_invalid: 说话人资源包缺少 {id}"))?;
                let spec = InstallSpec {
                    id: format!("speaker-{id}"),
                    activation_id: "speaker_identity".into(),
                    version: package.version.clone(),
                    url: download.url.clone(),
                    size: download.size,
                    sha256: download.sha256.clone(),
                    kind: InstallKind::Speaker(role),
                };
                validate_install_spec(&spec)?;
                specs.push(spec);
            }
        }
        _ => bail!("resource_capability_invalid: 未知的本地能力"),
    }
    Ok(specs)
}

fn validate_install_spec(spec: &InstallSpec) -> Result<()> {
    validate_catalog_segment(&spec.id)?;
    validate_catalog_segment(&spec.activation_id)?;
    validate_catalog_segment(&spec.version)?;
    if spec.sha256.len() != 64 || !spec.sha256.chars().all(|value| value.is_ascii_hexdigit()) {
        bail!("resource_catalog_invalid: 内置资源清单包含无效校验值")
    }
    Ok(())
}

fn validate_catalog_segment(value: &str) -> Result<()> {
    let mut components = Path::new(value).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        bail!("resource_catalog_invalid: 内置资源清单包含无效路径片段")
    }
    Ok(())
}

fn required_entrypoint_keys(component: &str) -> &'static [&'static str] {
    match component {
        "ffmpeg-cpu" => &["ffmpeg", "ffprobe"],
        "yt-dlp" => &["yt_dlp"],
        "whisper-cpu-upstream" => &["whisper"],
        "transcription-model" => &["default_model"],
        "speaker_identity" => &["speaker"],
        _ => &[],
    }
}

fn component_is_active(config: &LocalResourceConfig, spec: &InstallSpec) -> bool {
    config.active_versions.get(&spec.activation_id) == Some(&spec.version)
        && required_entrypoint_keys(&spec.activation_id)
            .iter()
            .all(|key| {
                config
                    .active_entrypoints
                    .get(*key)
                    .is_some_and(|relative| config.root.join(relative).is_file())
            })
}

fn require_config_at(path: &Path) -> Result<LocalResourceConfig> {
    let config = read_config_at(path)?
        .ok_or_else(|| anyhow!("resource_setup_required: 请先选择并确认本地资源保存位置"))?;
    if !config.root.is_dir() {
        bail!("resource_root_unavailable: 已选择的本地资源保存位置当前不可用")
    }
    write_probe(&config.root)?;
    Ok(config)
}

pub fn create_install(
    db: &Connection,
    capability: &str,
    profile: Option<&str>,
) -> Result<ResourceJob> {
    let locator = config_path();
    let mut config = require_config_at(&locator)?;
    if let Some(active) = active_job(db)? {
        if active.capability_id == capability {
            return Ok(active);
        }
        bail!("resource_job_active: 另一项本地资源正在准备中")
    }
    let legacy_job_active: bool = db.query_row(
        "SELECT
             EXISTS(SELECT 1 FROM model_downloads WHERE status IN ('queued','running'))
             OR EXISTS(SELECT 1 FROM speaker_jobs WHERE status IN ('queued','running'))",
        [],
        |row| row.get(0),
    )?;
    if legacy_job_active {
        bail!("resource_job_active: 另一项本地资源正在准备中")
    }
    if capability != "local_transcription" && profile.is_some() {
        bail!("resource_profile_not_applicable: 此能力不使用转录方案")
    }
    if let Some(profile) = profile {
        validate_profile(profile)?;
        if config.transcription_profile != profile {
            config.transcription_profile = profile.to_owned();
            config.updated_at = now();
            write_config_at(&locator, &config)?;
        }
    }
    let specs = specs_for_capability(capability, &config.transcription_profile)?;
    let candidates = legacy_candidate_files(&config.root);
    adopt_verified_assets(&config.root, &specs, &candidates)?;
    create_install_in(db, capability, &locator, &specs, spawn_worker)
}

fn create_install_in(
    db: &Connection,
    capability: &str,
    locator: &Path,
    specs: &[InstallSpec],
    spawn: impl FnOnce(&str, &str) -> Result<()>,
) -> Result<ResourceJob> {
    let config = require_config_at(locator)?;
    if let Some(active) = active_job(db)? {
        if active.capability_id == capability {
            return Ok(active);
        }
        bail!("resource_job_active: 另一项本地资源正在准备中")
    }
    if specs.iter().all(|spec| component_is_active(&config, spec))
        && config
            .enabled_capabilities
            .iter()
            .any(|enabled| enabled == capability)
    {
        bail!("resource_already_ready: 此能力已经可以使用")
    }

    let total_bytes = specs.iter().map(|spec| spec.size).sum::<u64>();
    let partial_bytes = specs
        .iter()
        .map(|spec| {
            if component_is_active(&config, spec) {
                spec.size
            } else {
                verified_download_bytes(&config.root, spec)
            }
        })
        .sum::<u64>();
    let remaining = total_bytes.saturating_sub(partial_bytes);
    let available = crate::util::available_space(&config.root)?;
    if available < remaining.saturating_add(SPACE_RESERVE) {
        bail!(
            "resource_root_low_space: 仍需约 {:.1} MB，本地资源保存位置空间不足",
            remaining as f64 / 1_048_576.0
        )
    }

    let timestamp = now();
    let job = ResourceJob {
        id: new_id("resource"),
        capability_id: capability.to_owned(),
        status: "queued".into(),
        stage: if total_bytes > 0 && partial_bytes == total_bytes {
            "verified".into()
        } else {
            "queued".into()
        },
        progress: if total_bytes == 0 {
            0.0
        } else {
            partial_bytes as f64 / total_bytes as f64
        },
        bytes_downloaded: partial_bytes,
        total_bytes,
        target_root: config.root.to_string_lossy().into_owned(),
        cancel_requested_at: None,
        error_message: None,
        error_code: None,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        completed_at: None,
        worker_pid: None,
        attempt_count: 1,
    };

    db.busy_timeout(Duration::from_secs(5))?;
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)
        .context("resource_job_active: 无法锁定本地资源任务队列")?;
    if let Some(active) = active_job(&tx)? {
        tx.commit()?;
        if active.capability_id == capability {
            return Ok(active);
        }
        bail!("resource_job_active: 另一项本地资源正在准备中")
    }
    tx.execute(
        "INSERT INTO resource_jobs(
             id,capability_id,status,stage,progress,bytes_downloaded,total_bytes,target_root,
             created_at,updated_at,attempt_count
         ) VALUES(?1,?2,'queued',?3,?4,?5,?6,?7,?8,?8,1)",
        params![
            job.id,
            job.capability_id,
            job.stage,
            job.progress,
            job.bytes_downloaded,
            job.total_bytes,
            job.target_root,
            job.created_at
        ],
    )?;
    tx.commit()?;

    if let Err(error) = spawn(&job.id, capability) {
        let timestamp = now();
        let _ = db.execute(
            "UPDATE resource_jobs
             SET status='failed',stage='failed',error_message=?2,updated_at=?3,completed_at=?3
             WHERE id=?1 AND status='queued'",
            params![job.id, error.to_string(), timestamp],
        );
        return Err(error);
    }
    load_job(db, &job.id)
}

fn active_job(db: &Connection) -> Result<Option<ResourceJob>> {
    db.query_row(
        "SELECT id FROM resource_jobs
         WHERE status IN ('queued','running') ORDER BY created_at DESC LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|id| load_job(db, &id))
    .transpose()
}

pub fn load_job(db: &Connection, job_id: &str) -> Result<ResourceJob> {
    db.query_row(
        "SELECT id,capability_id,status,stage,progress,bytes_downloaded,total_bytes,target_root,
                cancel_requested_at,error_message,created_at,updated_at,completed_at,worker_pid,
                attempt_count
         FROM resource_jobs WHERE id=?1",
        [job_id],
        |row| {
            let status = row.get::<_, String>(2)?;
            let error_message = row.get::<_, Option<String>>(9)?;
            Ok(ResourceJob {
                id: row.get(0)?,
                capability_id: row.get(1)?,
                status: status.clone(),
                stage: row.get(3)?,
                progress: row.get(4)?,
                bytes_downloaded: row.get(5)?,
                total_bytes: row.get(6)?,
                target_root: row.get(7)?,
                cancel_requested_at: row.get(8)?,
                error_code: crate::model::background_error_code(&status, error_message.as_deref()),
                error_message,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
                completed_at: row.get(12)?,
                worker_pid: row.get(13)?,
                attempt_count: row.get(14)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("resource_job_not_found: 本地资源任务不存在：{job_id}"))
}

pub fn list_jobs(db: &Connection) -> Result<Vec<ResourceJob>> {
    db.prepare("SELECT id FROM resource_jobs ORDER BY created_at DESC")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| load_job(db, &id))
        .collect()
}

pub fn cancel(db: &Connection, job_id: &str) -> Result<ResourceJob> {
    let timestamp = now();
    let changed = db.execute(
        "UPDATE resource_jobs SET cancel_requested_at=?2,updated_at=?2
         WHERE id=?1 AND status IN ('queued','running')",
        params![job_id, timestamp],
    )?;
    if changed == 0 {
        bail!("resource_job_not_cancellable: 当前本地资源任务不能取消")
    }
    load_job(db, job_id)
}

pub fn resume(db: &Connection, job_id: &str) -> Result<ResourceJob> {
    resume_in(db, job_id, &config_path(), spawn_worker)
}

fn resume_in(
    db: &Connection,
    job_id: &str,
    locator: &Path,
    spawn: impl FnOnce(&str, &str) -> Result<()>,
) -> Result<ResourceJob> {
    let job = load_job(db, job_id)?;
    if !matches!(job.status.as_str(), "cancelled" | "failed" | "interrupted") {
        bail!("resource_job_not_resumable: 当前本地资源任务不能继续")
    }
    let config = require_config_at(locator)?;
    if !same_path(&config.root, Path::new(&job.target_root)) {
        bail!("resource_root_changed: 本地资源保存位置已变化，请重新开始准备")
    }
    if active_job(db)?.is_some() {
        bail!("resource_job_active: 另一项本地资源正在准备中")
    }
    let timestamp = now();
    db.execute(
        "UPDATE resource_jobs
         SET status='queued',stage='queued',cancel_requested_at=NULL,error_message=NULL,
             completed_at=NULL,worker_pid=NULL,updated_at=?2,attempt_count=attempt_count+1
         WHERE id=?1 AND status=?3",
        params![job_id, timestamp, job.status],
    )?;
    if let Err(error) = spawn(job_id, &job.capability_id) {
        let timestamp = now();
        let _ = db.execute(
            "UPDATE resource_jobs
             SET status='failed',stage='failed',error_message=?2,updated_at=?3,completed_at=?3
             WHERE id=?1 AND status='queued'",
            params![job_id, error.to_string(), timestamp],
        );
        return Err(error);
    }
    load_job(db, job_id)
}

pub fn repair(db: &Connection, capability: &str) -> Result<ResourceJob> {
    create_install(db, capability, None)
}

fn components_to_remove(
    config: &LocalResourceConfig,
    capability: &str,
) -> Result<&'static [&'static str]> {
    Ok(match capability {
        "url_import" => &["yt-dlp"],
        "basic_media" => {
            if config.enabled_capabilities.iter().any(|enabled| {
                matches!(
                    enabled.as_str(),
                    "url_import" | "local_transcription" | "speaker_identity"
                )
            }) {
                bail!("resource_dependency_required: 其他本地能力仍需要基础媒体处理")
            }
            &["ffmpeg-cpu"]
        }
        "local_transcription" => &["whisper-cpu-upstream", "transcription-model"],
        "speaker_identity" => &["speaker_identity"],
        _ => bail!("resource_capability_invalid: 未知的本地能力"),
    })
}

pub fn remove(db: &Connection, capability: &str) -> Result<()> {
    let locator = config_path();
    let mut config = require_config_at(&locator)?;
    if active_job(db)?.is_some() {
        bail!("resource_job_active: 请先完成或取消正在进行的本地资源任务")
    }
    let legacy_job_active: bool = db.query_row(
        "SELECT
             EXISTS(SELECT 1 FROM model_downloads WHERE status IN ('queued','running'))
             OR EXISTS(SELECT 1 FROM speaker_jobs WHERE status IN ('queued','running'))",
        [],
        |row| row.get(0),
    )?;
    if legacy_job_active {
        bail!("resource_job_active: 请先完成或取消正在进行的本地资源任务")
    }
    let component_ids = components_to_remove(&config, capability)?;
    let removal_root = config
        .root
        .join("staging")
        .join(format!("remove-{}", new_id("resource")));
    let mut moved_packages = Vec::new();
    for component in component_ids {
        if let Some(version) = config.active_versions.get(*component).cloned() {
            if Path::new(&version).components().count() != 1 {
                bail!("resource_config_invalid: 本地资源版本路径无效")
            }
            let package = if *component == "transcription-model" {
                config.root.join("models").join(component).join(version)
            } else {
                config.root.join("packages").join(component).join(version)
            };
            if package.is_dir() {
                fs::create_dir_all(&removal_root)
                    .context("resource_remove_failed: 无法创建资源移除暂存目录")?;
                let quarantine = removal_root.join(component);
                fs::rename(&package, &quarantine)
                    .context("resource_remove_failed: 无法暂存待移除的本地资源")?;
                moved_packages.push((package, quarantine));
            }
        }
        config.active_versions.remove(*component);
        for key in required_entrypoint_keys(component) {
            config.active_entrypoints.remove(*key);
        }
    }
    config
        .enabled_capabilities
        .retain(|enabled| enabled != capability);
    config.updated_at = now();
    if let Err(error) = write_config_at(&locator, &config) {
        for (package, quarantine) in moved_packages.iter().rev() {
            if quarantine.is_dir() && !package.exists() {
                let _ = fs::rename(quarantine, package);
            }
        }
        return Err(error);
    }
    if removal_root.is_dir() {
        let _ = fs::remove_dir_all(removal_root);
    }
    Ok(())
}

pub fn reconcile_interrupted(db: &Connection) -> Result<()> {
    let jobs = db
        .prepare(
            "SELECT id FROM resource_jobs WHERE status IN ('queued','running') ORDER BY created_at",
        )?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in jobs {
        let job = load_job(db, &id)?;
        let stale = chrono::DateTime::parse_from_rfc3339(&job.updated_at)
            .map(|time| {
                chrono::Utc::now()
                    .signed_duration_since(time.with_timezone(&chrono::Utc))
                    .num_seconds()
                    >= 5
            })
            .unwrap_or(true);
        let worker_alive = job.worker_pid.is_some_and(crate::util::process_is_active);
        if stale && !worker_alive {
            db.execute(
                "UPDATE resource_jobs
                 SET status='interrupted',stage='interrupted',
                     error_message='resource_worker_interrupted: 上次本地资源任务已中断，可以继续。',
                     worker_pid=NULL,updated_at=?2
                 WHERE id=?1 AND status=?3 AND updated_at=?4 AND worker_pid IS ?5",
                params![id, now(), job.status, job.updated_at, job.worker_pid],
            )?;
        }
    }
    Ok(())
}

fn spawn_worker(job_id: &str, capability: &str) -> Result<()> {
    crate::util::spawn_detached_current(&["__resource_worker", job_id, capability])
        .context("resource_worker_start_failed: 无法启动本地资源任务")?;
    Ok(())
}

pub fn run_worker_isolated(job_id: &str, capability: &str) -> Result<()> {
    let job_id = job_id.to_owned();
    let capability = capability.to_owned();
    thread::Builder::new()
        .name("siaocut-resource-worker".into())
        .spawn(move || run_worker(&job_id, &capability))
        .context("resource_worker_start_failed: 无法启动隔离的本地资源线程")?
        .join()
        .map_err(|_| anyhow!("resource_worker_interrupted: 本地资源线程异常退出"))?
}

fn run_worker(job_id: &str, capability: &str) -> Result<()> {
    if let Some(delay) = env::var("SIAOCUT_RESOURCE_START_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        thread::sleep(Duration::from_millis(delay));
    }
    let db = db::open()?;
    let locator = config_path();
    let config = require_config_at(&locator)?;
    let specs = specs_for_capability(capability, &config.transcription_profile)?;
    run_install_attempt_in(&db, job_id, capability, &locator, &specs)
}

fn run_install_attempt_in(
    db: &Connection,
    job_id: &str,
    capability: &str,
    locator: &Path,
    specs: &[InstallSpec],
) -> Result<()> {
    let result = install_in(db, job_id, capability, locator, specs);
    if let Err(error) = &result {
        let timestamp = now();
        let _ = db.execute(
            "UPDATE resource_jobs
             SET status='failed',stage='failed',error_message=?2,worker_pid=NULL,
                 updated_at=?3,completed_at=?3
             WHERE id=?1 AND status!='cancelled'",
            params![job_id, error.to_string(), timestamp],
        );
    }
    result
}

fn install_in(
    db: &Connection,
    job_id: &str,
    capability: &str,
    locator: &Path,
    specs: &[InstallSpec],
) -> Result<()> {
    let job = load_job(db, job_id)?;
    if job.capability_id != capability {
        bail!("resource_job_state_changed: 本地资源任务与目标能力不匹配")
    }
    if job.cancel_requested_at.is_some() {
        finish_cancelled(db, job_id)?;
        return Ok(());
    }
    let mut config = require_config_at(locator)?;
    if !same_path(&config.root, Path::new(&job.target_root)) {
        bail!("resource_root_changed: 本地资源保存位置已变化，任务未写入新位置")
    }
    let changed = db.execute(
        "UPDATE resource_jobs
         SET status='running',stage='downloading',worker_pid=?2,error_message=NULL,updated_at=?3
         WHERE id=?1 AND status='queued'",
        params![job_id, std::process::id(), now()],
    )?;
    if changed == 0 {
        bail!("resource_job_state_changed: 本地资源任务状态已变化")
    }

    let total = specs.iter().map(|spec| spec.size).sum::<u64>();
    let mut completed = 0_u64;
    let mut activations = Vec::new();
    let mut speaker_assets = BTreeMap::new();
    for spec in specs {
        if cancellation_requested(db, job_id)? {
            finish_cancelled(db, job_id)?;
            return Ok(());
        }
        if component_is_active(&config, spec) {
            completed = completed.saturating_add(spec.size);
            update_progress(db, job_id, "verified", completed, total)?;
            continue;
        }
        let asset = download_component(db, job_id, &config.root, spec, completed, total)?;
        update_progress(
            db,
            job_id,
            "installing",
            completed.saturating_add(spec.size),
            total,
        )?;
        if let InstallKind::Speaker(role) = &spec.kind {
            speaker_assets.insert(*role, asset);
        } else {
            let activation = stage_component(&config.root, job_id, spec, &asset)?;
            activations.push(activation);
        }
        completed = completed.saturating_add(spec.size);
    }
    if !speaker_assets.is_empty() {
        activations.push(stage_speaker_package(
            &config.root,
            job_id,
            specs,
            &speaker_assets,
        )?);
    }

    let current = require_config_at(locator)?;
    if !same_path(&current.root, &config.root) {
        bail!("resource_root_changed: 本地资源保存位置已变化，任务未启用")
    }
    config = current;
    for activation in activations {
        config
            .active_versions
            .insert(activation.id, activation.version);
        for (key, path) in activation.entrypoints {
            let relative = path
                .strip_prefix(&config.root)
                .map_err(|_| anyhow!("resource_activation_failed: 本地资源入口超出所选保存位置"))?;
            config
                .active_entrypoints
                .insert(key, relative.to_string_lossy().into_owned());
        }
    }
    enable_capability(&mut config, "basic_media");
    match capability {
        "url_import" => enable_capability(&mut config, "url_import"),
        "local_transcription" => enable_capability(&mut config, "local_transcription"),
        "speaker_identity" => enable_capability(&mut config, "speaker_identity"),
        _ => {}
    }
    config.updated_at = now();
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)
        .context("resource_job_state_changed: 无法锁定本地资源启用步骤")?;
    if cancellation_requested(&tx, job_id)? {
        tx.commit()?;
        finish_cancelled(db, job_id)?;
        return Ok(());
    }
    write_config_at(locator, &config)?;
    let timestamp = now();
    let changed = tx.execute(
        "UPDATE resource_jobs
         SET status='completed',stage='completed',progress=1,bytes_downloaded=?2,
             cancel_requested_at=NULL,worker_pid=NULL,updated_at=?3,completed_at=?3
         WHERE id=?1 AND status='running' AND cancel_requested_at IS NULL",
        params![job_id, total, timestamp],
    )?;
    if changed == 0 {
        bail!("resource_job_state_changed: 本地资源任务在启用前已变化")
    }
    tx.commit()?;
    Ok(())
}

fn enable_capability(config: &mut LocalResourceConfig, capability: &str) {
    if !config
        .enabled_capabilities
        .iter()
        .any(|enabled| enabled == capability)
    {
        config.enabled_capabilities.push(capability.to_owned());
        config.enabled_capabilities.sort();
    }
}

fn asset_extension(spec: &InstallSpec) -> &'static str {
    match &spec.kind {
        InstallKind::ZipPackage => "zip",
        InstallKind::Executable => "exe",
        InstallKind::Model { .. } => "bin",
        InstallKind::Speaker(SpeakerAssetKind::Runtime | SpeakerAssetKind::Segmentation) => {
            "tar.bz2"
        }
        InstallKind::Speaker(SpeakerAssetKind::Embedding) => "onnx",
    }
}

fn partial_path(root: &Path, spec: &InstallSpec) -> PathBuf {
    root.join("downloads").join(format!(
        "{}-{}.{}.part",
        spec.id,
        spec.version,
        asset_extension(spec)
    ))
}

fn asset_path(root: &Path, spec: &InstallSpec) -> PathBuf {
    root.join("downloads").join(format!(
        "{}-{}.{}",
        spec.id,
        spec.version,
        asset_extension(spec)
    ))
}

fn collect_candidate_files(directory: &Path, max_depth: usize, output: &mut Vec<PathBuf>) {
    if max_depth == 0 || output.len() >= 4096 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if output.len() >= 4096 {
            break;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_file() {
            output.push(entry.path());
        } else if file_type.is_dir() {
            collect_candidate_files(&entry.path(), max_depth - 1, output);
        }
    }
}

fn legacy_candidate_files(managed_root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut roots = vec![
        db::home_dir().join("bin"),
        db::home_dir().join("models"),
        db::home_dir().join("downloads"),
        db::home_dir().join("speaker"),
    ];
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        let legacy = local_app_data.join("SiaoCut");
        roots.extend([
            legacy.join("bin"),
            legacy.join("models"),
            legacy.join("downloads"),
            legacy.join("speaker"),
        ]);
    }
    for root in roots {
        if !same_path(&root, managed_root) && !root.starts_with(managed_root) {
            collect_candidate_files(&root, 5, &mut files);
        }
    }
    for variable in [
        "SIAOCUT_FFMPEG",
        "SIAOCUT_FFPROBE",
        "SIAOCUT_WHISPER_CLI",
        "SIAOCUT_YTDLP",
        "SIAOCUT_WHISPER_MODEL",
        "SIAOCUT_SPEAKER_PACKAGE_DIR",
    ] {
        let Some(path) = env::var_os(variable).map(PathBuf::from) else {
            continue;
        };
        if path.is_file() {
            files.push(path);
        } else if path.is_dir() && !path.starts_with(managed_root) {
            collect_candidate_files(&path, 4, &mut files);
        }
    }
    files.sort();
    files.dedup();
    files
}

fn adopt_verified_assets(
    root: &Path,
    specs: &[InstallSpec],
    candidates: &[PathBuf],
) -> Result<(u64, u64)> {
    fs::create_dir_all(root.join("downloads"))?;
    let mut adopted_files = 0_u64;
    let mut adopted_bytes = 0_u64;
    for spec in specs {
        let target = asset_path(root, spec);
        if target.is_file()
            && fs::metadata(&target)?.len() == spec.size
            && hash_file(&target)? == spec.sha256
        {
            continue;
        }
        let Some(candidate) = candidates.iter().find(|candidate| {
            fs::metadata(candidate)
                .map(|metadata| metadata.is_file() && metadata.len() == spec.size)
                .unwrap_or(false)
                && hash_file(candidate)
                    .map(|hash| hash.eq_ignore_ascii_case(&spec.sha256))
                    .unwrap_or(false)
        }) else {
            continue;
        };
        let partial = target.with_extension(format!("{}.adopting", asset_extension(spec)));
        if partial.exists() {
            fs::remove_file(&partial)?;
        }
        fs::copy(candidate, &partial).context("resource_adoption_failed: 无法复用已有本地资源")?;
        if fs::metadata(&partial)?.len() != spec.size
            || !hash_file(&partial)?.eq_ignore_ascii_case(&spec.sha256)
        {
            let _ = fs::remove_file(&partial);
            bail!("resource_adoption_failed: 已有本地资源复制后校验失败")
        }
        if target.exists() {
            fs::remove_file(&target)?;
        }
        fs::rename(&partial, &target)
            .context("resource_adoption_failed: 无法启用已复用的本地资源")?;
        adopted_files += 1;
        adopted_bytes = adopted_bytes.saturating_add(spec.size);
    }
    Ok((adopted_files, adopted_bytes))
}

fn verified_download_bytes(root: &Path, spec: &InstallSpec) -> u64 {
    let target = asset_path(root, spec);
    if fs::metadata(&target)
        .map(|metadata| metadata.len() == spec.size)
        .unwrap_or(false)
        && hash_file(&target)
            .map(|hash| hash.eq_ignore_ascii_case(&spec.sha256))
            .unwrap_or(false)
    {
        spec.size
    } else {
        fs::metadata(partial_path(root, spec))
            .map(|metadata| metadata.len().min(spec.size))
            .unwrap_or(0)
    }
}

fn download_component(
    db: &Connection,
    job_id: &str,
    root: &Path,
    spec: &InstallSpec,
    completed_before: u64,
    total: u64,
) -> Result<PathBuf> {
    let target = asset_path(root, spec);
    if target.is_file() {
        if fs::metadata(&target)?.len() == spec.size && hash_file(&target)? == spec.sha256 {
            update_progress(
                db,
                job_id,
                "verified",
                completed_before.saturating_add(spec.size),
                total,
            )?;
            return Ok(target);
        }
        fs::remove_file(&target).context("resource_hash_mismatch: 无法清理未通过校验的下载")?;
    }

    let partial = partial_path(root, spec);
    let mut existing = fs::metadata(&partial)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if existing > spec.size {
        fs::remove_file(&partial)?;
        existing = 0;
    }
    if existing == spec.size {
        if hash_file(&partial)? == spec.sha256 {
            fs::rename(&partial, &target)?;
            return Ok(target);
        }
        fs::remove_file(&partial)?;
        bail!("resource_hash_mismatch: 下载文件校验失败，已删除无效文件")
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("resource_download_failed: 无法初始化下载连接")?;
    let mut request = client.get(&spec.url);
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let mut response = request
        .send()
        .context("resource_download_failed: 无法连接资源下载服务")?;
    if !response.status().is_success() {
        bail!(
            "resource_download_failed: 资源下载失败：HTTP {}",
            response.status()
        )
    }
    let append = existing > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
    if existing > 0 && !append {
        existing = 0;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&partial)
        .context("resource_root_not_writable: 无法写入本地资源下载文件")?;
    let mut downloaded = existing;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut last_update = Instant::now() - Duration::from_secs(1);
    let chunk_delay = env::var("SIAOCUT_RESOURCE_CHUNK_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());
    loop {
        let count = response
            .read(&mut buffer)
            .context("resource_download_failed: 下载连接意外中断")?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])?;
        downloaded = downloaded.saturating_add(count as u64);
        if let Some(delay) = chunk_delay {
            thread::sleep(Duration::from_millis(delay));
        }
        if last_update.elapsed() >= Duration::from_millis(400) {
            if cancellation_requested(db, job_id)? {
                file.flush()?;
                finish_cancelled(db, job_id)?;
                return Err(anyhow!("resource_cancelled: 本地资源任务已取消"));
            }
            update_progress(
                db,
                job_id,
                "downloading",
                completed_before.saturating_add(downloaded),
                total,
            )?;
            last_update = Instant::now();
        }
    }
    file.flush()?;
    drop(file);
    let actual_size = fs::metadata(&partial)?.len();
    if actual_size != spec.size {
        bail!(
            "resource_download_failed: 下载不完整；应为 {} 字节，实际为 {} 字节，可继续下载",
            spec.size,
            actual_size
        )
    }
    if hash_file(&partial)? != spec.sha256 {
        fs::remove_file(&partial)?;
        bail!("resource_hash_mismatch: 下载文件校验失败，已删除无效文件")
    }
    fs::rename(&partial, &target)?;
    update_progress(
        db,
        job_id,
        "verified",
        completed_before.saturating_add(spec.size),
        total,
    )?;
    Ok(target)
}

fn stage_component(
    root: &Path,
    job_id: &str,
    spec: &InstallSpec,
    asset: &Path,
) -> Result<ComponentActivation> {
    let staging = root.join("staging").join(job_id).join(&spec.id);
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .context("resource_activation_failed: 无法清理未完成的暂存目录")?;
    }
    fs::create_dir_all(&staging).context("resource_activation_failed: 无法创建本地资源暂存目录")?;
    match &spec.kind {
        InstallKind::ZipPackage => extract_zip_safely(asset, &staging)?,
        InstallKind::Executable => {
            fs::copy(asset, staging.join("yt-dlp.exe"))
                .context("resource_activation_failed: 无法暂存 URL 导入资源")?;
        }
        InstallKind::Model { file_name } => {
            fs::copy(asset, staging.join(file_name))
                .context("resource_activation_failed: 无法暂存本地转录模型")?;
        }
        InstallKind::Speaker(_) => {
            bail!("resource_catalog_invalid: 说话人资源必须作为完整资源包启用")
        }
    }

    let final_dir = if matches!(&spec.kind, InstallKind::Model { .. }) {
        root.join("models")
            .join(&spec.activation_id)
            .join(&spec.version)
    } else {
        root.join("packages")
            .join(&spec.activation_id)
            .join(&spec.version)
    };
    if final_dir.is_dir() {
        fs::remove_dir_all(&final_dir)
            .context("resource_activation_failed: 无法替换未启用的本地资源版本")?;
    }
    if let Some(parent) = final_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&staging, &final_dir)
        .context("resource_activation_failed: 无法原子启用本地资源版本")?;

    let mut entrypoints = BTreeMap::new();
    match spec.activation_id.as_str() {
        "ffmpeg-cpu" => {
            entrypoints.insert(
                "ffmpeg".into(),
                find_file(&final_dir, "ffmpeg.exe")
                    .ok_or_else(|| anyhow!("resource_archive_invalid: 媒体资源包缺少必要程序"))?,
            );
            entrypoints.insert(
                "ffprobe".into(),
                find_file(&final_dir, "ffprobe.exe")
                    .ok_or_else(|| anyhow!("resource_archive_invalid: 媒体资源包缺少必要程序"))?,
            );
        }
        "yt-dlp" => {
            let path = final_dir.join("yt-dlp.exe");
            if !path.is_file() {
                bail!("resource_activation_failed: URL 导入资源未正确安装")
            }
            entrypoints.insert("yt_dlp".into(), path);
        }
        "whisper-cpu-upstream" => {
            entrypoints.insert(
                "whisper".into(),
                find_file(&final_dir, "whisper-cli.exe").ok_or_else(|| {
                    anyhow!("resource_archive_invalid: 本地转录资源包缺少必要程序")
                })?,
            );
        }
        "transcription-model" => {
            let InstallKind::Model { file_name } = &spec.kind else {
                bail!("resource_catalog_invalid: 本地转录模型类型无效")
            };
            let path = final_dir.join(file_name);
            if !path.is_file() {
                bail!("resource_activation_failed: 本地转录模型未正确安装")
            }
            entrypoints.insert("default_model".into(), path);
        }
        _ => bail!("resource_catalog_invalid: 不支持的本地资源组件"),
    }
    Ok(ComponentActivation {
        id: spec.activation_id.clone(),
        version: spec.version.clone(),
        entrypoints,
    })
}

fn stage_speaker_package(
    root: &Path,
    job_id: &str,
    specs: &[InstallSpec],
    assets: &BTreeMap<SpeakerAssetKind, PathBuf>,
) -> Result<ComponentActivation> {
    let version = specs
        .iter()
        .find(|spec| spec.activation_id == "speaker_identity")
        .map(|spec| spec.version.clone())
        .ok_or_else(|| anyhow!("resource_catalog_invalid: 说话人资源版本缺失"))?;
    let runtime = assets
        .get(&SpeakerAssetKind::Runtime)
        .ok_or_else(|| anyhow!("resource_catalog_invalid: 说话人运行资源缺失"))?;
    let segmentation = assets
        .get(&SpeakerAssetKind::Segmentation)
        .ok_or_else(|| anyhow!("resource_catalog_invalid: 说话人分段资源缺失"))?;
    let embedding = assets
        .get(&SpeakerAssetKind::Embedding)
        .ok_or_else(|| anyhow!("resource_catalog_invalid: 说话人特征资源缺失"))?;
    let staging = root.join("staging").join(job_id).join("speaker_identity");
    speaker::prepare_package_at(runtime, segmentation, embedding, &staging)?;
    let final_dir = root
        .join("packages")
        .join("speaker_identity")
        .join(&version);
    if final_dir.is_dir() {
        fs::remove_dir_all(&final_dir)
            .context("resource_activation_failed: 无法替换未启用的说话人资源")?;
    }
    if let Some(parent) = final_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&staging, &final_dir)
        .context("resource_activation_failed: 无法原子启用说话人资源")?;
    let executable = speaker::package_executable_at(&final_dir)?;
    Ok(ComponentActivation {
        id: "speaker_identity".into(),
        version,
        entrypoints: BTreeMap::from([("speaker".into(), executable)]),
    })
}

fn extract_zip_safely(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive_path).context("resource_archive_invalid: 无法读取媒体资源包")?;
    let mut archive =
        ZipArchive::new(file).context("resource_archive_invalid: 媒体资源包不是有效的 ZIP 文件")?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .context("resource_archive_invalid: 无法读取媒体资源包内容")?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("resource_archive_invalid: 媒体资源包包含不安全路径"))?;
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut target = File::create(&output)?;
        std::io::copy(&mut entry, &mut target)?;
        target.flush()?;
    }
    Ok(())
}

fn find_file(directory: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|value| value == name) {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = find_file(&path, name)
        {
            return Some(found);
        }
    }
    None
}

fn cancellation_requested(db: &Connection, job_id: &str) -> Result<bool> {
    db.query_row(
        "SELECT cancel_requested_at IS NOT NULL FROM resource_jobs WHERE id=?1",
        [job_id],
        |row| row.get(0),
    )
    .context("resource_job_not_found: 本地资源任务不存在")
}

fn update_progress(
    db: &Connection,
    job_id: &str,
    stage: &str,
    bytes: u64,
    total: u64,
) -> Result<()> {
    let progress = if total == 0 {
        0.0
    } else {
        (bytes as f64 / total as f64).clamp(0.0, 0.99)
    };
    let changed = db.execute(
        "UPDATE resource_jobs
         SET stage=?2,bytes_downloaded=MAX(bytes_downloaded,?3),
             progress=MAX(progress,?4),updated_at=?5
         WHERE id=?1 AND status='running' AND cancel_requested_at IS NULL",
        params![job_id, stage, bytes.min(total), progress, now()],
    )?;
    if changed == 0 {
        bail!("resource_cancelled: 本地资源任务已取消")
    }
    Ok(())
}

fn finish_cancelled(db: &Connection, job_id: &str) -> Result<()> {
    let timestamp = now();
    db.execute(
        "UPDATE resource_jobs
         SET status='cancelled',stage='cancelled',worker_pid=NULL,updated_at=?2,completed_at=?2
         WHERE id=?1 AND status IN ('queued','running')",
        params![job_id, timestamp],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        io::Cursor,
        net::{TcpListener, TcpStream},
        sync::{Arc, Mutex},
    };
    use tempfile::tempdir;

    #[derive(Clone)]
    struct ResponsePlan {
        payload: Vec<u8>,
        max_bytes: Option<usize>,
    }

    struct MockResourceServer {
        base_url: String,
        ranges: Arc<Mutex<Vec<u64>>>,
        thread: thread::JoinHandle<()>,
    }

    impl MockResourceServer {
        fn start(plans: Vec<ResponsePlan>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let ranges = Arc::new(Mutex::new(Vec::new()));
            let observed = Arc::clone(&ranges);
            let thread = thread::spawn(move || {
                for plan in plans {
                    let (mut stream, _) = listener.accept().unwrap();
                    let range = read_request_range(&mut stream);
                    observed.lock().unwrap().push(range);
                    write_response(&mut stream, &plan.payload, range, plan.max_bytes);
                }
            });
            Self {
                base_url,
                ranges,
                thread,
            }
        }

        fn finish(self) -> Vec<u64> {
            self.thread.join().unwrap();
            self.ranges.lock().unwrap().clone()
        }
    }

    fn read_request_range(stream: &mut TcpStream) -> u64 {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
        }
        String::from_utf8_lossy(&request)
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("range: bytes=")
                    .and_then(|value| value.split('-').next())
                    .and_then(|value| value.parse().ok())
            })
            .unwrap_or(0)
    }

    fn write_response(
        stream: &mut TcpStream,
        payload: &[u8],
        range: u64,
        max_bytes: Option<usize>,
    ) {
        let start = usize::try_from(range).unwrap();
        let remaining = payload.len() - start;
        let status = if start > 0 {
            "HTTP/1.1 206 Partial Content"
        } else {
            "HTTP/1.1 200 OK"
        };
        let content_range = if start > 0 {
            format!(
                "Content-Range: bytes {start}-{}/{}\r\n",
                payload.len() - 1,
                payload.len()
            )
        } else {
            String::new()
        };
        write!(
            stream,
            "{status}\r\nContent-Length: {remaining}\r\nAccept-Ranges: bytes\r\n{content_range}Connection: close\r\n\r\n"
        )
        .unwrap();
        let count = max_bytes.unwrap_or(remaining).min(remaining);
        stream.write_all(&payload[start..start + count]).unwrap();
        stream.flush().unwrap();
    }

    fn sha256(payload: &[u8]) -> String {
        format!("{:x}", Sha256::digest(payload))
    }

    fn write_test_config(locator: &Path, root: &Path) {
        for directory in ["packages", "models", "downloads", "staging"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        write_config_at(
            locator,
            &LocalResourceConfig {
                schema_version: 1,
                root: root.canonicalize().unwrap(),
                transcription_profile: "standard".into(),
                enabled_capabilities: Vec::new(),
                active_entrypoints: BTreeMap::new(),
                active_versions: BTreeMap::new(),
                updated_at: now(),
            },
        )
        .unwrap();
    }

    fn create_test_job(
        db: &Connection,
        capability: &str,
        locator: &Path,
        specs: &[InstallSpec],
    ) -> ResourceJob {
        create_install_in(db, capability, locator, specs, |_, _| Ok(())).unwrap()
    }

    fn test_zip() -> Vec<u8> {
        use zip::{ZipWriter, write::SimpleFileOptions};

        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("bundle/bin/ffmpeg.exe", b"ffmpeg".as_slice()),
            ("bundle/bin/ffprobe.exe", b"ffprobe".as_slice()),
            ("bundle/bin/avcodec.dll", b"library".as_slice()),
        ] {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn test_whisper_zip() -> Vec<u8> {
        use zip::{ZipWriter, write::SimpleFileOptions};

        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("whisper-bin-x64/whisper-cli.exe", b"whisper".as_slice()),
            ("whisper-bin-x64/ggml.dll", b"library".as_slice()),
        ] {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn install_requires_an_explicitly_configured_location() {
        let temp = tempdir().unwrap();
        let db = db::open_at(&temp.path().join("siaocut.db")).unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let unused_root = temp.path().join("must-not-exist");
        let specs = vec![InstallSpec {
            id: "yt-dlp".into(),
            activation_id: "yt-dlp".into(),
            version: "test".into(),
            url: "http://127.0.0.1/unused".into(),
            size: 1,
            sha256: "00".repeat(32),
            kind: InstallKind::Executable,
        }];

        let error = create_install_in(&db, "url_import", &locator, &specs, |_, _| {
            panic!("worker must not start without a confirmed directory")
        })
        .unwrap_err();

        assert!(error.to_string().starts_with("resource_setup_required:"));
        assert!(!locator.exists());
        assert!(!unused_root.exists());
        let jobs: i64 = db
            .query_row("SELECT COUNT(*) FROM resource_jobs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(jobs, 0);
    }

    #[test]
    fn url_import_includes_the_shared_media_component() {
        let specs = specs_for_capability("url_import", "standard").unwrap();
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.id.as_str())
                .collect::<Vec<_>>(),
            ["ffmpeg-cpu", "yt-dlp"]
        );
    }

    #[test]
    fn transcription_profiles_resolve_to_a_cpu_runtime_and_auditable_model_sizes() {
        let fast = specs_for_capability("local_transcription", "fast").unwrap();
        let standard = specs_for_capability("local_transcription", "standard").unwrap();
        let quality = specs_for_capability("local_transcription", "quality").unwrap();

        for specs in [&fast, &standard, &quality] {
            assert_eq!(specs[0].id, "ffmpeg-cpu");
            assert_eq!(specs[1].id, "whisper-cpu-upstream");
            assert_eq!(specs[2].activation_id, "transcription-model");
            assert!(matches!(specs[2].kind, InstallKind::Model { .. }));
        }
        assert_eq!(fast[2].id, "transcription-model-tiny");
        assert_eq!(fast[2].size, 77_691_713);
        assert_eq!(standard[2].id, "transcription-model-base");
        assert_eq!(standard[2].size, 147_951_465);
        assert_eq!(quality[2].id, "transcription-model-small");
        assert_eq!(quality[2].size, 487_601_967);
        assert!(
            fast.iter().map(|spec| spec.size).sum::<u64>()
                < standard.iter().map(|spec| spec.size).sum::<u64>()
        );
        assert!(
            standard.iter().map(|spec| spec.size).sum::<u64>()
                < quality.iter().map(|spec| spec.size).sum::<u64>()
        );
    }

    #[test]
    fn speaker_preparation_is_independent_but_reuses_basic_media() {
        let specs = specs_for_capability("speaker_identity", "standard").unwrap();
        assert_eq!(specs[0].id, "ffmpeg-cpu");
        assert_eq!(specs.len(), 4);
        assert!(
            specs[1..]
                .iter()
                .all(|spec| spec.activation_id == "speaker_identity")
        );
        assert_eq!(
            specs[1..].iter().map(|spec| spec.size).sum::<u64>(),
            64_389_270
        );
    }

    #[test]
    fn shared_media_cannot_be_removed_while_another_capability_needs_it() {
        let temp = tempdir().unwrap();
        let mut config = LocalResourceConfig {
            schema_version: 1,
            root: temp.path().to_path_buf(),
            transcription_profile: "standard".into(),
            enabled_capabilities: vec!["basic_media".into(), "local_transcription".into()],
            active_entrypoints: BTreeMap::new(),
            active_versions: BTreeMap::new(),
            updated_at: now(),
        };

        assert!(
            components_to_remove(&config, "basic_media")
                .unwrap_err()
                .to_string()
                .starts_with("resource_dependency_required:")
        );
        assert_eq!(
            components_to_remove(&config, "local_transcription").unwrap(),
            ["whisper-cpu-upstream", "transcription-model"]
        );
        config
            .enabled_capabilities
            .retain(|capability| capability == "basic_media");
        assert_eq!(
            components_to_remove(&config, "basic_media").unwrap(),
            ["ffmpeg-cpu"]
        );
    }

    #[test]
    fn local_transcription_activates_cpu_runtime_and_selected_profile_atomically() {
        let ffmpeg = test_zip();
        let whisper = test_whisper_zip();
        let model = b"standard profile model".to_vec();
        let server = MockResourceServer::start(vec![
            ResponsePlan {
                payload: ffmpeg.clone(),
                max_bytes: None,
            },
            ResponsePlan {
                payload: whisper.clone(),
                max_bytes: None,
            },
            ResponsePlan {
                payload: model.clone(),
                max_bytes: None,
            },
        ]);
        let temp = tempdir().unwrap();
        let db = db::open_at(&temp.path().join("siaocut.db")).unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let root = temp.path().join("resources");
        write_test_config(&locator, &root);
        let specs = vec![
            InstallSpec {
                id: "ffmpeg-cpu".into(),
                activation_id: "ffmpeg-cpu".into(),
                version: "test-media".into(),
                url: format!("{}/ffmpeg.zip", server.base_url),
                size: ffmpeg.len() as u64,
                sha256: sha256(&ffmpeg),
                kind: InstallKind::ZipPackage,
            },
            InstallSpec {
                id: "whisper-cpu-upstream".into(),
                activation_id: "whisper-cpu-upstream".into(),
                version: "test-cpu".into(),
                url: format!("{}/whisper.zip", server.base_url),
                size: whisper.len() as u64,
                sha256: sha256(&whisper),
                kind: InstallKind::ZipPackage,
            },
            InstallSpec {
                id: "transcription-model-base".into(),
                activation_id: "transcription-model".into(),
                version: "base-test".into(),
                url: format!("{}/model.bin", server.base_url),
                size: model.len() as u64,
                sha256: sha256(&model),
                kind: InstallKind::Model {
                    file_name: "ggml-base.bin".into(),
                },
            },
        ];
        let job = create_test_job(&db, "local_transcription", &locator, &specs);

        run_install_attempt_in(&db, &job.id, "local_transcription", &locator, &specs).unwrap();

        assert_eq!(server.finish(), vec![0, 0, 0]);
        let config = read_config_at(&locator).unwrap().unwrap();
        for key in ["ffmpeg", "ffprobe", "whisper", "default_model"] {
            let relative = config.active_entrypoints.get(key).unwrap();
            assert!(config.root.join(relative).is_file(), "missing {key}");
        }
        assert!(!config.active_entrypoints.contains_key("whisper_vad_model"));
        assert_eq!(config.active_versions["transcription-model"], "base-test");
        assert!(
            config
                .enabled_capabilities
                .contains(&"local_transcription".into())
        );
        assert_eq!(load_job(&db, &job.id).unwrap().status, "completed");
    }

    #[test]
    fn zip_extraction_rejects_parent_traversal() {
        use zip::{ZipWriter, write::SimpleFileOptions};

        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("unsafe.zip");
        let file = File::create(&archive_path).unwrap();
        let mut writer = ZipWriter::new(file);
        writer
            .start_file("../outside.exe", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"unsafe").unwrap();
        writer.finish().unwrap();

        let destination = temp.path().join("extract");
        fs::create_dir_all(&destination).unwrap();
        let error = extract_zip_safely(&archive_path, &destination).unwrap_err();

        assert!(error.to_string().starts_with("resource_archive_invalid:"));
        assert!(!temp.path().join("outside.exe").exists());
    }

    #[test]
    fn interrupted_download_resumes_with_an_http_range() {
        let payload = vec![42_u8; 512 * 1024];
        let server = MockResourceServer::start(vec![
            ResponsePlan {
                payload: payload.clone(),
                max_bytes: Some(128 * 1024),
            },
            ResponsePlan {
                payload: payload.clone(),
                max_bytes: None,
            },
        ]);
        let temp = tempdir().unwrap();
        let db = db::open_at(&temp.path().join("siaocut.db")).unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let root = temp.path().join("resources");
        write_test_config(&locator, &root);
        let spec = InstallSpec {
            id: "yt-dlp".into(),
            activation_id: "yt-dlp".into(),
            version: "test".into(),
            url: format!("{}/yt-dlp.exe", server.base_url),
            size: payload.len() as u64,
            sha256: sha256(&payload),
            kind: InstallKind::Executable,
        };
        let job = create_test_job(&db, "url_import", &locator, std::slice::from_ref(&spec));

        assert!(
            run_install_attempt_in(
                &db,
                &job.id,
                "url_import",
                &locator,
                std::slice::from_ref(&spec)
            )
            .is_err()
        );
        assert_eq!(load_job(&db, &job.id).unwrap().status, "failed");
        let partial_bytes = fs::metadata(partial_path(&root, &spec)).unwrap().len();
        assert!(partial_bytes > 0 && partial_bytes < spec.size);

        resume_in(&db, &job.id, &locator, |_, _| Ok(())).unwrap();
        run_install_attempt_in(
            &db,
            &job.id,
            "url_import",
            &locator,
            std::slice::from_ref(&spec),
        )
        .unwrap();

        let ranges = server.finish();
        assert_eq!(ranges[0], 0);
        assert_eq!(ranges[1], partial_bytes);
        assert_eq!(load_job(&db, &job.id).unwrap().status, "completed");
        let config = read_config_at(&locator).unwrap().unwrap();
        assert!(config.active_entrypoints.contains_key("yt_dlp"));
        assert!(config.enabled_capabilities.contains(&"url_import".into()));
    }

    #[test]
    fn corrupt_download_never_becomes_active() {
        let payload = b"corrupt payload".to_vec();
        let server = MockResourceServer::start(vec![ResponsePlan {
            payload: payload.clone(),
            max_bytes: None,
        }]);
        let temp = tempdir().unwrap();
        let db = db::open_at(&temp.path().join("siaocut.db")).unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let root = temp.path().join("resources");
        write_test_config(&locator, &root);
        let spec = InstallSpec {
            id: "yt-dlp".into(),
            activation_id: "yt-dlp".into(),
            version: "test".into(),
            url: format!("{}/yt-dlp.exe", server.base_url),
            size: payload.len() as u64,
            sha256: "00".repeat(32),
            kind: InstallKind::Executable,
        };
        let job = create_test_job(&db, "url_import", &locator, std::slice::from_ref(&spec));

        let error = run_install_attempt_in(
            &db,
            &job.id,
            "url_import",
            &locator,
            std::slice::from_ref(&spec),
        )
        .unwrap_err();

        server.finish();
        assert!(error.to_string().starts_with("resource_hash_mismatch:"));
        assert!(!partial_path(&root, &spec).exists());
        assert!(!asset_path(&root, &spec).exists());
        let config = read_config_at(&locator).unwrap().unwrap();
        assert!(config.active_entrypoints.is_empty());
        assert!(config.active_versions.is_empty());
    }

    #[test]
    fn url_import_activates_media_and_url_resources_together() {
        let ffmpeg = test_zip();
        let yt_dlp = b"yt-dlp executable".to_vec();
        let server = MockResourceServer::start(vec![
            ResponsePlan {
                payload: ffmpeg.clone(),
                max_bytes: None,
            },
            ResponsePlan {
                payload: yt_dlp.clone(),
                max_bytes: None,
            },
        ]);
        let temp = tempdir().unwrap();
        let db = db::open_at(&temp.path().join("siaocut.db")).unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let root = temp.path().join("resources");
        write_test_config(&locator, &root);
        let specs = vec![
            InstallSpec {
                id: "ffmpeg-cpu".into(),
                activation_id: "ffmpeg-cpu".into(),
                version: "test-media".into(),
                url: format!("{}/ffmpeg.zip", server.base_url),
                size: ffmpeg.len() as u64,
                sha256: sha256(&ffmpeg),
                kind: InstallKind::ZipPackage,
            },
            InstallSpec {
                id: "yt-dlp".into(),
                activation_id: "yt-dlp".into(),
                version: "test-url".into(),
                url: format!("{}/yt-dlp.exe", server.base_url),
                size: yt_dlp.len() as u64,
                sha256: sha256(&yt_dlp),
                kind: InstallKind::Executable,
            },
        ];
        let job = create_test_job(&db, "url_import", &locator, &specs);

        run_install_attempt_in(&db, &job.id, "url_import", &locator, &specs).unwrap();

        server.finish();
        let config = read_config_at(&locator).unwrap().unwrap();
        for key in ["ffmpeg", "ffprobe", "yt_dlp"] {
            let relative = config.active_entrypoints.get(key).unwrap();
            assert!(config.root.join(relative).is_file(), "missing {key}");
        }
        assert_eq!(config.active_versions["ffmpeg-cpu"], "test-media");
        assert_eq!(config.active_versions["yt-dlp"], "test-url");
        assert!(config.enabled_capabilities.contains(&"basic_media".into()));
        assert!(config.enabled_capabilities.contains(&"url_import".into()));
        assert_eq!(load_job(&db, &job.id).unwrap().status, "completed");
    }

    #[test]
    fn adopts_a_verified_legacy_asset_without_removing_the_original() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("resources");
        let legacy = temp.path().join("legacy");
        fs::create_dir_all(&legacy).unwrap();
        let payload = b"verified legacy model";
        let valid = legacy.join("ggml-base.bin");
        let corrupt = legacy.join("corrupt.bin");
        fs::write(&valid, payload).unwrap();
        fs::write(&corrupt, vec![b'x'; payload.len()]).unwrap();
        let spec = InstallSpec {
            id: "transcription-model-base".into(),
            activation_id: "transcription-model".into(),
            version: "base-test".into(),
            url: "http://127.0.0.1/unused".into(),
            size: payload.len() as u64,
            sha256: sha256(payload),
            kind: InstallKind::Model {
                file_name: "ggml-base.bin".into(),
            },
        };

        let adopted = adopt_verified_assets(
            &root,
            std::slice::from_ref(&spec),
            &[corrupt, valid.clone()],
        )
        .unwrap();

        assert_eq!(adopted, (1, payload.len() as u64));
        assert!(valid.is_file());
        assert_eq!(
            hash_file(&asset_path(&root, &spec)).unwrap(),
            sha256(payload)
        );
        assert_eq!(verified_download_bytes(&root, &spec), spec.size);
        let locator = temp.path().join("config/local-resources.json");
        let database = db::open_at(&temp.path().join("siaocut.db")).unwrap();
        write_test_config(&locator, &root);
        let job = create_test_job(
            &database,
            "local_transcription",
            &locator,
            std::slice::from_ref(&spec),
        );
        assert_eq!(job.bytes_downloaded, job.total_bytes);
        assert_eq!(job.stage, "verified");
    }

    #[test]
    fn cancellation_before_worker_start_preserves_download_state() {
        let temp = tempdir().unwrap();
        let db = db::open_at(&temp.path().join("siaocut.db")).unwrap();
        let locator = temp.path().join("config/local-resources.json");
        let root = temp.path().join("resources");
        write_test_config(&locator, &root);
        let spec = InstallSpec {
            id: "yt-dlp".into(),
            activation_id: "yt-dlp".into(),
            version: "test".into(),
            url: "http://127.0.0.1/unused".into(),
            size: 1,
            sha256: "00".repeat(32),
            kind: InstallKind::Executable,
        };
        let job = create_test_job(&db, "url_import", &locator, std::slice::from_ref(&spec));

        cancel(&db, &job.id).unwrap();
        install_in(
            &db,
            &job.id,
            "url_import",
            &locator,
            std::slice::from_ref(&spec),
        )
        .unwrap();

        assert_eq!(load_job(&db, &job.id).unwrap().status, "cancelled");
        assert!(!partial_path(&root, &spec).exists());
    }
}
