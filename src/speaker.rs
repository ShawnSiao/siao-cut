use crate::{
    db,
    media::{hash_file, tool_path},
    project,
    util::{hidden_command, new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use bzip2::read::BzDecoder;
use reqwest::{StatusCode, blocking::Client, header::RANGE};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tar::Archive;

pub const PACKAGE_ID: &str = "sherpa-onnx-speaker-zh-en-v1";
pub const RUNTIME_VERSION: &str = "sherpa-onnx 1.13.2";
pub const SEGMENTATION_MODEL: &str = "pyannote segmentation 3.0 int8";
pub const EMBEDDING_MODEL: &str = "3D-Speaker ERes2Net Base 16 kHz";
const PACKAGE_SOURCE: &str = "https://github.com/k2-fsa/sherpa-onnx";
const RUNTIME_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.2/sherpa-onnx-v1.13.2-win-x64-shared-MD-Release-no-tts.tar.bz2";
const SEGMENTATION_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2";
const EMBEDDING_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx";
const RUNTIME_ARCHIVE_SIZE: u64 = 17_837_065;
const SEGMENTATION_ARCHIVE_SIZE: u64 = 6_958_444;
const EMBEDDING_SIZE: u64 = 39_593_761;
const DOWNLOAD_SIZE: u64 = RUNTIME_ARCHIVE_SIZE + SEGMENTATION_ARCHIVE_SIZE + EMBEDDING_SIZE;

#[derive(Clone, Copy)]
struct DownloadAsset {
    file_name: &'static str,
    url: &'static str,
    size: u64,
    sha256: &'static str,
}

const DOWNLOADS: &[DownloadAsset] = &[
    DownloadAsset {
        file_name: "sherpa-onnx-v1.13.2-win-x64.tar.bz2",
        url: RUNTIME_ARCHIVE_URL,
        size: RUNTIME_ARCHIVE_SIZE,
        sha256: "d74ad2c3e2f943e51ed8b15d409281dea378fcb21f7bb83e8b070be03f2f6715",
    },
    DownloadAsset {
        file_name: "pyannote-segmentation-3.0.tar.bz2",
        url: SEGMENTATION_ARCHIVE_URL,
        size: SEGMENTATION_ARCHIVE_SIZE,
        sha256: "24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488",
    },
    DownloadAsset {
        file_name: "3dspeaker-eres2net-base-16k.onnx",
        url: EMBEDDING_URL,
        size: EMBEDDING_SIZE,
        sha256: "1a331345f04805badbb495c775a6ddffcdd1a732567d5ec8b3d5749e3c7a5e4b",
    },
];

#[derive(Clone, Copy)]
struct InstalledAssetSpec {
    id: &'static str,
    name: &'static str,
    relative_path: &'static str,
    source: &'static str,
    license: &'static str,
    size: u64,
    sha256: &'static str,
}

const INSTALLED_ASSETS: &[InstalledAssetSpec] = &[
    InstalledAssetSpec {
        id: "runtime",
        name: "sherpa-onnx Windows x64 CPU",
        relative_path: "runtime/sherpa-onnx-offline-speaker-diarization.exe",
        source: PACKAGE_SOURCE,
        license: "Apache-2.0",
        size: 323_584,
        sha256: "86d696832204b7859aef601a0f996371abca6f955d71e1242f308027872a0e9c",
    },
    InstalledAssetSpec {
        id: "onnxruntime",
        name: "ONNX Runtime",
        relative_path: "runtime/onnxruntime.dll",
        source: "https://github.com/microsoft/onnxruntime",
        license: "MIT",
        size: 15_394_304,
        sha256: "8b695444d1a35ed0c8338b8c14438b3be5e0a3b222b88b1e7b4ce8753f135b50",
    },
    InstalledAssetSpec {
        id: "onnxruntime-providers",
        name: "ONNX Runtime provider bridge",
        relative_path: "runtime/onnxruntime_providers_shared.dll",
        source: "https://github.com/microsoft/onnxruntime",
        license: "MIT",
        size: 10_752,
        sha256: "ebc55b0f28e8a79cbf78e810a7f510ba70e75a2dfbcfcc6aca31ab2b8710a59a",
    },
    InstalledAssetSpec {
        id: "segmentation",
        name: SEGMENTATION_MODEL,
        relative_path: "models/pyannote-segmentation-3.0-int8.onnx",
        source: "https://huggingface.co/pyannote/segmentation-3.0",
        license: "MIT",
        size: 1_540_506,
        sha256: "d582f4b4c6b48205de7e0643c57df0df5615a3c176189be3fc461e9d18827b5d",
    },
    InstalledAssetSpec {
        id: "embedding",
        name: EMBEDDING_MODEL,
        relative_path: "models/3dspeaker-eres2net-base-16k.onnx",
        source: "https://github.com/modelscope/3D-Speaker",
        license: "Apache-2.0",
        size: EMBEDDING_SIZE,
        sha256: "1a331345f04805badbb495c775a6ddffcdd1a732567d5ec8b3d5749e3c7a5e4b",
    },
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerAssetStatus {
    pub id: String,
    pub name: String,
    pub source: String,
    pub license: String,
    pub size: u64,
    pub sha256: String,
    pub installed: bool,
    pub verified: Option<bool>,
    pub verification_status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerPackageStatus {
    pub id: String,
    pub name: String,
    pub runtime_version: String,
    pub description: String,
    pub source: String,
    pub license: String,
    pub download_size: u64,
    pub installed_size: u64,
    pub installed: bool,
    pub verified: Option<bool>,
    pub verification_status: String,
    pub assets: Vec<SpeakerAssetStatus>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerIdentity {
    pub id: String,
    pub source_label: String,
    pub label: String,
    pub color_index: u32,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerTurn {
    pub id: String,
    pub speaker_id: String,
    pub start: f64,
    pub end: f64,
    pub confidence: Option<f64>,
    pub source: String,
    pub model_version: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentSpeaker {
    pub segment_id: String,
    pub speaker_id: String,
    pub source: String,
    pub confidence: Option<f64>,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerTrack {
    pub status: String,
    pub runtime_version: String,
    pub segmentation_model: String,
    pub embedding_model: String,
    pub generated_at: Option<String>,
    pub speakers: Vec<SpeakerIdentity>,
    pub turns: Vec<SpeakerTurn>,
    pub associations: Vec<SegmentSpeaker>,
}

impl Default for SpeakerTrack {
    fn default() -> Self {
        Self {
            status: "not_analyzed".into(),
            runtime_version: RUNTIME_VERSION.into(),
            segmentation_model: SEGMENTATION_MODEL.into(),
            embedding_model: EMBEDDING_MODEL.into(),
            generated_at: None,
            speakers: Vec::new(),
            turns: Vec::new(),
            associations: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerJob {
    pub id: String,
    pub kind: String,
    pub project_id: Option<String>,
    pub status: String,
    pub stage: String,
    pub progress: f64,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub cancel_requested_at: Option<String>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub worker_pid: Option<u32>,
    pub attempt_count: u32,
}

fn package_dir() -> PathBuf {
    db::home_dir().join("speaker")
}

fn installed_path(spec: InstalledAssetSpec) -> PathBuf {
    package_dir().join(spec.relative_path)
}

fn download_dir() -> PathBuf {
    package_dir().join("downloads")
}

pub fn package_status(verify: bool) -> Result<SpeakerPackageStatus> {
    let mut any_installed = false;
    let mut all_installed = true;
    let mut all_verified = true;
    let mut assets = Vec::new();
    for spec in INSTALLED_ASSETS {
        let path = installed_path(*spec);
        let installed = path.is_file();
        any_installed |= installed;
        all_installed &= installed;
        let verified = if verify && installed {
            Some(fs::metadata(&path)?.len() == spec.size && hash_file(&path)? == spec.sha256)
        } else {
            None
        };
        if verify {
            all_verified &= verified == Some(true);
        }
        assets.push(SpeakerAssetStatus {
            id: spec.id.into(),
            name: spec.name.into(),
            source: spec.source.into(),
            license: spec.license.into(),
            size: spec.size,
            sha256: spec.sha256.into(),
            installed,
            verified,
            verification_status: if !installed {
                "not_installed"
            } else {
                match verified {
                    Some(true) => "verified",
                    Some(false) => "failed",
                    None => "not_checked",
                }
            }
            .to_owned(),
        });
    }
    let verified = if verify && any_installed {
        Some(all_verified)
    } else {
        None
    };
    Ok(SpeakerPackageStatus {
        id: PACKAGE_ID.into(),
        name: "本地说话人分离（中英）".into(),
        runtime_version: RUNTIME_VERSION.into(),
        description: "CPU 本地运行；分析结果只进入待审阅说话人轨，不改写字幕或剪辑。".into(),
        source: PACKAGE_SOURCE.into(),
        license: "Apache-2.0 / MIT".into(),
        download_size: DOWNLOAD_SIZE,
        installed_size: INSTALLED_ASSETS.iter().map(|item| item.size).sum(),
        installed: all_installed,
        verified,
        verification_status: if !all_installed {
            "not_installed"
        } else {
            match verified {
                Some(true) => "verified",
                Some(false) => "failed",
                None => "not_checked",
            }
        }
        .to_owned(),
        assets,
    })
}

pub fn create_install(db: &Connection) -> Result<SpeakerJob> {
    let status = package_status(true)?;
    if status.installed && status.verified == Some(true) {
        bail!("speaker_package_installed: 说话人模型包已经安装并通过校验")
    }
    if let Some(job) = active_job(db, "install", None)? {
        return Ok(job);
    }
    fs::create_dir_all(download_dir())?;
    let available = crate::util::available_space(&package_dir())?;
    let reserve = 128 * 1024 * 1024;
    if available < DOWNLOAD_SIZE.saturating_mul(2).saturating_add(reserve) {
        bail!("disk_space_low: 说话人模型安装至少需要约 260 MB 可用空间")
    }
    let downloaded = downloaded_bytes();
    let job = insert_job(db, "install", None, "等待下载", downloaded, DOWNLOAD_SIZE)?;
    spawn_worker(&job.id)?;
    Ok(job)
}

pub fn create_analysis(db: &Connection, project_id: &str) -> Result<SpeakerJob> {
    let package = package_status(true)?;
    if !package.installed || package.verified != Some(true) {
        bail!("speaker_package_missing: 请先显式安装并校验说话人模型包")
    }
    let loaded = project::load(db, project_id)?;
    let source = Path::new(&loaded.media.source_path)
        .canonicalize()
        .context("无法读取项目关联的原始媒体")?;
    if !source.is_file() {
        bail!("speaker_source_missing: 项目关联的原始媒体不存在")
    }
    if let Some(job) = active_job(db, "analyze", Some(project_id))? {
        return Ok(job);
    }
    let job = insert_job(db, "analyze", Some(project_id), "等待分析", 0, 0)?;
    spawn_worker(&job.id)?;
    Ok(job)
}

fn insert_job(
    db: &Connection,
    kind: &str,
    project_id: Option<&str>,
    stage: &str,
    bytes_downloaded: u64,
    total_bytes: u64,
) -> Result<SpeakerJob> {
    let id = new_id("speaker");
    let timestamp = now();
    db.execute(
        "INSERT INTO speaker_jobs(id,kind,project_id,status,stage,progress,bytes_downloaded,total_bytes,created_at,updated_at,attempt_count) VALUES(?1,?2,?3,'queued',?4,?5,?6,?7,?8,?8,1)",
        params![
            id,
            kind,
            project_id,
            stage,
            if total_bytes == 0 { 0.0 } else { bytes_downloaded as f64 / total_bytes as f64 },
            bytes_downloaded,
            total_bytes,
            timestamp
        ],
    )?;
    load_job(db, &id)
}

fn active_job(db: &Connection, kind: &str, project_id: Option<&str>) -> Result<Option<SpeakerJob>> {
    let id = if let Some(project_id) = project_id {
        db.query_row(
            "SELECT id FROM speaker_jobs WHERE kind=?1 AND project_id=?2 AND status IN ('queued','running') ORDER BY created_at DESC LIMIT 1",
            params![kind, project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    } else {
        db.query_row(
            "SELECT id FROM speaker_jobs WHERE kind=?1 AND project_id IS NULL AND status IN ('queued','running') ORDER BY created_at DESC LIMIT 1",
            [kind],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    };
    id.map(|id| load_job(db, &id)).transpose()
}

pub fn load_job(db: &Connection, job_id: &str) -> Result<SpeakerJob> {
    db.query_row(
        "SELECT id,kind,project_id,status,stage,progress,bytes_downloaded,total_bytes,cancel_requested_at,error_message,created_at,updated_at,completed_at,worker_pid,attempt_count FROM speaker_jobs WHERE id=?1",
        [job_id],
        |row| {
            let status = row.get::<_, String>(3)?;
            let error_message = row.get::<_, Option<String>>(9)?;
            Ok(SpeakerJob {
                id: row.get(0)?,
                kind: row.get(1)?,
                project_id: row.get(2)?,
                error_code: crate::model::background_error_code(
                    &status,
                    error_message.as_deref(),
                ),
                status,
                stage: row.get(4)?,
                progress: row.get(5)?,
                bytes_downloaded: row.get(6)?,
                total_bytes: row.get(7)?,
                cancel_requested_at: row.get(8)?,
                error_message,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
                completed_at: row.get(12)?,
                worker_pid: row.get(13)?,
                attempt_count: row.get::<_, i64>(14)? as u32,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("speaker_job_not_found: 说话人任务不存在：{job_id}"))
}

pub fn list_jobs(db: &Connection) -> Result<Vec<SpeakerJob>> {
    db.prepare("SELECT id FROM speaker_jobs ORDER BY created_at DESC")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| load_job(db, &id))
        .collect()
}

pub fn cancel(db: &Connection, job_id: &str) -> Result<SpeakerJob> {
    let job = load_job(db, job_id)?;
    if !["queued", "running"].contains(&job.status.as_str()) {
        bail!("speaker_job_not_cancellable: 当前说话人任务不能取消")
    }
    let timestamp = now();
    db.execute(
        "UPDATE speaker_jobs SET status='cancelled',cancel_requested_at=?2,worker_pid=NULL,completed_at=?2,updated_at=?2 WHERE id=?1",
        params![job_id, timestamp],
    )?;
    if let Some(pid) = job.worker_pid
        && pid != std::process::id()
        && crate::util::process_is_active(pid)
    {
        let _ = crate::util::terminate_process_tree_by_id(pid);
    }
    load_job(db, job_id)
}

pub fn resume(db: &Connection, job_id: &str) -> Result<SpeakerJob> {
    let job = load_job(db, job_id)?;
    if !["cancelled", "failed", "interrupted"].contains(&job.status.as_str()) {
        bail!("speaker_job_not_resumable: 当前说话人任务不能继续")
    }
    let downloaded = if job.kind == "install" {
        downloaded_bytes()
    } else {
        0
    };
    db.execute(
        "UPDATE speaker_jobs SET status='queued',stage=?2,progress=?3,bytes_downloaded=?4,cancel_requested_at=NULL,error_message=NULL,completed_at=NULL,worker_pid=NULL,attempt_count=attempt_count+1,updated_at=?5 WHERE id=?1",
        params![
            job_id,
            if job.kind == "install" { "等待下载" } else { "等待分析" },
            if job.total_bytes == 0 { 0.0 } else { downloaded as f64 / job.total_bytes as f64 },
            downloaded,
            now()
        ],
    )?;
    spawn_worker(job_id)?;
    load_job(db, job_id)
}

pub fn reconcile_interrupted(db: &Connection) -> Result<()> {
    let rows = db
        .prepare("SELECT id,worker_pid,updated_at FROM speaker_jobs WHERE status IN ('queued','running')")?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<u32>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, pid, updated_at) in rows {
        let stale = chrono::DateTime::parse_from_rfc3339(&updated_at)
            .map(|time| {
                chrono::Utc::now()
                    .signed_duration_since(time.with_timezone(&chrono::Utc))
                    .num_seconds()
                    >= 5
            })
            .unwrap_or(true);
        if stale && !pid.is_some_and(crate::util::process_is_active) {
            db.execute(
                "UPDATE speaker_jobs SET status='interrupted',error_message='上次说话人任务已中断，可以显式继续。',worker_pid=NULL,updated_at=?2 WHERE id=?1",
                params![id, now()],
            )?;
        }
    }
    Ok(())
}

fn spawn_worker(job_id: &str) -> Result<()> {
    crate::util::spawn_detached_current(&["__speaker_worker", job_id])
        .context("无法启动说话人后台任务")
}

pub fn run_worker(job_id: &str) -> Result<()> {
    let mut db = db::open()?;
    let claimed = db.execute(
        "UPDATE speaker_jobs SET status='running',worker_pid=?2,error_message=NULL,updated_at=?3 WHERE id=?1 AND status='queued'",
        params![job_id, std::process::id(), now()],
    )?;
    if claimed == 0 {
        return Ok(());
    }
    let kind = load_job(&db, job_id)?.kind;
    let result = if kind == "install" {
        install_package(&db, job_id)
    } else {
        analyze_project(&mut db, job_id)
    };
    match result {
        Ok(()) => {
            let completed = now();
            db.execute(
                "UPDATE speaker_jobs SET status='completed',stage='完成',progress=1,worker_pid=NULL,completed_at=?2,updated_at=?2 WHERE id=?1 AND status='running'",
                params![job_id, completed],
            )?;
            Ok(())
        }
        Err(error) => {
            let current = load_job(&db, job_id)?;
            if current.status == "cancelled" {
                return Ok(());
            }
            let failed = now();
            db.execute(
                "UPDATE speaker_jobs SET status='failed',worker_pid=NULL,error_message=?2,completed_at=?3,updated_at=?3 WHERE id=?1",
                params![job_id, error.to_string(), failed],
            )?;
            Err(error)
        }
    }
}

fn downloaded_bytes() -> u64 {
    DOWNLOADS
        .iter()
        .map(|asset| {
            let complete = download_dir().join(asset.file_name);
            let partial = download_dir().join(format!("{}.part", asset.file_name));
            fs::metadata(complete)
                .or_else(|_| fs::metadata(partial))
                .map(|value| value.len().min(asset.size))
                .unwrap_or(0)
        })
        .sum()
}

fn install_package(db: &Connection, job_id: &str) -> Result<()> {
    fs::create_dir_all(download_dir())?;
    for (index, asset) in DOWNLOADS.iter().enumerate() {
        update_job(db, job_id, &format!("下载组件 {}/3", index + 1), None, None)?;
        download_asset(db, job_id, *asset)?;
    }
    update_job(db, job_id, "解包并校验", Some(0.94), Some(DOWNLOAD_SIZE))?;
    let staging = package_dir().join(format!("staging-{job_id}"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(staging.join("runtime"))?;
    fs::create_dir_all(staging.join("models"))?;
    fs::create_dir_all(staging.join("licenses"))?;
    extract_selected(
        &download_dir().join(DOWNLOADS[0].file_name),
        &staging,
        &[
            (
                "/bin/sherpa-onnx-offline-speaker-diarization.exe",
                "runtime/sherpa-onnx-offline-speaker-diarization.exe",
            ),
            ("/bin/onnxruntime.dll", "runtime/onnxruntime.dll"),
            (
                "/bin/onnxruntime_providers_shared.dll",
                "runtime/onnxruntime_providers_shared.dll",
            ),
        ],
    )?;
    extract_selected(
        &download_dir().join(DOWNLOADS[1].file_name),
        &staging,
        &[
            (
                "/model.int8.onnx",
                "models/pyannote-segmentation-3.0-int8.onnx",
            ),
            ("/LICENSE", "licenses/pyannote-segmentation-MIT.txt"),
        ],
    )?;
    fs::copy(
        download_dir().join(DOWNLOADS[2].file_name),
        staging.join("models/3dspeaker-eres2net-base-16k.onnx"),
    )?;
    fs::write(
        staging.join("licenses/NOTICE.txt"),
        "sherpa-onnx: Apache-2.0\nONNX Runtime: MIT\npyannote segmentation 3.0: MIT\n3D-Speaker: Apache-2.0\n",
    )?;
    for spec in INSTALLED_ASSETS {
        let path = staging.join(spec.relative_path);
        if !path.is_file()
            || fs::metadata(&path)?.len() != spec.size
            || hash_file(&path)? != spec.sha256
        {
            bail!(
                "speaker_package_hash_mismatch: 解包后的 {} 未通过 SHA-256 校验",
                spec.name
            )
        }
    }
    for spec in INSTALLED_ASSETS {
        let source = staging.join(spec.relative_path);
        let target = installed_path(*spec);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if target.is_file() {
            fs::remove_file(&target)?;
        }
        fs::rename(source, target)?;
    }
    let notice_target = package_dir().join("licenses/NOTICE.txt");
    fs::create_dir_all(notice_target.parent().expect("license directory"))?;
    fs::copy(staging.join("licenses/NOTICE.txt"), notice_target)?;
    let pyannote_license = staging.join("licenses/pyannote-segmentation-MIT.txt");
    if pyannote_license.is_file() {
        fs::copy(
            pyannote_license,
            package_dir().join("licenses/pyannote-segmentation-MIT.txt"),
        )?;
    }
    fs::remove_dir_all(staging)?;
    let status = package_status(true)?;
    if !status.installed || status.verified != Some(true) {
        bail!("speaker_package_hash_mismatch: 说话人模型包安装后校验失败")
    }
    Ok(())
}

fn update_job(
    db: &Connection,
    job_id: &str,
    stage: &str,
    progress: Option<f64>,
    bytes: Option<u64>,
) -> Result<()> {
    db.execute(
        "UPDATE speaker_jobs SET stage=?2,progress=COALESCE(?3,progress),bytes_downloaded=COALESCE(?4,bytes_downloaded),updated_at=?5 WHERE id=?1 AND status='running'",
        params![job_id, stage, progress, bytes, now()],
    )?;
    Ok(())
}

fn download_asset(db: &Connection, job_id: &str, asset: DownloadAsset) -> Result<()> {
    let target = download_dir().join(asset.file_name);
    if target.is_file()
        && fs::metadata(&target)?.len() == asset.size
        && hash_file(&target)? == asset.sha256
    {
        update_download_progress(db, job_id)?;
        return Ok(());
    }
    if target.is_file() {
        fs::remove_file(&target)?;
    }
    let partial = download_dir().join(format!("{}.part", asset.file_name));
    let mut existing = fs::metadata(&partial)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if existing > asset.size {
        fs::remove_file(&partial)?;
        existing = 0;
    }
    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;
    let mut request = client.get(asset.url);
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let mut response = request.send().context("说话人模型下载连接失败")?;
    if !response.status().is_success() {
        bail!("说话人模型下载失败：HTTP {}", response.status())
    }
    let append = existing > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&partial)?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut last_update = Instant::now() - Duration::from_secs(1);
    loop {
        let count = response.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        if last_update.elapsed() >= Duration::from_millis(400) {
            if load_job(db, job_id)?.status == "cancelled" {
                return Ok(());
            }
            update_download_progress(db, job_id)?;
            last_update = Instant::now();
        }
    }
    output.flush()?;
    drop(output);
    if fs::metadata(&partial)?.len() != asset.size {
        bail!("说话人模型下载不完整，可显式继续")
    }
    if hash_file(&partial)? != asset.sha256 {
        fs::remove_file(&partial)?;
        bail!("speaker_package_hash_mismatch: 下载资产 SHA-256 校验失败")
    }
    fs::rename(partial, target)?;
    update_download_progress(db, job_id)?;
    Ok(())
}

fn update_download_progress(db: &Connection, job_id: &str) -> Result<()> {
    let bytes = downloaded_bytes();
    update_job(
        db,
        job_id,
        &load_job(db, job_id)?.stage,
        Some((bytes as f64 / DOWNLOAD_SIZE as f64).clamp(0.0, 0.92)),
        Some(bytes),
    )
}

fn extract_selected(archive_path: &Path, staging: &Path, wanted: &[(&str, &str)]) -> Result<()> {
    let decoder = BzDecoder::new(File::open(archive_path)?);
    let mut archive = Archive::new(decoder);
    let mut extracted = vec![false; wanted.len()];
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let archive_name = entry.path()?.to_string_lossy().replace('\\', "/");
        for (index, (suffix, relative_target)) in wanted.iter().enumerate() {
            if archive_name.ends_with(suffix) {
                let target = staging.join(relative_target);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut output = File::create(target)?;
                std::io::copy(&mut entry, &mut output)?;
                extracted[index] = true;
                break;
            }
        }
    }
    if let Some((missing, _)) = wanted
        .iter()
        .zip(extracted)
        .find(|(_, extracted)| !*extracted)
    {
        bail!("speaker_package_archive_invalid: 归档缺少 {}", missing.0)
    }
    Ok(())
}

fn analyze_project(db: &mut Connection, job_id: &str) -> Result<()> {
    let job = load_job(db, job_id)?;
    let project_id = job
        .project_id
        .as_deref()
        .ok_or_else(|| anyhow!("speaker_project_missing: 分析任务缺少项目"))?;
    let loaded = project::load(db, project_id)?;
    let source = Path::new(&loaded.media.source_path).canonicalize()?;
    let work_dir = package_dir().join("work").join(job_id);
    if work_dir.exists() {
        fs::remove_dir_all(&work_dir)?;
    }
    fs::create_dir_all(&work_dir)?;
    let wav = work_dir.join("speaker-16k.wav");
    update_job(db, job_id, "准备 16 kHz 音频", Some(0.08), None)?;
    let ffmpeg = tool_path("SIAOCUT_FFMPEG", "ffmpeg");
    let output = hidden_command(ffmpeg)
        .args(["-y", "-v", "error", "-i"])
        .arg(&source)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
        .arg(&wav)
        .output()
        .context("无法启动 FFmpeg 准备说话人音频")?;
    if !output.status.success() {
        bail!(
            "speaker_audio_prepare_failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
    if load_job(db, job_id)?.status == "cancelled" {
        return Ok(());
    }
    update_job(db, job_id, "本地识别说话人", Some(0.2), None)?;
    let runtime = installed_path(INSTALLED_ASSETS[0]);
    let segmentation = installed_path(INSTALLED_ASSETS[3]);
    let embedding = installed_path(INSTALLED_ASSETS[4]);
    let output = hidden_command(&runtime)
        .arg("--clustering.cluster-threshold=0.90")
        .arg(format!(
            "--segmentation.pyannote-model={}",
            segmentation.display()
        ))
        .arg(format!("--embedding.model={}", embedding.display()))
        .arg("--print-args=false")
        .arg(&wav)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("无法启动本地说话人分离运行时")?;
    if !output.status.success() {
        bail!(
            "speaker_runtime_failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
    update_job(db, job_id, "建立字幕关联", Some(0.88), None)?;
    let parsed = parse_runtime_output(&String::from_utf8_lossy(&output.stdout))?;
    let track = build_track(&loaded.transcript.segments, &parsed);
    project::mutate_with_snapshot(db, project_id, "生成说话人轨", |tx| {
        replace_track_tx(tx, project_id, Some(&track))
    })?;
    if work_dir.exists() {
        fs::remove_dir_all(work_dir)?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
struct ParsedTurn {
    start: f64,
    end: f64,
    source_label: String,
}

fn parse_runtime_output(output: &str) -> Result<Vec<ParsedTurn>> {
    let mut turns = Vec::new();
    for line in output.lines() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 4 || parts[1] != "--" || !parts[3].starts_with("speaker_") {
            continue;
        }
        let start = parts[0].parse::<f64>()?;
        let end = parts[2].parse::<f64>()?;
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
            bail!("speaker_runtime_output_invalid: 运行时返回了无效时间范围")
        }
        turns.push(ParsedTurn {
            start,
            end,
            source_label: parts[3].into(),
        });
    }
    turns.sort_by(|left, right| left.start.total_cmp(&right.start));
    Ok(turns)
}

fn build_track(segments: &[crate::model::Segment], parsed: &[ParsedTurn]) -> SpeakerTrack {
    let generated_at = now();
    let mut source_ids = BTreeMap::new();
    for turn in parsed {
        if !source_ids.contains_key(&turn.source_label) {
            let index = source_ids.len();
            source_ids.insert(turn.source_label.clone(), (new_id("voice"), index));
        }
    }
    let speakers = source_ids
        .iter()
        .map(|(source_label, (id, index))| SpeakerIdentity {
            id: id.clone(),
            source_label: source_label.clone(),
            label: format!("说话人 {}", index + 1),
            color_index: *index as u32,
            created_at: generated_at.clone(),
        })
        .collect::<Vec<_>>();
    let turns = parsed
        .iter()
        .map(|turn| SpeakerTurn {
            id: new_id("turn"),
            speaker_id: source_ids[&turn.source_label].0.clone(),
            start: turn.start,
            end: turn.end,
            confidence: None,
            source: "sherpa-onnx".into(),
            model_version: RUNTIME_VERSION.into(),
            created_at: generated_at.clone(),
        })
        .collect::<Vec<_>>();
    let associations = associate_segments(segments, &turns, &generated_at);
    SpeakerTrack {
        status: if turns.is_empty() {
            "no_speech"
        } else {
            "ready"
        }
        .into(),
        runtime_version: RUNTIME_VERSION.into(),
        segmentation_model: SEGMENTATION_MODEL.into(),
        embedding_model: EMBEDDING_MODEL.into(),
        generated_at: Some(generated_at),
        speakers,
        turns,
        associations,
    }
}

fn associate_segments(
    segments: &[crate::model::Segment],
    turns: &[SpeakerTurn],
    timestamp: &str,
) -> Vec<SegmentSpeaker> {
    segments
        .iter()
        .filter_map(|segment| {
            let duration = (segment.end - segment.start).max(f64::EPSILON);
            turns
                .iter()
                .map(|turn| {
                    let overlap =
                        (segment.end.min(turn.end) - segment.start.max(turn.start)).max(0.0);
                    (turn, overlap)
                })
                .filter(|(_, overlap)| *overlap > 0.0)
                .max_by(|left, right| left.1.total_cmp(&right.1))
                .map(|(turn, overlap)| SegmentSpeaker {
                    segment_id: segment.id.clone(),
                    speaker_id: turn.speaker_id.clone(),
                    source: "overlap".into(),
                    confidence: Some((overlap / duration).clamp(0.0, 1.0)),
                    updated_at: timestamp.into(),
                })
        })
        .collect()
}

pub fn load_track(db: &Connection, project_id: &str) -> Result<SpeakerTrack> {
    let metadata = db
        .query_row(
            "SELECT status,runtime_version,segmentation_model,embedding_model,generated_at FROM speaker_tracks WHERE project_id=?1",
            [project_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    let Some(metadata) = metadata else {
        return Ok(SpeakerTrack::default());
    };
    let speakers = db
        .prepare("SELECT id,source_label,label,color_index,created_at FROM speakers WHERE project_id=?1 ORDER BY color_index,id")?
        .query_map([project_id], |row| {
            Ok(SpeakerIdentity {
                id: row.get(0)?,
                source_label: row.get(1)?,
                label: row.get(2)?,
                color_index: row.get::<_, i64>(3)? as u32,
                created_at: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let turns = db
        .prepare("SELECT id,speaker_id,start_seconds,end_seconds,confidence,source,model_version,created_at FROM speaker_turns WHERE project_id=?1 ORDER BY start_seconds,id")?
        .query_map([project_id], |row| {
            Ok(SpeakerTurn {
                id: row.get(0)?,
                speaker_id: row.get(1)?,
                start: row.get(2)?,
                end: row.get(3)?,
                confidence: row.get(4)?,
                source: row.get(5)?,
                model_version: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let associations = db
        .prepare("SELECT segment_id,speaker_id,source,confidence,updated_at FROM segment_speakers WHERE project_id=?1 ORDER BY segment_id")?
        .query_map([project_id], |row| {
            Ok(SegmentSpeaker {
                segment_id: row.get(0)?,
                speaker_id: row.get(1)?,
                source: row.get(2)?,
                confidence: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(SpeakerTrack {
        status: metadata.0,
        runtime_version: metadata.1,
        segmentation_model: metadata.2,
        embedding_model: metadata.3,
        generated_at: Some(metadata.4),
        speakers,
        turns,
        associations,
    })
}

#[cfg(test)]
fn replace_track(db: &mut Connection, project_id: &str, track: &SpeakerTrack) -> Result<()> {
    let tx = db.transaction()?;
    replace_track_tx(&tx, project_id, Some(track))?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn clear_track_tx(tx: &Transaction<'_>, project_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM segment_speakers WHERE project_id=?1",
        [project_id],
    )?;
    tx.execute(
        "DELETE FROM speaker_turns WHERE project_id=?1",
        [project_id],
    )?;
    tx.execute("DELETE FROM speakers WHERE project_id=?1", [project_id])?;
    tx.execute(
        "DELETE FROM speaker_tracks WHERE project_id=?1",
        [project_id],
    )?;
    Ok(())
}

pub(crate) fn replace_track_tx(
    tx: &Transaction<'_>,
    project_id: &str,
    track: Option<&SpeakerTrack>,
) -> Result<()> {
    clear_track_tx(tx, project_id)?;
    let Some(track) = track.filter(|track| track.status != "not_analyzed") else {
        return Ok(());
    };
    tx.execute(
        "INSERT INTO speaker_tracks(project_id,status,runtime_version,segmentation_model,embedding_model,generated_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            project_id,
            track.status,
            track.runtime_version,
            track.segmentation_model,
            track.embedding_model,
            track.generated_at.as_deref().unwrap_or_default()
        ],
    )?;
    for speaker in &track.speakers {
        tx.execute(
            "INSERT INTO speakers(id,project_id,source_label,label,color_index,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![speaker.id, project_id, speaker.source_label, speaker.label, speaker.color_index, speaker.created_at],
        )?;
    }
    for turn in &track.turns {
        tx.execute(
            "INSERT INTO speaker_turns(id,project_id,speaker_id,start_seconds,end_seconds,confidence,source,model_version,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![turn.id, project_id, turn.speaker_id, turn.start, turn.end, turn.confidence, turn.source, turn.model_version, turn.created_at],
        )?;
    }
    for association in &track.associations {
        tx.execute(
            "INSERT INTO segment_speakers(project_id,segment_id,speaker_id,source,confidence,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![project_id, association.segment_id, association.speaker_id, association.source, association.confidence, association.updated_at],
        )?;
    }
    Ok(())
}

pub fn rename(
    db: &mut Connection,
    project_id: &str,
    speaker_id: &str,
    label: &str,
) -> Result<SpeakerTrack> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 40 {
        bail!("speaker_label_invalid: 说话人名称需要 1 至 40 个字符")
    }
    project::mutate_with_snapshot(db, project_id, "重命名说话人", |tx| {
        let changed = tx.execute(
            "UPDATE speakers SET label=?3 WHERE id=?1 AND project_id=?2",
            params![speaker_id, project_id, label],
        )?;
        if changed == 0 {
            bail!("speaker_not_found: 说话人不存在")
        }
        Ok(())
    })?;
    load_track(db, project_id)
}

pub fn merge(
    db: &mut Connection,
    project_id: &str,
    from_id: &str,
    into_id: &str,
) -> Result<SpeakerTrack> {
    if from_id == into_id {
        bail!("speaker_merge_same: 请选择两个不同的说话人")
    }
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM speakers WHERE project_id=?1 AND id IN (?2,?3)",
        params![project_id, from_id, into_id],
        |row| row.get(0),
    )?;
    if count != 2 {
        bail!("speaker_not_found: 合并的说话人不存在")
    }
    let tx = db.transaction()?;
    tx.execute(
        "UPDATE speaker_turns SET speaker_id=?3 WHERE project_id=?1 AND speaker_id=?2",
        params![project_id, from_id, into_id],
    )?;
    tx.execute(
        "UPDATE segment_speakers SET speaker_id=?3,source='manual',confidence=NULL,updated_at=?4 WHERE project_id=?1 AND speaker_id=?2",
        params![project_id, from_id, into_id, now()],
    )?;
    tx.execute(
        "DELETE FROM speakers WHERE project_id=?1 AND id=?2",
        params![project_id, from_id],
    )?;
    project::snapshot_in_transaction(&tx, project_id, "合并说话人")?;
    tx.commit()?;
    load_track(db, project_id)
}

pub fn assign(
    db: &mut Connection,
    project_id: &str,
    segment_id: &str,
    speaker_id: &str,
) -> Result<SpeakerTrack> {
    let valid: i64 = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM segments WHERE id=?2 AND project_id=?1) AND EXISTS(SELECT 1 FROM speakers WHERE id=?3 AND project_id=?1)",
        params![project_id, segment_id, speaker_id],
        |row| row.get(0),
    )?;
    if valid == 0 {
        bail!("speaker_assignment_invalid: 字幕段或说话人不存在")
    }
    project::mutate_with_snapshot(db, project_id, "重新分配说话人", |tx| {
        tx.execute(
            "INSERT INTO segment_speakers(project_id,segment_id,speaker_id,source,confidence,updated_at) VALUES(?1,?2,?3,'manual',NULL,?4) ON CONFLICT(project_id,segment_id) DO UPDATE SET speaker_id=excluded.speaker_id,source='manual',confidence=NULL,updated_at=excluded.updated_at",
            params![project_id, segment_id, speaker_id, now()],
        )?;
        Ok(())
    })?;
    load_track(db, project_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Segment;
    use tempfile::tempdir;

    #[test]
    fn speaker_runtime_output_parser_ignores_progress_and_keeps_turns() {
        let turns = parse_runtime_output(
            "progress 50.00%\nStarted\n0.638 -- 6.848 speaker_00\n7.017 -- 10.696 speaker_01\n",
        )
        .unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[1].source_label, "speaker_01");
    }

    #[test]
    fn speaker_association_uses_largest_overlap_without_editing_text() {
        let segments = vec![Segment {
            id: "s1".into(),
            start: 1.0,
            end: 4.0,
            text: "保持原文".into(),
            confidence: Some(0.9),
        }];
        let turns = vec![
            SpeakerTurn {
                id: "t1".into(),
                speaker_id: "a".into(),
                start: 0.0,
                end: 1.5,
                confidence: None,
                source: "test".into(),
                model_version: "test".into(),
                created_at: "now".into(),
            },
            SpeakerTurn {
                id: "t2".into(),
                speaker_id: "b".into(),
                start: 1.5,
                end: 4.0,
                confidence: None,
                source: "test".into(),
                model_version: "test".into(),
                created_at: "now".into(),
            },
        ];
        let associations = associate_segments(&segments, &turns, "now");
        assert_eq!(associations[0].speaker_id, "b");
        assert_eq!(segments[0].text, "保持原文");
    }

    #[test]
    fn speaker_package_is_optional_for_project_database() {
        let temp = tempdir().unwrap();
        let db = crate::db::open_at(&temp.path().join("speaker.db")).unwrap();
        let track = load_track(&db, "missing-project").unwrap();
        assert_eq!(track.status, "not_analyzed");
        assert!(track.speakers.is_empty());
    }

    #[test]
    fn speaker_manual_adjustments_are_recoverable_project_history() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("dialogue.wav");
        fs::write(&media, b"fixture").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("history.db")).unwrap();
        let project = crate::project::create(&mut db, &media, Some("dialogue".into())).unwrap();
        let first =
            crate::project::add_segment(&mut db, &project.id, 0.0, 1.0, "第一段".into(), Some(0.9))
                .unwrap();
        let second =
            crate::project::add_segment(&mut db, &project.id, 1.0, 2.0, "第二段".into(), Some(0.9))
                .unwrap();
        let timestamp = now();
        let baseline = SpeakerTrack {
            status: "ready".into(),
            runtime_version: RUNTIME_VERSION.into(),
            segmentation_model: SEGMENTATION_MODEL.into(),
            embedding_model: EMBEDDING_MODEL.into(),
            generated_at: Some(timestamp.clone()),
            speakers: vec![
                SpeakerIdentity {
                    id: "voice-a".into(),
                    source_label: "speaker_00".into(),
                    label: "说话人 1".into(),
                    color_index: 0,
                    created_at: timestamp.clone(),
                },
                SpeakerIdentity {
                    id: "voice-b".into(),
                    source_label: "speaker_01".into(),
                    label: "说话人 2".into(),
                    color_index: 1,
                    created_at: timestamp.clone(),
                },
            ],
            turns: vec![
                SpeakerTurn {
                    id: "turn-a".into(),
                    speaker_id: "voice-a".into(),
                    start: 0.0,
                    end: 1.0,
                    confidence: None,
                    source: "test".into(),
                    model_version: RUNTIME_VERSION.into(),
                    created_at: timestamp.clone(),
                },
                SpeakerTurn {
                    id: "turn-b".into(),
                    speaker_id: "voice-b".into(),
                    start: 1.0,
                    end: 2.0,
                    confidence: None,
                    source: "test".into(),
                    model_version: RUNTIME_VERSION.into(),
                    created_at: timestamp.clone(),
                },
            ],
            associations: vec![
                SegmentSpeaker {
                    segment_id: first.id.clone(),
                    speaker_id: "voice-a".into(),
                    source: "overlap".into(),
                    confidence: Some(1.0),
                    updated_at: timestamp.clone(),
                },
                SegmentSpeaker {
                    segment_id: second.id.clone(),
                    speaker_id: "voice-b".into(),
                    source: "overlap".into(),
                    confidence: Some(1.0),
                    updated_at: timestamp,
                },
            ],
        };
        replace_track(&mut db, &project.id, &baseline).unwrap();
        crate::project::snapshot(&db, &project.id, "生成说话人轨").unwrap();

        rename(&mut db, &project.id, "voice-a", "主持人").unwrap();
        assert_eq!(
            load_track(&db, &project.id).unwrap().speakers[0].label,
            "主持人"
        );
        crate::project::undo(&mut db, &project.id).unwrap();
        assert_eq!(
            load_track(&db, &project.id).unwrap().speakers[0].label,
            "说话人 1"
        );

        merge(&mut db, &project.id, "voice-b", "voice-a").unwrap();
        assert_eq!(load_track(&db, &project.id).unwrap().speakers.len(), 1);
        crate::project::undo(&mut db, &project.id).unwrap();
        assert_eq!(load_track(&db, &project.id).unwrap().speakers.len(), 2);

        assign(&mut db, &project.id, &first.id, "voice-b").unwrap();
        let assigned = load_track(&db, &project.id).unwrap();
        assert_eq!(
            assigned
                .associations
                .iter()
                .find(|item| item.segment_id == first.id)
                .unwrap()
                .speaker_id,
            "voice-b"
        );
        crate::project::undo(&mut db, &project.id).unwrap();
        let restored = load_track(&db, &project.id).unwrap();
        assert_eq!(
            restored
                .associations
                .iter()
                .find(|item| item.segment_id == first.id)
                .unwrap()
                .speaker_id,
            "voice-a"
        );
        assert_eq!(
            crate::project::load(&db, &project.id)
                .unwrap()
                .transcript
                .segments[0]
                .text,
            "第一段"
        );
    }
}
