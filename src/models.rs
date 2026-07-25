use crate::{
    db,
    media::hash_file,
    util::{new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{StatusCode, blocking::Client, header::RANGE};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;
use std::{
    env, fs,
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
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

fn target_path_in(models_dir: &Path, spec: ModelSpec) -> PathBuf {
    models_dir.join(spec.file_name)
}

fn partial_path_in(models_dir: &Path, spec: ModelSpec) -> PathBuf {
    models_dir.join(format!("{}.part", spec.file_name))
}

fn model_status(spec: ModelSpec, models_dir: &Path, verify: bool) -> Result<ModelStatus> {
    let path = target_path_in(models_dir, spec);
    let bytes = fs::metadata(&path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let installed = path.is_file();
    let verified = if verify && installed {
        Some(bytes == spec.size && hash_file(&path)? == spec.sha256)
    } else {
        None
    };
    Ok(ModelStatus {
        spec,
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
}

pub fn catalog(verify: bool) -> Result<Vec<ModelStatus>> {
    let models_dir = models_dir();
    MODEL_SPECS
        .iter()
        .copied()
        .map(|item| model_status(item, &models_dir, verify))
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
    create_download_in(db, spec, &models_dir(), spawn_worker)
}

fn create_download_in(
    db: &Connection,
    spec: ModelSpec,
    models_dir: &Path,
    spawn: impl FnOnce(&str, &str) -> Result<()>,
) -> Result<ModelDownloadJob> {
    fs::create_dir_all(models_dir)?;
    if target_path_in(models_dir, spec).is_file() {
        let status = model_status(spec, models_dir, true)?;
        if status.verified == Some(true) {
            bail!("模型已经安装并通过校验：{}", spec.id)
        }
        bail!("model_hash_mismatch: 已有模型未通过校验，请先移除后重新下载")
    }
    // This fast path preserves idempotent polling. The transaction below
    // repeats the check and remains the authority for concurrent creators.
    if let Some(active) = active_job(db, spec.id)? {
        return Ok(active);
    }
    let partial_bytes = fs::metadata(partial_path_in(models_dir, spec))
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        .min(spec.size);
    let remaining = spec.size.saturating_sub(partial_bytes);
    let available = crate::util::available_space(models_dir)?;
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
        model_id: spec.id.to_owned(),
        status: "queued".into(),
        stage_code: Some("queued".into()),
        progress: partial_bytes as f64 / spec.size as f64,
        bytes_downloaded: partial_bytes,
        total_bytes: spec.size,
        target_path: target_path_in(models_dir, spec)
            .to_string_lossy()
            .to_string(),
        cancel_requested_at: None,
        error_message: None,
        error_code: None,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        completed_at: None,
        worker_pid: None,
    };

    // Serialize the active-job check and insert across every SQLite connection.
    // A process-local mutex would not protect the detached worker/service model.
    db.busy_timeout(Duration::from_secs(5))?;
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)
        .context("无法锁定模型下载任务队列")?;
    if let Some(active) = active_job(&tx, spec.id)? {
        tx.commit()?;
        return Ok(active);
    }
    tx.execute(
        "INSERT INTO model_downloads(id,model_id,status,progress,bytes_downloaded,total_bytes,target_path,created_at,updated_at) VALUES(?1,?2,'queued',?3,?4,?5,?6,?7,?7)",
        params![job.id, job.model_id, job.progress, job.bytes_downloaded, job.total_bytes, job.target_path, job.created_at],
    )?;
    tx.commit()?;

    if let Err(error) = spawn(&job.id, spec.id) {
        let timestamp = now();
        let _ = db.execute(
            "UPDATE model_downloads SET status='failed',error_message=?2,updated_at=?3,completed_at=?3 WHERE id=?1 AND status='queued'",
            params![job.id, error.to_string(), timestamp],
        );
        return Err(error);
    }
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
            let timestamp = now();
            db.execute(
                "UPDATE model_downloads
                 SET status='interrupted',error_message='上次下载进程已中断，可以继续下载。',worker_pid=NULL,updated_at=?2
                 WHERE id=?1 AND status=?3 AND updated_at=?4 AND worker_pid IS ?5",
                params![id, timestamp, &job.status, &job.updated_at, job.worker_pid],
            )?;
        }
    }
    Ok(())
}

pub fn remove(db: &Connection, model_id: &str) -> Result<()> {
    let spec = spec(model_id)?;
    remove_in(db, spec, &models_dir())
}

fn remove_in(db: &Connection, spec: ModelSpec, models_dir: &Path) -> Result<()> {
    if active_job(db, spec.id)?.is_some() {
        bail!("请先取消正在进行的模型下载")
    }
    let target = target_path_in(models_dir, spec);
    let partial = partial_path_in(models_dir, spec);
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

fn run_worker(job_id: &str, model_id: &str) -> Result<()> {
    if let Some(delay) = env::var("SIAOCUT_MODEL_START_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        thread::sleep(Duration::from_millis(delay));
    }
    let db = db::open()?;
    run_download_attempt(&db, job_id, spec(model_id)?, &models_dir())
}

/// Run the synchronous reqwest worker on a thread that has never entered Tokio.
///
/// `reqwest::blocking::Client` owns an internal async runtime. Creating or
/// dropping it on a Tokio runtime thread can panic, so the model worker keeps
/// its complete blocking-client lifecycle on this dedicated OS thread.
pub fn run_worker_isolated(job_id: &str, model_id: &str) -> Result<()> {
    let job_id = job_id.to_owned();
    let model_id = model_id.to_owned();
    run_on_blocking_thread(move || run_worker(&job_id, &model_id))
}

fn run_on_blocking_thread(work: impl FnOnce() -> Result<()> + Send + 'static) -> Result<()> {
    thread::Builder::new()
        .name("siaocut-model-worker".into())
        .spawn(work)
        .context("无法启动隔离的模型下载线程")?
        .join()
        .map_err(|_| anyhow!("模型下载 Worker 线程异常退出"))?
}

fn run_download_attempt(
    db: &Connection,
    job_id: &str,
    spec: ModelSpec,
    models_dir: &Path,
) -> Result<()> {
    let result = download_in(db, job_id, spec, models_dir);
    if let Err(error) = &result {
        let timestamp = now();
        let _ = db.execute(
            "UPDATE model_downloads SET status='failed',error_message=?2,worker_pid=NULL,updated_at=?3,completed_at=?3 WHERE id=?1 AND status!='cancelled'",
            params![job_id, error.to_string(), timestamp],
        );
    }
    result
}

fn download_in(db: &Connection, job_id: &str, spec: ModelSpec, models_dir: &Path) -> Result<()> {
    let initial = load_job(db, job_id)?;
    if initial.model_id != spec.id {
        bail!("模型下载任务与模型不匹配")
    }
    if initial.cancel_requested_at.is_some() {
        finish_cancelled(db, job_id)?;
        return Ok(());
    }
    let partial = partial_path_in(models_dir, spec);
    let target = target_path_in(models_dir, spec);
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
    use sha2::{Digest, Sha256};
    use std::{
        net::{TcpListener, TcpStream},
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
    };
    use tempfile::tempdir;

    #[derive(Clone, Copy)]
    struct ResponsePlan {
        max_bytes: Option<usize>,
        chunk_size: usize,
        chunk_delay: Duration,
    }

    impl ResponsePlan {
        fn complete() -> Self {
            Self {
                max_bytes: None,
                chunk_size: 64 * 1024,
                chunk_delay: Duration::ZERO,
            }
        }
    }

    struct MockModelServer {
        url: &'static str,
        range_starts: Arc<Mutex<Vec<u64>>>,
        thread: thread::JoinHandle<()>,
    }

    impl MockModelServer {
        fn start(payload: Vec<u8>, plans: Vec<ResponsePlan>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let address = listener.local_addr().unwrap();
            let url = Box::leak(format!("http://{address}/model.bin").into_boxed_str());
            let range_starts = Arc::new(Mutex::new(Vec::new()));
            let observed_ranges = Arc::clone(&range_starts);
            let thread = thread::spawn(move || {
                for plan in plans {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                assert!(
                                    Instant::now() < deadline,
                                    "timed out waiting for model download request"
                                );
                                thread::sleep(Duration::from_millis(10));
                            }
                            Err(error) => panic!("mock model server accept failed: {error}"),
                        }
                    };
                    stream.set_nonblocking(false).unwrap();
                    let range_start = read_range_start(&mut stream);
                    observed_ranges.lock().unwrap().push(range_start);
                    write_model_response(&mut stream, &payload, range_start, plan);
                }
            });
            Self {
                url,
                range_starts,
                thread,
            }
        }

        fn finish(self) -> Vec<u64> {
            self.thread.join().unwrap();
            self.range_starts.lock().unwrap().clone()
        }
    }

    fn read_range_start(stream: &mut TcpStream) -> u64 {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0, "model request ended before headers");
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

    fn write_model_response(
        stream: &mut TcpStream,
        payload: &[u8],
        range_start: u64,
        plan: ResponsePlan,
    ) {
        let start = usize::try_from(range_start).unwrap();
        assert!(start <= payload.len());
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
        let headers = format!(
            "{status}\r\nContent-Length: {remaining}\r\nAccept-Ranges: bytes\r\n{content_range}Connection: close\r\n\r\n"
        );
        stream.write_all(headers.as_bytes()).unwrap();
        let send_bytes = plan.max_bytes.unwrap_or(remaining).min(remaining);
        for chunk in payload[start..start + send_bytes].chunks(plan.chunk_size) {
            if stream.write_all(chunk).is_err() {
                break;
            }
            if stream.flush().is_err() {
                break;
            }
            if !plan.chunk_delay.is_zero() {
                thread::sleep(plan.chunk_delay);
            }
        }
    }

    fn test_spec(url: &'static str, payload: &[u8]) -> ModelSpec {
        let sha256 = Box::leak(format!("{:x}", Sha256::digest(payload)).into_boxed_str());
        ModelSpec {
            id: "test-model",
            name: "Test model",
            file_name: "test-model.bin",
            description: "Local model download fixture",
            source: "local test server",
            url,
            size: payload.len() as u64,
            sha256,
            license: "MIT",
            recommended: false,
        }
    }

    fn create_test_job(db: &Connection, spec: ModelSpec, models_dir: &Path) -> ModelDownloadJob {
        create_download_in(db, spec, models_dir, |_, _| Ok(())).unwrap()
    }

    fn wait_for_job(
        db: &Connection,
        job_id: &str,
        predicate: impl Fn(&ModelDownloadJob) -> bool,
    ) -> ModelDownloadJob {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let job = load_job(db, job_id).unwrap();
            if predicate(&job) {
                return job;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {job_id}");
            thread::sleep(Duration::from_millis(20));
        }
    }

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn blocking_client_lifecycle_is_isolated_from_tokio_runtime() {
        tokio::task::yield_now().await;
        assert!(tokio::runtime::Handle::try_current().is_ok());

        run_on_blocking_thread(|| {
            assert!(tokio::runtime::Handle::try_current().is_err());
            let client = Client::builder().build()?;
            drop(client);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn concurrent_create_reuses_job_committed_by_lock_holder() {
        let temp = tempdir().unwrap();
        let database_path = temp.path().join("concurrent-models.db");
        let lock_holder = db::open_at(&database_path).unwrap();
        let concurrent = db::open_at(&database_path).unwrap();
        let models_dir = temp.path().join("models");
        fs::create_dir_all(&models_dir).unwrap();
        let spec = ModelSpec {
            id: "test-model",
            name: "Test model",
            file_name: "test-model.bin",
            description: "Atomic create fixture",
            source: "test",
            url: "http://127.0.0.1/unused",
            size: 1024,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            license: "MIT",
            recommended: false,
        };
        let tx = Transaction::new_unchecked(&lock_holder, TransactionBehavior::Immediate).unwrap();
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let concurrent_spawn_count = Arc::clone(&spawn_count);
        let (started_tx, started_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let concurrent_models_dir = models_dir.clone();
        let handle = thread::spawn(move || {
            started_tx.send(()).unwrap();
            let result = create_download_in(&concurrent, spec, &concurrent_models_dir, |_, _| {
                concurrent_spawn_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
            result_tx.send(result).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(
            result_rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "concurrent creator bypassed the immediate transaction"
        );

        let timestamp = now();
        tx.execute(
            "INSERT INTO model_downloads(id,model_id,status,progress,bytes_downloaded,total_bytes,target_path,created_at,updated_at) VALUES('m-lock-holder',?1,'queued',0,0,?2,?3,?4,?4)",
            params![
                spec.id,
                spec.size,
                target_path_in(&models_dir, spec).to_string_lossy(),
                timestamp
            ],
        )
        .unwrap();
        tx.commit().unwrap();
        let returned = result_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        handle.join().unwrap();

        assert_eq!(returned.id, "m-lock-holder");
        assert_eq!(spawn_count.load(Ordering::SeqCst), 0);
        let db = db::open_at(&database_path).unwrap();
        let active_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM model_downloads WHERE model_id=?1 AND status IN ('queued','running')",
                [spec.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active_count, 1);
    }

    #[test]
    fn cancellation_preserves_partial_then_range_resume_installs_and_removes() {
        let payload = (0..2 * 1024 * 1024)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let server = MockModelServer::start(
            payload.clone(),
            vec![
                ResponsePlan {
                    max_bytes: None,
                    chunk_size: 32 * 1024,
                    chunk_delay: Duration::from_millis(35),
                },
                ResponsePlan::complete(),
            ],
        );
        let spec = test_spec(server.url, &payload);
        let temp = tempdir().unwrap();
        let database_path = temp.path().join("cancel-resume.db");
        let models_dir = temp.path().join("models");
        let db = db::open_at(&database_path).unwrap();
        let first = create_test_job(&db, spec, &models_dir);
        let worker_database_path = database_path.clone();
        let worker_models_dir = models_dir.clone();
        let first_id = first.id.clone();
        let worker = thread::spawn(move || {
            run_on_blocking_thread(move || {
                let db = db::open_at(&worker_database_path)?;
                run_download_attempt(&db, &first_id, spec, &worker_models_dir)
            })
        });

        wait_for_job(&db, &first.id, |job| {
            job.status == "running" && job.bytes_downloaded > 0
        });
        cancel(&db, &first.id).unwrap();
        worker.join().unwrap().unwrap();
        let cancelled = load_job(&db, &first.id).unwrap();
        assert_eq!(cancelled.status, "cancelled");
        let partial = partial_path_in(&models_dir, spec);
        let partial_bytes = fs::metadata(&partial).unwrap().len();
        assert!(partial_bytes > 0);
        assert!(partial_bytes < spec.size);

        let resumed = create_test_job(&db, spec, &models_dir);
        assert_eq!(resumed.bytes_downloaded, partial_bytes);
        let resumed_id = resumed.id.clone();
        let resumed_database_path = database_path.clone();
        let resumed_models_dir = models_dir.clone();
        run_on_blocking_thread(move || {
            let db = db::open_at(&resumed_database_path)?;
            run_download_attempt(&db, &resumed_id, spec, &resumed_models_dir)
        })
        .unwrap();

        let completed = load_job(&db, &resumed.id).unwrap();
        assert_eq!(completed.status, "completed");
        assert_eq!(completed.progress, 1.0);
        assert_eq!(
            model_status(spec, &models_dir, true).unwrap().verified,
            Some(true)
        );
        let ranges = server.finish();
        assert_eq!(ranges[0], 0);
        assert_eq!(ranges[1], partial_bytes);

        remove_in(&db, spec, &models_dir).unwrap();
        assert!(!target_path_in(&models_dir, spec).exists());
        assert!(!partial.exists());
    }

    #[test]
    fn failed_download_is_recorded_and_retry_resumes_partial() {
        let payload = (0..512 * 1024)
            .map(|index| (index % 239) as u8)
            .collect::<Vec<_>>();
        let first_attempt_bytes = 96 * 1024;
        let server = MockModelServer::start(
            payload.clone(),
            vec![
                ResponsePlan {
                    max_bytes: Some(first_attempt_bytes),
                    chunk_size: 16 * 1024,
                    chunk_delay: Duration::ZERO,
                },
                ResponsePlan::complete(),
            ],
        );
        let spec = test_spec(server.url, &payload);
        let temp = tempdir().unwrap();
        let database_path = temp.path().join("failure-recovery.db");
        let models_dir = temp.path().join("models");
        let db = db::open_at(&database_path).unwrap();
        let failed = create_test_job(&db, spec, &models_dir);
        let failed_id = failed.id.clone();
        let first_database_path = database_path.clone();
        let first_models_dir = models_dir.clone();
        let error = run_on_blocking_thread(move || {
            let db = db::open_at(&first_database_path)?;
            run_download_attempt(&db, &failed_id, spec, &first_models_dir)
        })
        .unwrap_err();
        assert!(
            error.to_string().contains("不完整")
                || error.to_string().contains("request or response body error")
        );
        let failed = load_job(&db, &failed.id).unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed.error_message.is_some());
        assert!(failed.completed_at.is_some());
        assert_eq!(failed.worker_pid, None);
        let partial_bytes = fs::metadata(partial_path_in(&models_dir, spec))
            .unwrap()
            .len();
        assert_eq!(partial_bytes, first_attempt_bytes as u64);

        let retry = create_test_job(&db, spec, &models_dir);
        assert_eq!(retry.bytes_downloaded, partial_bytes);
        let retry_id = retry.id.clone();
        let retry_database_path = database_path.clone();
        let retry_models_dir = models_dir.clone();
        run_on_blocking_thread(move || {
            let db = db::open_at(&retry_database_path)?;
            run_download_attempt(&db, &retry_id, spec, &retry_models_dir)
        })
        .unwrap();
        assert_eq!(load_job(&db, &retry.id).unwrap().status, "completed");
        let ranges = server.finish();
        assert_eq!(ranges, vec![0, partial_bytes]);
    }
}
