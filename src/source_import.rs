use crate::{
    db::{self, home_dir},
    media::{ffprobe_duration, hash_file, tool_path},
    project,
    util::{hidden_command, new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{StatusCode, Url, header};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env, fs,
    io::{BufRead, BufReader},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs},
    path::{Path, PathBuf},
    process::Stdio,
    sync::mpsc,
    thread,
    time::Duration,
};

pub const MAX_DURATION_SECONDS: f64 = 2.0 * 60.0 * 60.0;
pub const MAX_FILE_SIZE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const PINNED_YTDLP_VERSION: &str = "2026.06.09";
pub const PINNED_YTDLP_SHA256: &str =
    "3a48cb955d55c8821b60ccbdbbc6f61bc958f2f3d3b7ad5eaf3d83a543293a27";
const MAX_REDIRECTS: usize = 8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePreview {
    pub original_url: String,
    pub webpage_url: String,
    pub site_media_id: String,
    pub extractor: String,
    pub title: String,
    pub duration_seconds: f64,
    pub file_size_bytes: Option<u64>,
    pub file_size_known: bool,
    pub thumbnail_url: Option<String>,
    pub tool_version: String,
    pub tool_sha256: String,
    pub requires_confirmation: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceImportJob {
    pub id: String,
    pub project_id: Option<String>,
    pub original_url: String,
    pub webpage_url: String,
    pub site_media_id: String,
    pub extractor: String,
    pub title: String,
    pub duration_seconds: f64,
    pub file_size_bytes: Option<u64>,
    pub status: String,
    pub progress: f64,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub output_directory: String,
    pub output_path: Option<String>,
    pub output_sha256: Option<String>,
    pub tool_version: String,
    pub tool_sha256: String,
    pub cancel_requested_at: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub worker_pid: Option<u32>,
    pub attempt_count: u32,
}

#[derive(Clone, Debug)]
struct ToolIdentity {
    path: PathBuf,
    version: String,
    sha256: String,
}

pub fn yt_dlp_path() -> PathBuf {
    if let Some(path) = env::var_os("SIAOCUT_YTDLP") {
        return PathBuf::from(path);
    }
    let bundled = home_dir().join("bin").join("yt-dlp.exe");
    if bundled.is_file() {
        bundled
    } else {
        PathBuf::from("yt-dlp.exe")
    }
}

pub fn configured() -> bool {
    yt_dlp_path().is_file()
}

pub fn inspect(input: &str) -> Result<SourcePreview> {
    let original = validate_public_https_url(input)?;
    preflight_public_url(&original)?;
    let tool = verify_tool(&yt_dlp_path())?;
    let output = hidden_command(&tool.path)
        .args(inspection_arguments(&original))
        .output()
        .context("无法启动固定版本的 yt-dlp")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        bail!(
            "source_inspection_failed: yt-dlp 无法读取此公开单视频：{}",
            detail.trim()
        )
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)
        .context("source_metadata_invalid: yt-dlp 返回了无效 JSON")?;
    parse_metadata(original, &metadata, &tool)
}

pub fn start(
    db: &Connection,
    input: &str,
    confirmed_media_id: &str,
    start_delay_ms: Option<u64>,
) -> Result<SourceImportJob> {
    start_internal(db, input, confirmed_media_id, start_delay_ms, None)
}

pub(crate) fn start_with_job_id(
    db: &Connection,
    input: &str,
    confirmed_media_id: &str,
    start_delay_ms: Option<u64>,
    job_id: &str,
) -> Result<SourceImportJob> {
    if let Ok(existing) = load(db, job_id) {
        return Ok(existing);
    }
    start_internal(db, input, confirmed_media_id, start_delay_ms, Some(job_id))
}

fn start_internal(
    db: &Connection,
    input: &str,
    confirmed_media_id: &str,
    start_delay_ms: Option<u64>,
    job_id: Option<&str>,
) -> Result<SourceImportJob> {
    let preview = inspect(input)?;
    if preview.site_media_id != confirmed_media_id {
        bail!(
            "source_confirmation_mismatch: 当前站点媒体 ID 为 {}，与确认值不一致",
            preview.site_media_id
        )
    }
    if job_id.is_none()
        && let Some(job) = active_job(db, &preview.original_url)?
    {
        return Ok(job);
    }
    let job = insert_job_with_id(db, &preview, job_id)?;
    if let Err(error) = spawn_worker(&job.id, start_delay_ms) {
        let timestamp = now();
        let _ = db.execute(
            "UPDATE source_imports SET status='failed',error_message=?2,updated_at=?3,completed_at=?3 WHERE id=?1",
            params![&job.id, error.to_string(), timestamp],
        );
        return Err(error);
    }
    Ok(job)
}

fn insert_job_with_id(
    db: &Connection,
    preview: &SourcePreview,
    job_id: Option<&str>,
) -> Result<SourceImportJob> {
    insert_job_with_id_at(db, preview, &home_dir().join("imports"), job_id)
}

#[cfg(test)]
fn insert_job_at(
    db: &Connection,
    preview: &SourcePreview,
    imports_directory: &Path,
) -> Result<SourceImportJob> {
    insert_job_with_id_at(db, preview, imports_directory, None)
}

fn insert_job_with_id_at(
    db: &Connection,
    preview: &SourcePreview,
    imports_directory: &Path,
    job_id: Option<&str>,
) -> Result<SourceImportJob> {
    let id = job_id.map(str::to_owned).unwrap_or_else(|| new_id("src"));
    let output_directory = imports_directory.join(&id);
    fs::create_dir_all(&output_directory)?;
    let reserve = 128 * 1024 * 1024_u64;
    let required = preview
        .file_size_bytes
        .unwrap_or(MAX_FILE_SIZE_BYTES)
        .saturating_add(reserve);
    if crate::util::available_space(&output_directory)? < required {
        bail!(
            "disk_space_low: URL 导入最多仍需 {:.1} GB，可用空间不足",
            required as f64 / 1_073_741_824.0
        )
    }
    let timestamp = now();
    let job = SourceImportJob {
        id,
        project_id: None,
        original_url: preview.original_url.clone(),
        webpage_url: preview.webpage_url.clone(),
        site_media_id: preview.site_media_id.clone(),
        extractor: preview.extractor.clone(),
        title: preview.title.clone(),
        duration_seconds: preview.duration_seconds,
        file_size_bytes: preview.file_size_bytes,
        status: "queued".into(),
        progress: 0.0,
        bytes_downloaded: 0,
        total_bytes: preview.file_size_bytes,
        output_directory: output_directory
            .canonicalize()?
            .to_string_lossy()
            .to_string(),
        output_path: None,
        output_sha256: None,
        tool_version: preview.tool_version.clone(),
        tool_sha256: preview.tool_sha256.clone(),
        cancel_requested_at: None,
        error_message: None,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        completed_at: None,
        worker_pid: None,
        attempt_count: 1,
    };
    db.execute(
        "INSERT INTO source_imports(
             id,original_url,webpage_url,site_media_id,extractor,title,duration_seconds,
             file_size_bytes,status,progress,bytes_downloaded,total_bytes,output_directory,
             tool_version,tool_sha256,created_at,updated_at,attempt_count
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'queued',0,0,?8,?9,?10,?11,?12,?12,1)",
        params![
            &job.id,
            &job.original_url,
            &job.webpage_url,
            &job.site_media_id,
            &job.extractor,
            &job.title,
            job.duration_seconds,
            job.file_size_bytes,
            &job.output_directory,
            &job.tool_version,
            &job.tool_sha256,
            &job.created_at,
        ],
    )?;
    Ok(job)
}

fn active_job(db: &Connection, original_url: &str) -> Result<Option<SourceImportJob>> {
    let id = db
        .query_row(
        "SELECT id FROM source_imports WHERE original_url=?1 AND status IN ('queued','running','finalizing') ORDER BY created_at DESC LIMIT 1",
            [original_url],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    id.map(|id| load(db, &id)).transpose()
}

pub fn load(db: &Connection, job_id: &str) -> Result<SourceImportJob> {
    db.query_row(
        "SELECT id,project_id,original_url,webpage_url,site_media_id,extractor,title,
                duration_seconds,file_size_bytes,status,progress,bytes_downloaded,total_bytes,
                output_directory,output_path,output_sha256,tool_version,tool_sha256,
                cancel_requested_at,error_message,created_at,updated_at,completed_at,worker_pid,attempt_count
         FROM source_imports WHERE id=?1",
        [job_id],
        |row| {
            Ok(SourceImportJob {
                id: row.get(0)?,
                project_id: row.get(1)?,
                original_url: row.get(2)?,
                webpage_url: row.get(3)?,
                site_media_id: row.get(4)?,
                extractor: row.get(5)?,
                title: row.get(6)?,
                duration_seconds: row.get(7)?,
                file_size_bytes: row.get(8)?,
                status: row.get(9)?,
                progress: row.get(10)?,
                bytes_downloaded: row.get(11)?,
                total_bytes: row.get(12)?,
                output_directory: row.get(13)?,
                output_path: row.get(14)?,
                output_sha256: row.get(15)?,
                tool_version: row.get(16)?,
                tool_sha256: row.get(17)?,
                cancel_requested_at: row.get(18)?,
                error_message: row.get(19)?,
                created_at: row.get(20)?,
                updated_at: row.get(21)?,
                completed_at: row.get(22)?,
                worker_pid: row.get(23)?,
                attempt_count: row.get::<_, i64>(24)? as u32,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("source_job_not_found: URL 导入任务不存在：{job_id}"))
}

pub fn list(db: &Connection) -> Result<Vec<SourceImportJob>> {
    db.prepare("SELECT id FROM source_imports ORDER BY created_at DESC")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| load(db, &id))
        .collect()
}

pub fn cancel(db: &Connection, job_id: &str) -> Result<SourceImportJob> {
    let changed = db.execute(
        "UPDATE source_imports SET cancel_requested_at=?2,updated_at=?2 WHERE id=?1 AND status IN ('queued','running')",
        params![job_id, now()],
    )?;
    if changed == 0 {
        bail!(
            "source_job_not_cancellable: URL 导入任务当前状态不能取消：{}",
            load(db, job_id)?.status
        )
    }
    load(db, job_id)
}

pub fn resume(db: &Connection, job_id: &str) -> Result<SourceImportJob> {
    let job = prepare_resume(db, job_id)?;
    if let Err(error) = spawn_worker(job_id, None) {
        let timestamp = now();
        let _ = db.execute(
            "UPDATE source_imports SET status='failed',error_message=?2,updated_at=?3,completed_at=?3 WHERE id=?1",
            params![job_id, error.to_string(), timestamp],
        );
        return Err(error);
    }
    Ok(job)
}

fn prepare_resume(db: &Connection, job_id: &str) -> Result<SourceImportJob> {
    let job = load(db, job_id)?;
    if !["cancelled", "failed", "interrupted"].contains(&job.status.as_str()) {
        bail!(
            "source_job_not_resumable: URL 导入任务当前状态不能继续：{}",
            job.status
        )
    }
    let partial_bytes = partial_bytes(Path::new(&job.output_directory))?;
    let progress = job
        .total_bytes
        .filter(|total| *total > 0)
        .map(|total| partial_bytes as f64 / total as f64)
        .unwrap_or(0.0)
        .clamp(0.0, 0.99);
    db.execute(
        "UPDATE source_imports
         SET status='queued',progress=?2,bytes_downloaded=?3,cancel_requested_at=NULL,
             error_message=NULL,completed_at=NULL,worker_pid=NULL,attempt_count=attempt_count+1,updated_at=?4
         WHERE id=?1",
        params![job_id, progress, partial_bytes, now()],
    )?;
    load(db, job_id)
}

pub fn reconcile_interrupted(db: &Connection) -> Result<()> {
    let jobs = db
        .prepare("SELECT id FROM source_imports WHERE status IN ('queued','running','finalizing')")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in jobs {
        let job = load(db, &id)?;
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
                "UPDATE source_imports
                 SET status='interrupted',error_message='上次 URL 下载进程已中断；需要显式继续。',worker_pid=NULL,updated_at=?2
                 WHERE id=?1",
                params![id, now()],
            )?;
        }
    }
    Ok(())
}

fn spawn_worker(job_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let mut arguments = vec!["__source_worker", job_id];
    let delay = start_delay_ms.map(|value| value.to_string());
    if let Some(delay) = delay.as_deref() {
        arguments.push(delay);
    }
    crate::util::spawn_detached_current(&arguments).context("无法启动 URL 导入任务")?;
    Ok(())
}

pub fn run_worker(job_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let mut db = db::open()?;
    db.execute(
        "UPDATE source_imports SET status='running',worker_pid=?2,updated_at=?3 WHERE id=?1",
        params![job_id, std::process::id(), now()],
    )?;
    if let Some(delay) = start_delay_ms {
        thread::sleep(Duration::from_millis(delay));
    }
    let result = run_download(&mut db, job_id);
    if let Err(error) = &result {
        let timestamp = now();
        let _ = db.execute(
            "UPDATE source_imports
             SET status='failed',error_message=?2,worker_pid=NULL,updated_at=?3,completed_at=?3
             WHERE id=?1 AND status NOT IN ('cancelled','completed')",
            params![job_id, error.to_string(), timestamp],
        );
    }
    result
}

#[derive(Debug)]
struct WorkerLine {
    stderr: bool,
    text: String,
}

fn run_download(db: &mut Connection, job_id: &str) -> Result<()> {
    let job = load(db, job_id)?;
    if job.cancel_requested_at.is_some() {
        finish_cancelled(db, job_id)?;
        return Ok(());
    }
    let original_url = validate_public_https_url(&job.original_url)?;
    preflight_public_url(&original_url)?;
    let tool = verify_tool(&yt_dlp_path())?;
    if tool.version != job.tool_version || tool.sha256 != job.tool_sha256 {
        bail!("source_tool_changed: URL 导入任务绑定的 yt-dlp 版本或哈希已变化")
    }
    let ffmpeg = tool_path("SIAOCUT_FFMPEG", "ffmpeg");
    run_download_command(db, job_id, &original_url, &tool, &ffmpeg)
}

fn run_download_command(
    db: &mut Connection,
    job_id: &str,
    original_url: &Url,
    tool: &ToolIdentity,
    ffmpeg: &str,
) -> Result<()> {
    let job = load(db, job_id)?;
    let output_directory = PathBuf::from(&job.output_directory);
    fs::create_dir_all(&output_directory)?;
    let mut command = hidden_command(&tool.path);
    command
        .args(download_arguments(original_url, &output_directory, ffmpeg))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("无法启动 yt-dlp 下载进程")?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("无法读取 yt-dlp 输出"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("无法读取 yt-dlp 错误"))?;
    let (line_tx, line_rx) = mpsc::channel();
    let stdout_tx = line_tx.clone();
    let stdout_reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = stdout_tx.send(WorkerLine {
                stderr: false,
                text: line,
            });
        }
    });
    let stderr_reader = thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = line_tx.send(WorkerLine {
                stderr: true,
                text: line,
            });
        }
    });
    let mut output_path = None;
    let mut error_lines = Vec::new();
    let status = loop {
        drain_worker_lines(db, job_id, &line_rx, &mut output_path, &mut error_lines)?;
        let cancel_requested: bool = db.query_row(
            "SELECT cancel_requested_at IS NOT NULL FROM source_imports WHERE id=?1",
            [job_id],
            |row| row.get(0),
        )?;
        if cancel_requested {
            crate::util::terminate_process_tree(&mut child);
            break None;
        }
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        thread::sleep(Duration::from_millis(200));
    };
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    drain_worker_lines(db, job_id, &line_rx, &mut output_path, &mut error_lines)?;
    if status.is_none() {
        finish_cancelled(db, job_id)?;
        return Ok(());
    }
    if !status.is_some_and(|status| status.success()) {
        bail!(
            "source_download_failed: yt-dlp 下载失败：{}",
            error_lines.join("\n").trim()
        )
    }
    let output_path = match output_path {
        Some(path) => PathBuf::from(path),
        None => find_completed_output(&output_directory)?,
    };
    finalize_download(db, job_id, &output_path)?;
    Ok(())
}

fn download_arguments(url: &Url, output_directory: &Path, ffmpeg: &str) -> Vec<String> {
    let mut arguments = vec![
        "--ignore-config".into(),
        "--no-plugin-dirs".into(),
        "--no-playlist".into(),
        "--no-cache-dir".into(),
        "--continue".into(),
        "--part".into(),
        "--no-overwrites".into(),
        "--newline".into(),
        "--socket-timeout".into(),
        "30".into(),
        "--retries".into(),
        "3".into(),
        "--fragment-retries".into(),
        "3".into(),
        "--max-filesize".into(),
        MAX_FILE_SIZE_BYTES.to_string(),
        "--format".into(),
        "bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b".into(),
        "--merge-output-format".into(),
        "mp4".into(),
        "--remux-video".into(),
        "mp4".into(),
        "--progress-template".into(),
        "download:__SIAOCUT_PROGRESS__%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s".into(),
        "--print".into(),
        "after_move:__SIAOCUT_FILE__%(filepath)s".into(),
        "--paths".into(),
        output_directory.to_string_lossy().to_string(),
        "--output".into(),
        "source.%(ext)s".into(),
    ];
    let ffmpeg_path = Path::new(ffmpeg);
    if ffmpeg_path.is_file()
        && let Some(parent) = ffmpeg_path.parent()
    {
        arguments.push("--ffmpeg-location".into());
        arguments.push(parent.to_string_lossy().to_string());
    }
    arguments.push("--".into());
    arguments.push(url.as_str().into());
    arguments
}

fn drain_worker_lines(
    db: &Connection,
    job_id: &str,
    receiver: &mpsc::Receiver<WorkerLine>,
    output_path: &mut Option<String>,
    error_lines: &mut Vec<String>,
) -> Result<()> {
    while let Ok(line) = receiver.try_recv() {
        if let Some((_, value)) = line.text.split_once("__SIAOCUT_FILE__") {
            *output_path = Some(value.trim().to_owned());
        }
        if let Some((_, value)) = line.text.split_once("__SIAOCUT_PROGRESS__") {
            let parts = value.split('|').collect::<Vec<_>>();
            let downloaded = parts.first().and_then(|value| value.parse::<u64>().ok());
            let total = parts
                .get(1)
                .and_then(|value| value.parse::<u64>().ok())
                .or_else(|| parts.get(2).and_then(|value| value.parse::<u64>().ok()));
            if let Some(downloaded) = downloaded {
                let recorded_total =
                    total.or_else(|| load(db, job_id).ok().and_then(|job| job.total_bytes));
                let progress = recorded_total
                    .filter(|total| *total > 0)
                    .map(|total| downloaded as f64 / total as f64)
                    .unwrap_or(0.01)
                    .clamp(0.01, 0.99);
                db.execute(
                    "UPDATE source_imports
                     SET bytes_downloaded=?2,total_bytes=COALESCE(?3,total_bytes),progress=?4,updated_at=?5
                     WHERE id=?1",
                    params![job_id, downloaded, recorded_total, progress, now()],
                )?;
            }
        } else if line.stderr && !line.text.trim().is_empty() {
            error_lines.push(line.text);
            if error_lines.len() > 40 {
                error_lines.remove(0);
            }
        }
    }
    Ok(())
}

fn find_completed_output(directory: &Path) -> Result<PathBuf> {
    let candidates = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_file()
                && path.extension().and_then(|value| value.to_str()) == Some("mp4")
                && !path.to_string_lossy().ends_with(".part")
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        bail!("source_output_invalid: 下载完成后未找到唯一 MP4 文件")
    }
    Ok(candidates[0].clone())
}

fn finalize_download(db: &mut Connection, job_id: &str, output: &Path) -> Result<()> {
    let job = load(db, job_id)?;
    let output = output
        .canonicalize()
        .context("source_output_invalid: 无法读取下载结果")?;
    let directory = PathBuf::from(&job.output_directory).canonicalize()?;
    if !output.starts_with(&directory)
        || output.extension().and_then(|value| value.to_str()) != Some("mp4")
    {
        bail!("source_output_invalid: 下载结果不在受控目录或不是 MP4")
    }
    let bytes = fs::metadata(&output)?.len();
    if bytes > MAX_FILE_SIZE_BYTES {
        bail!("source_size_limit: 下载结果超过 4 GB")
    }
    let duration = ffprobe_duration(&output)
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .ok_or_else(|| anyhow!("source_media_probe_failed: 无法验证下载结果的媒体时长"))?;
    if duration > MAX_DURATION_SECONDS {
        bail!("source_duration_limit: 下载结果超过 2 小时")
    }
    let output_sha256 = hash_file(&output)?;
    db.execute(
        "UPDATE source_imports
         SET status='finalizing',progress=0.99,bytes_downloaded=?2,total_bytes=?2,
             output_path=?3,output_sha256=?4,updated_at=?5
         WHERE id=?1",
        params![
            job_id,
            bytes,
            output.to_string_lossy(),
            &output_sha256,
            now()
        ],
    )?;
    let project = project::create(db, &output, Some(job.title))?;
    let completed_at = now();
    db.execute(
        "UPDATE source_imports
         SET project_id=?2,status='completed',progress=1,worker_pid=NULL,updated_at=?3,completed_at=?3,error_message=NULL
         WHERE id=?1",
        params![job_id, &project.id, completed_at],
    )?;
    Ok(())
}

fn finish_cancelled(db: &Connection, job_id: &str) -> Result<()> {
    let timestamp = now();
    db.execute(
        "UPDATE source_imports
         SET status='cancelled',worker_pid=NULL,updated_at=?2,completed_at=?2
         WHERE id=?1",
        params![job_id, timestamp],
    )?;
    Ok(())
}

fn partial_bytes(directory: &Path) -> Result<u64> {
    if !directory.is_dir() {
        return Ok(0);
    }
    Ok(fs::read_dir(directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file() && path.to_string_lossy().ends_with(".part"))
        .filter_map(|path| fs::metadata(path).ok().map(|metadata| metadata.len()))
        .sum())
}

fn inspection_arguments(url: &Url) -> Vec<String> {
    [
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-playlist",
        "--dump-single-json",
        "--skip-download",
        "--no-warnings",
        "--",
        url.as_str(),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn verify_tool(path: &Path) -> Result<ToolIdentity> {
    if !path.is_file() {
        bail!(
            "source_tool_not_configured: 未找到固定版本的 yt-dlp：{}",
            path.display()
        )
    }
    let expected_version =
        env::var("SIAOCUT_YTDLP_VERSION").unwrap_or_else(|_| PINNED_YTDLP_VERSION.to_owned());
    let expected_sha256 = env::var("SIAOCUT_YTDLP_SHA256")
        .unwrap_or_else(|_| PINNED_YTDLP_SHA256.to_owned())
        .to_lowercase();
    let actual_sha256 = hash_file(path)?;
    if actual_sha256 != expected_sha256 {
        bail!(
            "source_tool_hash_mismatch: yt-dlp SHA-256 不匹配；需要 {}，实际为 {}",
            expected_sha256,
            actual_sha256
        )
    }
    let output = hidden_command(path)
        .arg("--version")
        .output()
        .context("无法读取 yt-dlp 版本")?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || version != expected_version {
        bail!(
            "source_tool_version_mismatch: yt-dlp 版本需要 {}，实际为 {}",
            expected_version,
            if version.is_empty() {
                "unknown"
            } else {
                &version
            }
        )
    }
    Ok(ToolIdentity {
        path: path.to_path_buf(),
        version,
        sha256: actual_sha256,
    })
}

fn preflight_public_url(url: &Url) -> Result<Url> {
    let url = url.clone();
    thread::spawn(move || preflight_public_url_blocking(&url))
        .join()
        .map_err(|_| anyhow!("source_preflight_failed: URL 安全预检异常终止"))?
}

fn preflight_public_url_blocking(url: &Url) -> Result<Url> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(12))
        .build()
        .context("source_preflight_failed: 无法初始化 URL 安全预检")?;
    preflight_url_with(&client, url, validate_public_https_url)
}

fn preflight_url_with<F>(
    client: &reqwest::blocking::Client,
    url: &Url,
    validate_redirect: F,
) -> Result<Url>
where
    F: Fn(&str) -> Result<Url>,
{
    let mut current = url.clone();
    for _ in 0..=MAX_REDIRECTS {
        let mut response = client
            .head(current.clone())
            .header(header::USER_AGENT, "SiaoCut/0.2 URL preflight")
            .send()
            .context("source_preflight_failed: 无法读取 URL 响应头")?;
        if matches!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_IMPLEMENTED
        ) {
            response = client
                .get(current.clone())
                .header(header::USER_AGENT, "SiaoCut/0.2 URL preflight")
                .header(header::RANGE, "bytes=0-0")
                .send()
                .context("source_preflight_failed: 无法读取 URL 响应头")?;
        }
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| anyhow!("source_redirect_invalid: 重定向响应缺少有效 Location"))?;
            let next = current
                .join(location)
                .context("source_redirect_invalid: 重定向目标 URL 无效")?;
            current = validate_redirect(next.as_str())?;
            continue;
        }
        let content_length = response
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| response.content_length());
        if content_length.is_some_and(|size| size > MAX_FILE_SIZE_BYTES) {
            bail!("source_size_limit: URL 响应声明的内容超过 4 GB")
        }
        return Ok(current);
    }
    bail!("source_redirect_limit: URL 重定向次数超过 {MAX_REDIRECTS} 次")
}

fn validate_selected_download_urls(metadata: &Value) -> Result<()> {
    let mut candidates = Vec::new();
    if let Some(url) = metadata.get("url").and_then(Value::as_str) {
        candidates.push(url);
    }
    for key in ["requested_downloads", "requested_formats"] {
        candidates.extend(
            metadata
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|item| item.get("url").and_then(Value::as_str)),
        );
    }
    for candidate in candidates {
        validate_public_https_url(candidate)
            .context("source_selected_url_invalid: 已选媒体地址不是公开 HTTPS URL")?;
    }
    Ok(())
}

fn parse_metadata(
    original_url: Url,
    metadata: &Value,
    tool: &ToolIdentity,
) -> Result<SourcePreview> {
    let source_type = metadata
        .get("_type")
        .and_then(Value::as_str)
        .unwrap_or("video");
    let has_entries = metadata
        .get("entries")
        .and_then(Value::as_array)
        .is_some_and(|entries| !entries.is_empty());
    if source_type != "video" || has_entries {
        bail!("source_playlist_not_allowed: URL 必须指向一个视频，不能是播放列表或合集")
    }
    if metadata
        .get("availability")
        .and_then(Value::as_str)
        .is_some_and(|value| {
            matches!(
                value,
                "private" | "premium_only" | "subscriber_only" | "needs_auth"
            )
        })
    {
        bail!("source_auth_not_allowed: 不支持登录、订阅或私有内容")
    }
    let webpage_url = metadata
        .get("webpage_url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("source_metadata_invalid: 缺少规范页面 URL"))?;
    let webpage_url = validate_public_https_url(webpage_url)?;
    validate_selected_download_urls(metadata)?;
    let duration_seconds = metadata
        .get("duration")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| anyhow!("source_duration_unknown: 无法确认视频时长，已拒绝导入"))?;
    if duration_seconds > MAX_DURATION_SECONDS {
        bail!("source_duration_limit: 单个视频不能超过 2 小时")
    }
    let file_size_bytes = selected_file_size(metadata);
    if file_size_bytes.is_some_and(|size| size > MAX_FILE_SIZE_BYTES) {
        bail!("source_size_limit: 单个视频不能超过 4 GB")
    }
    let site_media_id = required_text(metadata, "id", "站点媒体 ID")?;
    let extractor = metadata
        .get("extractor_key")
        .or_else(|| metadata.get("extractor"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("source_metadata_invalid: 缺少站点提取器"))?
        .to_owned();
    let title = required_text(metadata, "title", "视频标题")?;
    let thumbnail_url = metadata
        .get("thumbnail")
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("https://"))
        .map(str::to_owned);
    Ok(SourcePreview {
        original_url: original_url.to_string(),
        webpage_url: webpage_url.to_string(),
        site_media_id,
        extractor,
        title,
        duration_seconds,
        file_size_bytes,
        file_size_known: file_size_bytes.is_some(),
        thumbnail_url,
        tool_version: tool.version.clone(),
        tool_sha256: tool.sha256.clone(),
        requires_confirmation: true,
    })
}

fn required_text(metadata: &Value, key: &str, label: &str) -> Result<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("source_metadata_invalid: 缺少{label}"))
}

fn selected_file_size(metadata: &Value) -> Option<u64> {
    let direct = metadata
        .get("filesize")
        .or_else(|| metadata.get("filesize_approx"))
        .and_then(Value::as_u64);
    direct.or_else(|| {
        metadata
            .get("requested_downloads")
            .and_then(Value::as_array)
            .and_then(|downloads| {
                downloads.iter().try_fold(0_u64, |total, download| {
                    download
                        .get("filesize")
                        .or_else(|| download.get("filesize_approx"))
                        .and_then(Value::as_u64)
                        .and_then(|size| total.checked_add(size))
                })
            })
    })
}

fn validate_public_https_url(input: &str) -> Result<Url> {
    let url = Url::parse(input).map_err(|_| anyhow!("source_url_invalid: URL 格式无效"))?;
    if url.scheme() != "https" {
        bail!("source_https_required: 只接受公开 HTTPS URL")
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("source_credentials_not_allowed: URL 不能包含用户名或密码")
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("source_url_invalid: URL 缺少主机名"))?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".home.arpa")
    {
        bail!("source_private_network: 已拒绝本机或私网地址")
    }
    let port = url.port_or_known_default().unwrap_or(443);
    if let Ok(ip) = host.parse::<IpAddr>() {
        ensure_public_ip(ip)?;
    } else {
        let addresses = (host.as_str(), port)
            .to_socket_addrs()
            .context("source_dns_failed: 无法解析 URL 主机")?
            .map(|address| address.ip())
            .collect::<Vec<_>>();
        if addresses.is_empty() {
            bail!("source_dns_failed: URL 主机没有可用地址")
        }
        let addresses = if addresses.iter().all(|address| fake_tunnel_ip(*address)) {
            resolve_public_dns_over_https(&host)?
        } else {
            addresses
        };
        for address in addresses {
            ensure_public_ip(address)?;
        }
    }
    Ok(url)
}

fn fake_tunnel_ip(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(ip) if {
        let [a, b, _, _] = ip.octets();
        a == 198 && (b == 18 || b == 19)
    })
}

fn resolve_public_dns_over_https(host: &str) -> Result<Vec<IpAddr>> {
    let host = host.to_owned();
    std::thread::spawn(move || resolve_public_dns_over_https_blocking(&host))
        .join()
        .map_err(|_| anyhow!("source_dns_failed: 公开 DNS 安全检查异常终止"))?
}

fn resolve_public_dns_over_https_blocking(host: &str) -> Result<Vec<IpAddr>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()?;
    let mut addresses = Vec::new();
    for record_type in ["A", "AAAA"] {
        let response = client
            .get("https://cloudflare-dns.com/dns-query")
            .query(&[("name", host), ("type", record_type)])
            .header("accept", "application/dns-json")
            .send()
            .context("source_dns_failed: 无法完成公开 DNS 安全检查")?
            .error_for_status()
            .context("source_dns_failed: 公开 DNS 安全检查失败")?
            .bytes()
            .context("source_dns_failed: 公开 DNS 返回了无效结果")?;
        let response: Value = serde_json::from_slice(&response)
            .context("source_dns_failed: 公开 DNS 返回了无效结果")?;
        if response.get("Status").and_then(Value::as_u64) != Some(0) {
            continue;
        }
        addresses.extend(
            response
                .get("Answer")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|answer| answer.get("data").and_then(Value::as_str))
                .filter_map(|address| address.parse::<IpAddr>().ok()),
        );
    }
    if addresses.is_empty() {
        bail!("source_dns_failed: 无法确认 URL 主机的公开地址")
    }
    Ok(addresses)
}

fn ensure_public_ip(ip: IpAddr) -> Result<()> {
    let public = match ip {
        IpAddr::V4(ip) => public_ipv4(ip),
        IpAddr::V6(ip) => public_ipv6(ip),
    };
    if !public {
        bail!("source_private_network: 已拒绝本机、私网或保留地址")
    }
    Ok(())
}

fn public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(ipv4) = ip.to_ipv4_mapped() {
        return public_ipv4(ipv4);
    }
    let segments = ip.segments();
    !ip.is_unspecified()
        && !ip.is_loopback()
        && !ip.is_multicast()
        && (segments[0] & 0xfe00) != 0xfc00
        && (segments[0] & 0xffc0) != 0xfe80
        && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        time::Instant,
    };
    use tempfile::tempdir;

    struct MockHttpServer {
        address: std::net::SocketAddr,
        range_starts: Arc<Mutex<Vec<u64>>>,
        bytes_served: Arc<AtomicU64>,
        stop: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl MockHttpServer {
        fn start(body: Vec<u8>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let address = listener.local_addr().unwrap();
            let body = Arc::new(body);
            let range_starts = Arc::new(Mutex::new(Vec::new()));
            let recorded_ranges = Arc::clone(&range_starts);
            let bytes_served = Arc::new(AtomicU64::new(0));
            let served_counter = Arc::clone(&bytes_served);
            let stop = Arc::new(AtomicBool::new(false));
            let stop_signal = Arc::clone(&stop);
            let worker = thread::spawn(move || {
                while !stop_signal.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => serve_mock_request(
                            &mut stream,
                            &body,
                            &recorded_ranges,
                            &served_counter,
                        ),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                address,
                range_starts,
                bytes_served,
                stop,
                worker: Some(worker),
            }
        }

        fn url(&self, path: &str) -> Url {
            Url::parse(&format!("http://{}{}", self.address, path)).unwrap()
        }
    }

    impl Drop for MockHttpServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ = TcpStream::connect(self.address);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    fn serve_mock_request(
        stream: &mut TcpStream,
        body: &[u8],
        range_starts: &Arc<Mutex<Vec<u64>>>,
        bytes_served: &Arc<AtomicU64>,
    ) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut request = vec![0_u8; 16 * 1024];
        let Ok(length) = stream.read(&mut request) else {
            return;
        };
        let request = String::from_utf8_lossy(&request[..length]);
        let first = request.lines().next().unwrap_or_default();
        let mut first = first.split_whitespace();
        let method = first.next().unwrap_or_default();
        let path = first.next().unwrap_or_default();
        if path == "/private-redirect" {
            let response = "HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(response.as_bytes());
            return;
        }
        if path == "/oversize" {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_FILE_SIZE_BYTES + 1
            );
            let _ = stream.write_all(response.as_bytes());
            return;
        }
        if path != "/video.mp4" {
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            return;
        }
        let range_start = request.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if !name.eq_ignore_ascii_case("range") {
                return None;
            }
            value
                .trim()
                .strip_prefix("bytes=")?
                .split('-')
                .next()?
                .parse::<u64>()
                .ok()
        });
        if let Some(start) = range_start {
            range_starts.lock().unwrap().push(start);
        }
        let start = range_start.unwrap_or(0).min(body.len() as u64) as usize;
        let remaining = body.len().saturating_sub(start);
        let status = if range_start.is_some() {
            "HTTP/1.1 206 Partial Content"
        } else {
            "HTTP/1.1 200 OK"
        };
        let content_range = range_start
            .map(|_| {
                format!(
                    "Content-Range: bytes {start}-{}/{}\r\n",
                    body.len() - 1,
                    body.len()
                )
            })
            .unwrap_or_default();
        let response = format!(
            "{status}\r\nContent-Type: video/mp4\r\nAccept-Ranges: bytes\r\nContent-Length: {remaining}\r\n{content_range}Connection: close\r\n\r\n"
        );
        if stream.write_all(response.as_bytes()).is_err() || method == "HEAD" {
            return;
        }
        for chunk in body[start..].chunks(32 * 1024) {
            if stream.write_all(chunk).is_err() {
                break;
            }
            bytes_served.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            let _ = stream.flush();
            thread::sleep(Duration::from_millis(30));
        }
    }

    fn tool() -> ToolIdentity {
        ToolIdentity {
            path: PathBuf::from("yt-dlp.exe"),
            version: PINNED_YTDLP_VERSION.to_owned(),
            sha256: PINNED_YTDLP_SHA256.to_owned(),
        }
    }

    fn public_url() -> Url {
        validate_public_https_url("https://93.184.216.34/watch/123").unwrap()
    }

    fn project_count(db: &Connection) -> i64 {
        db.query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap()
    }

    fn runtime_tool(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
    }

    fn run_local_download_attempt(
        database_path: &Path,
        job_id: &str,
        url: &Url,
        yt_dlp: &Path,
        ffmpeg: &str,
    ) -> Result<()> {
        let mut database = db::open_at(database_path)?;
        database.execute(
            "UPDATE source_imports SET status='running',worker_pid=?2,updated_at=?3 WHERE id=?1",
            params![job_id, std::process::id(), now()],
        )?;
        let tool = verify_tool(yt_dlp)?;
        run_download_command(&mut database, job_id, url, &tool, ffmpeg)
    }

    #[test]
    fn source_import_rejects_non_https_private_and_credentialed_urls() {
        for value in [
            "http://93.184.216.34/video",
            "https://127.0.0.1/video",
            "https://10.0.0.2/video",
            "https://[::1]/video",
            "https://user:pass@93.184.216.34/video",
            "https://host.local/video",
        ] {
            assert!(
                validate_public_https_url(value).is_err(),
                "accepted {value}"
            );
        }
        assert!(validate_public_https_url("https://93.184.216.34/video").is_ok());
        assert!(validate_public_https_url("https://[2606:4700:4700::1111]/video").is_ok());
        assert!(fake_tunnel_ip("198.18.0.4".parse().unwrap()));
    }

    #[test]
    fn source_import_mock_server_rejects_private_redirect_and_oversize_headers() {
        let server = MockHttpServer::start(vec![0_u8; 1024]);
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let redirect = preflight_url_with(
            &client,
            &server.url("/private-redirect"),
            validate_public_https_url,
        )
        .unwrap_err();
        assert!(redirect.to_string().contains("source_private_network"));
        let oversize =
            preflight_url_with(&client, &server.url("/oversize"), validate_public_https_url)
                .unwrap_err();
        assert!(oversize.to_string().contains("source_size_limit"));
    }

    #[test]
    fn source_import_parses_one_public_video_for_confirmation() {
        let arguments = inspection_arguments(&public_url());
        assert!(arguments.contains(&"--ignore-config".to_owned()));
        assert!(arguments.contains(&"--no-plugin-dirs".to_owned()));
        assert!(arguments.contains(&"--no-playlist".to_owned()));
        assert!(!arguments.iter().any(|argument| argument.contains("cookie")));
        assert!(!arguments.iter().any(|argument| argument == "--update"));
        let preview = parse_metadata(
            public_url(),
            &json!({
                "_type": "video",
                "id": "media-123",
                "extractor_key": "Example",
                "title": "Authorized sample",
                "duration": 90.5,
                "filesize": 12_345_678,
                "webpage_url": "https://93.184.216.34/watch/123",
                "thumbnail": "https://93.184.216.34/thumb.jpg"
            }),
            &tool(),
        )
        .unwrap();
        assert_eq!(preview.site_media_id, "media-123");
        assert_eq!(preview.duration_seconds, 90.5);
        assert_eq!(preview.file_size_bytes, Some(12_345_678));
        assert!(preview.requires_confirmation);
        assert_eq!(preview.tool_version, PINNED_YTDLP_VERSION);
    }

    #[test]
    fn source_import_rejects_playlists_auth_and_limits() {
        let base = json!({
            "_type": "video",
            "id": "media-123",
            "extractor": "example",
            "title": "Sample",
            "duration": 90,
            "webpage_url": "https://93.184.216.34/watch/123"
        });
        let mut playlist = base.clone();
        playlist["_type"] = json!("playlist");
        assert!(parse_metadata(public_url(), &playlist, &tool()).is_err());

        let mut private = base.clone();
        private["availability"] = json!("private");
        assert!(parse_metadata(public_url(), &private, &tool()).is_err());

        let mut long = base.clone();
        long["duration"] = json!(MAX_DURATION_SECONDS + 0.1);
        assert!(parse_metadata(public_url(), &long, &tool()).is_err());

        let mut large = base;
        large["filesize_approx"] = json!(MAX_FILE_SIZE_BYTES + 1);
        assert!(parse_metadata(public_url(), &large, &tool()).is_err());

        let selected_private = json!({
            "_type": "video",
            "id": "media-123",
            "extractor": "example",
            "title": "Sample",
            "duration": 90,
            "webpage_url": "https://93.184.216.34/watch/123",
            "requested_downloads": [{ "url": "https://127.0.0.1/video.mp4" }]
        });
        let error = parse_metadata(public_url(), &selected_private, &tool()).unwrap_err();
        assert!(error.to_string().contains("source_selected_url_invalid"));
    }

    #[test]
    fn source_import_download_contract_never_enables_cookies_or_updates() {
        let temp = tempdir().unwrap();
        let arguments = download_arguments(&public_url(), temp.path(), "ffmpeg");
        for required in [
            "--ignore-config",
            "--no-plugin-dirs",
            "--no-playlist",
            "--continue",
            "--part",
            "--no-overwrites",
            "--max-filesize",
            "--remux-video",
        ] {
            assert!(
                arguments.contains(&required.to_owned()),
                "missing {required}"
            );
        }
        assert!(arguments.contains(&MAX_FILE_SIZE_BYTES.to_string()));
        assert!(arguments.contains(&"bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b".to_owned()));
        assert!(arguments.contains(&"source.%(ext)s".to_owned()));
        assert!(!arguments.iter().any(|argument| argument.contains("cookie")));
        assert!(!arguments.iter().any(|argument| {
            argument == "-U" || argument == "--update" || argument == "--update-to"
        }));
    }

    #[test]
    fn source_import_job_resumes_partial_and_creates_project_only_after_validation() {
        let ffmpeg = tool_path("SIAOCUT_FFMPEG", "ffmpeg");
        let ffprobe = tool_path("SIAOCUT_FFPROBE", "ffprobe");
        if !crate::media::command_available(&ffmpeg) || !crate::media::command_available(&ffprobe) {
            return;
        }
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("source-jobs.db")).unwrap();
        let preview = parse_metadata(
            public_url(),
            &json!({
                "_type": "video",
                "id": "media-123",
                "extractor_key": "Example",
                "title": "Authorized sample",
                "duration": 1.0,
                "filesize": 12_345_678,
                "webpage_url": "https://93.184.216.34/watch/123"
            }),
            &tool(),
        )
        .unwrap();
        let job = insert_job_at(&db, &preview, &temp.path().join("imports")).unwrap();
        assert_eq!(job.status, "queued");
        assert_eq!(project_count(&db), 0);

        cancel(&db, &job.id).unwrap();
        finish_cancelled(&db, &job.id).unwrap();
        let partial = PathBuf::from(&job.output_directory).join("source.mp4.part");
        fs::write(&partial, vec![7_u8; 128]).unwrap();
        let resumed = prepare_resume(&db, &job.id).unwrap();
        assert_eq!(resumed.status, "queued");
        assert_eq!(resumed.bytes_downloaded, 128);
        assert_eq!(resumed.attempt_count, 2);
        assert!(resumed.progress > 0.0);
        assert_eq!(project_count(&db), 0);

        let output = PathBuf::from(&job.output_directory).join("source.mp4");
        let status = hidden_command(&ffmpeg)
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=24",
                "-t",
                "1",
                "-c:v",
                "mpeg4",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&output)
            .status()
            .unwrap();
        assert!(status.success());
        finalize_download(&mut db, &job.id, &output).unwrap();
        let completed = load(&db, &job.id).unwrap();
        let expected_hash = hash_file(&output).unwrap();
        assert_eq!(completed.status, "completed");
        assert_eq!(completed.progress, 1.0);
        assert!(completed.project_id.is_some());
        assert_eq!(
            completed.output_sha256.as_deref(),
            Some(expected_hash.as_str())
        );
        assert_eq!(project_count(&db), 1);
        let project = project::load(&db, completed.project_id.as_deref().unwrap()).unwrap();
        assert_eq!(project.title, "Authorized sample");
        assert_eq!(
            project.media.source_path,
            output.canonicalize().unwrap().to_string_lossy()
        );
    }

    #[test]
    fn source_import_mock_server_cancels_then_resumes_with_http_range() {
        let yt_dlp = runtime_tool("apps/desktop/src-tauri/runtime/yt-dlp/yt-dlp.exe");
        let ffmpeg_path = runtime_tool("apps/desktop/src-tauri/runtime/ffmpeg/ffmpeg.exe");
        if !yt_dlp.is_file() || !ffmpeg_path.is_file() {
            return;
        }
        let tool = verify_tool(&yt_dlp).unwrap();
        let temp = tempdir().unwrap();
        let fixture = temp.path().join("fixture.mp4");
        let status = hidden_command(&ffmpeg_path)
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=640x360:rate=24",
                "-t",
                "2",
                "-c:v",
                "mpeg4",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&fixture)
            .status()
            .unwrap();
        assert!(status.success());
        let file = fs::OpenOptions::new().write(true).open(&fixture).unwrap();
        file.set_len(8 * 1024 * 1024).unwrap();
        drop(file);
        let body = fs::read(&fixture).unwrap();
        let server = MockHttpServer::start(body.clone());
        let url = server.url("/video.mp4");
        let database_path = temp.path().join("mock-source.db");
        let db = db::open_at(&database_path).unwrap();
        let preview = SourcePreview {
            original_url: url.to_string(),
            webpage_url: url.to_string(),
            site_media_id: "local-range-fixture".to_owned(),
            extractor: "generic".to_owned(),
            title: "Local range fixture".to_owned(),
            duration_seconds: 2.0,
            file_size_bytes: Some(body.len() as u64),
            file_size_known: true,
            thumbnail_url: None,
            tool_version: tool.version.clone(),
            tool_sha256: tool.sha256.clone(),
            requires_confirmation: true,
        };
        let job = insert_job_at(&db, &preview, &temp.path().join("imports")).unwrap();
        let job_id = job.id.clone();
        let worker_database = database_path.clone();
        let worker_url = url.clone();
        let worker_tool = yt_dlp.clone();
        let ffmpeg = ffmpeg_path.to_string_lossy().to_string();
        let worker = thread::spawn(move || {
            run_local_download_attempt(
                &worker_database,
                &job_id,
                &worker_url,
                &worker_tool,
                &ffmpeg,
            )
        });
        let output_directory = PathBuf::from(&job.output_directory);
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if server.bytes_served.load(Ordering::Relaxed) > 128 * 1024 {
                cancel(&db, &job.id).unwrap();
                break;
            }
            assert!(
                !worker.is_finished(),
                "local download finished before cancellation"
            );
            assert!(
                Instant::now() < deadline,
                "timed out waiting for partial download"
            );
            thread::sleep(Duration::from_millis(30));
        }
        worker.join().unwrap().unwrap();
        let cancelled = load(&db, &job.id).unwrap();
        let preserved_bytes = partial_bytes(&output_directory).unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert!(
            preserved_bytes > 0 && preserved_bytes < body.len() as u64,
            "unexpected partial size {preserved_bytes} for {} byte fixture; files: {:?}",
            body.len(),
            fs::read_dir(&output_directory)
                .unwrap()
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .collect::<Vec<_>>()
        );
        assert_eq!(project_count(&db), 0);

        let resumed = prepare_resume(&db, &job.id).unwrap();
        assert_eq!(resumed.attempt_count, 2);
        assert!(resumed.bytes_downloaded >= preserved_bytes);
        let resumed_job_id = job.id.clone();
        let resumed_database = database_path.clone();
        let resumed_url = url.clone();
        let resumed_tool = yt_dlp.clone();
        let resumed_ffmpeg = ffmpeg_path.to_string_lossy().to_string();
        thread::spawn(move || {
            run_local_download_attempt(
                &resumed_database,
                &resumed_job_id,
                &resumed_url,
                &resumed_tool,
                &resumed_ffmpeg,
            )
        })
        .join()
        .unwrap()
        .unwrap();
        let completed = load(&db, &job.id).unwrap();
        assert_eq!(completed.status, "completed");
        assert_eq!(completed.attempt_count, 2);
        assert_eq!(project_count(&db), 1);
        assert!(
            server
                .range_starts
                .lock()
                .unwrap()
                .iter()
                .any(|start| *start >= preserved_bytes),
            "resume did not request the preserved byte range"
        );
    }
}
