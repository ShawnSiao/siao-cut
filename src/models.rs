use crate::{
    db,
    media::hash_file,
    util::{new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{StatusCode, blocking::Client, header::RANGE};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::{
    env, fs,
    fs::OpenOptions,
    io::{Read, Write},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub file_name: &'static str,
    pub description: &'static str,
    pub source: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    pub license: &'static str,
    pub recommended: bool,
}

const MODEL_SPECS: &[ModelSpec] = &[
    ModelSpec {
        id: "tiny",
        name: "省空间",
        file_name: "ggml-tiny.bin",
        description: "约 74 MB，适合快速试用与低配置电脑。",
        source: "https://huggingface.co/ggerganov/whisper.cpp",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        size: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        license: "MIT",
        recommended: false,
    },
    ModelSpec {
        id: "base",
        name: "平衡",
        file_name: "ggml-base.bin",
        description: "约 141 MB，默认推荐，兼顾速度与中英识别质量。",
        source: "https://huggingface.co/ggerganov/whisper.cpp",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        size: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        license: "MIT",
        recommended: true,
    },
    ModelSpec {
        id: "small",
        name: "高质量",
        file_name: "ggml-small.bin",
        description: "约 465 MB，识别质量更高，CPU 转录耗时更长。",
        source: "https://huggingface.co/ggerganov/whisper.cpp",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        size: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        license: "MIT",
        recommended: false,
    },
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    #[serde(flatten)]
    pub spec: ModelSpec,
    pub path: String,
    pub installed: bool,
    pub bytes_on_disk: u64,
    pub verified: Option<bool>,
    pub verification_status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadJob {
    pub id: String,
    pub model_id: String,
    pub status: String,
    pub stage_code: Option<String>,
    pub progress: f64,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub target_path: String,
    pub cancel_requested_at: Option<String>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub worker_pid: Option<u32>,
}

fn spec(model_id: &str) -> Result<ModelSpec> {
    MODEL_SPECS
        .iter()
        .copied()
        .find(|item| item.id == model_id)
        .ok_or_else(|| anyhow!("未知转录模型：{model_id}"))
}

fn models_dir() -> PathBuf {
    db::home_dir().join("models")
}

fn target_path(spec: ModelSpec) -> PathBuf {
    models_dir().join(spec.file_name)
}

fn partial_path(spec: ModelSpec) -> PathBuf {
    models_dir().join(format!("{}.part", spec.file_name))
}

pub fn catalog(verify: bool) -> Result<Vec<ModelStatus>> {
    MODEL_SPECS
        .iter()
        .copied()
        .map(|item| {
            let path = target_path(item);
            let bytes = fs::metadata(&path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            let installed = path.is_file();
            let verified = if verify && installed {
                Some(bytes == item.size && hash_file(&path)? == item.sha256)
            } else {
                None
            };
            Ok(ModelStatus {
                spec: item,
                path: path.to_string_lossy().to_string(),
                installed,
                bytes_on_disk: bytes,
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
            })
        })
        .collect()
}

pub fn verify(model_id: &str) -> Result<ModelStatus> {
    let wanted = spec(model_id)?;
    catalog(true)?
        .into_iter()
        .find(|item| item.spec.id == wanted.id)
        .ok_or_else(|| anyhow!("未知转录模型：{model_id}"))
}

pub fn create_download(db: &Connection, model_id: &str) -> Result<ModelDownloadJob> {
    let spec = spec(model_id)?;
    fs::create_dir_all(models_dir())?;
    if target_path(spec).is_file() {
        let status = verify(model_id)?;
        if status.verified == Some(true) {
            bail!("模型已经安装并通过校验：{model_id}")
        }
        bail!("model_hash_mismatch: 已有模型未通过校验，请先移除后重新下载")
    }
    if let Some(job) = active_job(db, model_id)? {
        return Ok(job);
    }
    let partial_bytes = fs::metadata(partial_path(spec))
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        .min(spec.size);
    let remaining = spec.size.saturating_sub(partial_bytes);
    let available = crate::util::available_space(&models_dir())?;
    let reserve = 128 * 1024 * 1024;
    if available < remaining.saturating_add(reserve) {
        bail!(
            "disk_space_low: 下载仍需 {:.1} MB，可用空间不足",
            remaining as f64 / 1_048_576.0
        )
    }
    let timestamp = now();
    let job = ModelDownloadJob {
        id: new_id("m"),
        model_id: model_id.to_owned(),
        status: "queued".into(),
        stage_code: Some("queued".into()),
        progress: partial_bytes as f64 / spec.size as f64,
        bytes_downloaded: partial_bytes,
        total_bytes: spec.size,
        target_path: target_path(spec).to_string_lossy().to_string(),
        cancel_requested_at: None,
        error_message: None,
        error_code: None,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        completed_at: None,
        worker_pid: None,
    };
    db.execute(
        "INSERT INTO model_downloads(id,model_id,status,progress,bytes_downloaded,total_bytes,target_path,created_at,updated_at) VALUES(?1,?2,'queued',?3,?4,?5,?6,?7,?7)",
        params![job.id, job.model_id, job.progress, job.bytes_downloaded, job.total_bytes, job.target_path, job.created_at],
    )?;
    spawn_worker(&job.id, model_id)?;
    Ok(job)
}

fn active_job(db: &Connection, model_id: &str) -> Result<Option<ModelDownloadJob>> {
    let id = db
        .query_row(
            "SELECT id FROM model_downloads WHERE model_id=?1 AND status IN ('queued','running') ORDER BY created_at DESC LIMIT 1",
            [model_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    id.map(|id| load_job(db, &id)).transpose()
}

pub fn load_job(db: &Connection, job_id: &str) -> Result<ModelDownloadJob> {
    db.query_row(
        "SELECT id,model_id,status,progress,bytes_downloaded,total_bytes,target_path,cancel_requested_at,error_message,created_at,updated_at,completed_at,worker_pid FROM model_downloads WHERE id=?1",
        [job_id],
        |row| {
            let status = row.get::<_, String>(2)?;
            let error_message = row.get::<_, Option<String>>(8)?;
            Ok(ModelDownloadJob {
                id: row.get(0)?,
                model_id: row.get(1)?,
                stage_code: Some(status.clone()),
                status: status.clone(),
                progress: row.get(3)?,
                bytes_downloaded: row.get(4)?,
                total_bytes: row.get(5)?,
                target_path: row.get(6)?,
                cancel_requested_at: row.get(7)?,
                error_code: crate::model::background_error_code(
                    &status,
                    error_message.as_deref(),
                ),
                error_message,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                completed_at: row.get(11)?,
                worker_pid: row.get(12)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("模型下载任务不存在：{job_id}"))
}

pub fn list_jobs(db: &Connection) -> Result<Vec<ModelDownloadJob>> {
    db.prepare("SELECT id FROM model_downloads ORDER BY created_at DESC")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| load_job(db, &id))
        .collect()
}

pub fn cancel(db: &Connection, job_id: &str) -> Result<ModelDownloadJob> {
    let changed = db.execute(
        "UPDATE model_downloads SET cancel_requested_at=?2,updated_at=?2 WHERE id=?1 AND status IN ('queued','running')",
        params![job_id, now()],
    )?;
    if changed == 0 {
        bail!(
            "模型下载任务当前状态不能取消：{}",
            load_job(db, job_id)?.status
        )
    }
    load_job(db, job_id)
}

pub fn reconcile_interrupted(db: &Connection) -> Result<()> {
    let jobs = db
        .prepare("SELECT id FROM model_downloads WHERE status IN ('queued','running')")?
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
                "UPDATE model_downloads SET status='interrupted',error_message='上次下载进程已中断，可以继续下载。',worker_pid=NULL,updated_at=?2 WHERE id=?1",
                params![id, now()],
            )?;
        }
    }
    Ok(())
}

pub fn remove(db: &Connection, model_id: &str) -> Result<()> {
    let spec = spec(model_id)?;
    if active_job(db, model_id)?.is_some() {
        bail!("请先取消正在进行的模型下载")
    }
    let target = target_path(spec);
    let partial = partial_path(spec);
    if target.is_file() {
        fs::remove_file(target)?;
    }
    if partial.is_file() {
        fs::remove_file(partial)?;
    }
    Ok(())
}

fn spawn_worker(job_id: &str, model_id: &str) -> Result<()> {
    crate::util::spawn_detached_current(&["__model_worker", job_id, model_id])
        .context("无法启动模型下载任务")?;
    Ok(())
}

pub fn run_worker(job_id: &str, model_id: &str) -> Result<()> {
    if let Some(delay) = env::var("SIAOCUT_MODEL_START_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        thread::sleep(Duration::from_millis(delay));
    }
    let db = db::open()?;
    let result = download(&db, job_id, spec(model_id)?);
    if let Err(error) = &result {
        let timestamp = now();
        let _ = db.execute(
            "UPDATE model_downloads SET status='failed',error_message=?2,worker_pid=NULL,updated_at=?3,completed_at=?3 WHERE id=?1 AND status!='cancelled'",
            params![job_id, error.to_string(), timestamp],
        );
    }
    result
}

fn download(db: &Connection, job_id: &str, spec: ModelSpec) -> Result<()> {
    let initial = load_job(db, job_id)?;
    if initial.model_id != spec.id {
        bail!("模型下载任务与模型不匹配")
    }
    if initial.cancel_requested_at.is_some() {
        finish_cancelled(db, job_id)?;
        return Ok(());
    }
    let partial = partial_path(spec);
    let target = target_path(spec);
    let mut existing = fs::metadata(&partial)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if existing > spec.size {
        fs::remove_file(&partial)?;
        existing = 0;
    }
    db.execute(
        "UPDATE model_downloads SET status='running',bytes_downloaded=?2,progress=?3,error_message=NULL,worker_pid=?4,updated_at=?5 WHERE id=?1",
        params![job_id, existing, existing as f64 / spec.size as f64, std::process::id(), now()],
    )?;

    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;
    let mut request = client.get(spec.url);
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let mut response = request.send().context("模型下载连接失败")?;
    if !response.status().is_success() {
        bail!("模型下载失败：HTTP {}", response.status())
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
        .open(&partial)?;
    let mut downloaded = existing;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut last_update = Instant::now() - Duration::from_secs(1);
    let chunk_delay_ms = env::var("SIAOCUT_MODEL_CHUNK_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());
    loop {
        let count = response.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])?;
        downloaded = downloaded.saturating_add(count as u64);
        if let Some(delay) = chunk_delay_ms {
            thread::sleep(Duration::from_millis(delay));
        }
        if last_update.elapsed() >= Duration::from_millis(400) {
            let cancelled: bool = db.query_row(
                "SELECT cancel_requested_at IS NOT NULL FROM model_downloads WHERE id=?1",
                [job_id],
                |row| row.get(0),
            )?;
            if cancelled {
                file.flush()?;
                finish_cancelled(db, job_id)?;
                return Ok(());
            }
            db.execute(
                "UPDATE model_downloads SET bytes_downloaded=?2,progress=?3,updated_at=?4 WHERE id=?1",
                params![job_id, downloaded, (downloaded as f64 / spec.size as f64).clamp(0.0, 0.99), now()],
            )?;
            last_update = Instant::now();
        }
    }
    file.flush()?;
    drop(file);
    let actual_size = fs::metadata(&partial)?.len();
    if actual_size != spec.size {
        bail!(
            "模型下载不完整：应为 {} 字节，实际为 {} 字节；可重新继续下载",
            spec.size,
            actual_size
        )
    }
    let actual_hash = hash_file(&partial)?;
    if actual_hash != spec.sha256 {
        fs::remove_file(&partial)?;
        bail!("model_hash_mismatch: 模型 SHA-256 校验失败，已删除无效下载")
    }
    if target.is_file() {
        fs::remove_file(&target)?;
    }
    fs::rename(&partial, &target)?;
    let timestamp = now();
    db.execute(
        "UPDATE model_downloads SET status='completed',progress=1,bytes_downloaded=?2,worker_pid=NULL,updated_at=?3,completed_at=?3 WHERE id=?1",
        params![job_id, spec.size, timestamp],
    )?;
    Ok(())
}

fn finish_cancelled(db: &Connection, job_id: &str) -> Result<()> {
    let timestamp = now();
    db.execute(
        "UPDATE model_downloads SET status='cancelled',worker_pid=NULL,updated_at=?2,completed_at=?2 WHERE id=?1",
        params![job_id, timestamp],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_exposes_three_auditable_profiles() {
        let catalog = catalog(false).unwrap();
        assert_eq!(catalog.len(), 3);
        assert_eq!(catalog[1].spec.id, "base");
        assert!(catalog[1].spec.recommended);
        assert_eq!(catalog[1].spec.sha256.len(), 64);
    }

    #[test]
    fn rejects_unknown_model() {
        assert!(
            spec("untrusted")
                .unwrap_err()
                .to_string()
                .contains("未知转录模型")
        );
    }
}
