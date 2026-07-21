use crate::{
    db,
    media::{hash_file, tool_path},
    project,
    util::{hidden_command, new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

mod moss;
mod provider;
mod review;

use moss::MossProvider;
use provider::{ProviderRequest, TranscriptionProvider};
use review::{build_review_items, build_track};
pub use review::{render_structured_export, resolve_review, review_items};

pub const PROVIDER_ID: &str = "moss_openai";
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8000";
pub const DEFAULT_MODEL_ID: &str = "OpenMOSS-Team/MOSS-Transcribe-Diarize";

static MOSS_PROVIDER: MossProvider = MossProvider;

fn provider_for(provider_id: &str) -> Result<&'static dyn TranscriptionProvider> {
    if provider_id == MOSS_PROVIDER.provider_id() {
        return Ok(&MOSS_PROVIDER);
    }
    bail!("transcription_provider_invalid: 不支持的转写提供方：{provider_id}")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub provider_id: String,
    pub endpoint: String,
    pub model_id: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub provider_id: String,
    pub endpoint: String,
    pub model_id: String,
    pub state: String,
    pub detail: String,
    pub checked_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionJob {
    pub id: String,
    pub project_id: String,
    pub provider_id: String,
    pub endpoint: String,
    pub model_id: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub hotwords: Vec<String>,
    pub status: String,
    pub stage: String,
    pub result_run_id: Option<String>,
    pub base_version_id: Option<String>,
    pub source_sha256: Option<String>,
    pub input_audio_sha256: Option<String>,
    pub cancel_requested_at: Option<String>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub worker_pid: Option<u32>,
    pub attempt_count: u32,
    pub candidate: Option<TranscriptionCandidateSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionCandidateSummary {
    pub run_id: String,
    pub segment_count: u32,
    pub speaker_count: u32,
    pub duration_seconds: Option<f64>,
    pub warning_count: u32,
    pub base_version_id: Option<String>,
    pub current_version_id: Option<String>,
    pub can_apply: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItem {
    pub id: String,
    pub project_id: String,
    pub run_id: String,
    pub segment_id: Option<String>,
    pub severity: String,
    pub kind: String,
    pub message: String,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Clone, Debug)]
struct ImportedSegment {
    id: String,
    start: f64,
    end: f64,
    speaker: String,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinalizationOutcome {
    Completed,
    AwaitingApply,
}

struct TemporaryFile {
    path: PathBuf,
}

impl TemporaryFile {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn config(db: &Connection) -> Result<ProviderConfig> {
    db.query_row(
        "SELECT provider_id,endpoint,model_id,updated_at FROM transcription_provider_config WHERE provider_id=?1",
        [PROVIDER_ID],
        |row| Ok(ProviderConfig { provider_id: row.get(0)?, endpoint: row.get(1)?, model_id: row.get(2)?, updated_at: row.get(3)? }),
    ).context("transcription_provider_invalid: MOSS 提供方配置不存在")
}

pub fn configure(db: &Connection, endpoint: &str, model_id: &str) -> Result<ProviderConfig> {
    let endpoint = provider_for(PROVIDER_ID)?.validate_endpoint(endpoint)?;
    let model_id = model_id.trim();
    if model_id.is_empty() || model_id.len() > 200 {
        bail!("transcription_provider_invalid: 模型标识不能为空或超过 200 个字符")
    }
    db.execute(
        "UPDATE transcription_provider_config SET endpoint=?2,model_id=?3,updated_at=?4 WHERE provider_id=?1",
        params![PROVIDER_ID, endpoint, model_id, now()],
    )?;
    config(db)
}

pub fn validate_loopback_endpoint(endpoint: &str) -> Result<String> {
    MOSS_PROVIDER.validate_endpoint(endpoint)
}

pub fn health(db: &Connection) -> Result<ProviderHealth> {
    let config = config(db)?;
    provider_for(&config.provider_id)?.health(&config)
}

pub fn start(
    db: &mut Connection,
    project_id: &str,
    language: Option<&str>,
    prompt: Option<&str>,
    hotwords: &[String],
    start_delay_ms: Option<u64>,
) -> Result<TranscriptionJob> {
    let project = project::load(db, project_id)?;
    let source_path = Path::new(&project.media.source_path);
    if !source_path.is_file() {
        bail!("audio_source_missing: 项目关联的原始媒体不存在")
    }
    let source_sha256 = hash_file(source_path)?;
    if source_sha256 != project.media.sha256 {
        bail!("transcription_source_changed: 原始媒体内容与项目记录不一致，请重新定位媒体")
    }
    let base_version_id = project.history.current_version_id.clone();
    let provider = config(db)?;
    let endpoint = validate_loopback_endpoint(&provider.endpoint)?;
    let provider_health = health(db)?;
    if provider_health.state != "healthy" {
        bail!(
            "transcription_provider_unavailable: {}",
            provider_health.detail
        )
    }
    let timestamp = now();
    let id = new_id("transcription");
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(job) = latest_active(&tx, project_id)? {
        tx.commit()?;
        return Ok(job);
    }
    tx.execute(
        "INSERT INTO transcription_jobs(
            id,project_id,provider_id,endpoint,model_id,language,prompt,hotwords_json,
            status,stage,base_version_id,source_sha256,created_at,updated_at
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'queued','queued',?9,?10,?11,?11)",
        params![
            id,
            project_id,
            provider.provider_id,
            endpoint,
            provider.model_id,
            clean_language(language),
            clean_optional(prompt),
            serde_json::to_string(hotwords)?,
            base_version_id,
            source_sha256,
            timestamp
        ],
    )?;
    tx.commit()?;
    if let Err(error) = spawn_worker(&id, start_delay_ms) {
        db.execute("UPDATE transcription_jobs SET status='failed',stage='failed',error_message=?2,completed_at=?3,updated_at=?3 WHERE id=?1", params![id, error.to_string(), now()])?;
        return Err(error);
    }
    load(db, &id)
}

fn clean_language(value: Option<&str>) -> Option<String> {
    clean_optional(value).filter(|value| !value.eq_ignore_ascii_case("auto"))
}

fn clean_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn load(db: &Connection, job_id: &str) -> Result<TranscriptionJob> {
    let mut job = db.query_row(
        "SELECT id,project_id,provider_id,endpoint,model_id,language,prompt,hotwords_json,
                status,stage,result_run_id,base_version_id,source_sha256,input_audio_sha256,
                cancel_requested_at,error_message,created_at,updated_at,completed_at,worker_pid,attempt_count
         FROM transcription_jobs WHERE id=?1",
        [job_id], |row| {
            let status: String = row.get(8)?;
            let error_message: Option<String> = row.get(15)?;
            let hotwords_json: String = row.get(7)?;
            Ok(TranscriptionJob {
                id: row.get(0)?, project_id: row.get(1)?, provider_id: row.get(2)?, endpoint: row.get(3)?, model_id: row.get(4)?,
                language: row.get(5)?, prompt: row.get(6)?, hotwords: serde_json::from_str(&hotwords_json).unwrap_or_default(),
                status: status.clone(), stage: row.get(9)?, result_run_id: row.get(10)?, base_version_id: row.get(11)?,
                source_sha256: row.get(12)?, input_audio_sha256: row.get(13)?, cancel_requested_at: row.get(14)?,
                error_code: crate::model::background_error_code(&status, error_message.as_deref()), error_message,
                created_at: row.get(16)?, updated_at: row.get(17)?, completed_at: row.get(18)?, worker_pid: row.get(19)?, attempt_count: row.get::<_, i64>(20)? as u32,
                candidate: None,
            })
        }
    ).optional()?.ok_or_else(|| anyhow!("transcription_job_not_found: 转写任务不存在：{job_id}"))?;
    job.candidate = candidate_summary(db, &job)?;
    Ok(job)
}

fn candidate_summary(
    db: &Connection,
    job: &TranscriptionJob,
) -> Result<Option<TranscriptionCandidateSummary>> {
    let Some(run_id) = job.result_run_id.as_deref() else {
        return Ok(None);
    };
    let candidate = db
        .query_row(
            "SELECT segment_count,speaker_count,duration_seconds,warning_count,base_version_id,status
             FROM transcription_runs WHERE id=?1",
            [run_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((
        segment_count,
        speaker_count,
        duration_seconds,
        warning_count,
        base_version_id,
        run_status,
    )) = candidate
    else {
        return Ok(None);
    };
    Ok(Some(TranscriptionCandidateSummary {
        run_id: run_id.to_owned(),
        segment_count: segment_count as u32,
        speaker_count: speaker_count as u32,
        duration_seconds,
        warning_count: warning_count as u32,
        base_version_id,
        current_version_id: project::current_version_id(db, &job.project_id)?,
        can_apply: job.status == "awaiting_apply" && run_status == "prepared",
    }))
}

pub fn list(db: &Connection, project_id: Option<&str>) -> Result<Vec<TranscriptionJob>> {
    let ids = if let Some(project_id) = project_id {
        db.prepare(
            "SELECT id FROM transcription_jobs WHERE project_id=?1 ORDER BY created_at DESC",
        )?
        .query_map([project_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        db.prepare("SELECT id FROM transcription_jobs ORDER BY created_at DESC")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    ids.into_iter().map(|id| load(db, &id)).collect()
}

pub fn latest(db: &Connection, project_id: &str) -> Result<Option<TranscriptionJob>> {
    db.query_row(
        "SELECT id FROM transcription_jobs WHERE project_id=?1 ORDER BY created_at DESC LIMIT 1",
        [project_id],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|id| load(db, &id))
    .transpose()
}

fn latest_active(db: &Connection, project_id: &str) -> Result<Option<TranscriptionJob>> {
    db.query_row("SELECT id FROM transcription_jobs WHERE project_id=?1 AND status IN ('queued','running','finalizing','awaiting_apply') ORDER BY created_at DESC LIMIT 1", [project_id], |row| row.get::<_, String>(0))
        .optional()?.map(|id| load(db, &id)).transpose()
}

pub fn cancel(db: &Connection, job_id: &str) -> Result<TranscriptionJob> {
    let job = load(db, job_id)?;
    if !matches!(job.status.as_str(), "queued" | "running" | "finalizing") {
        bail!("transcription_job_not_cancellable: 当前转写任务不能取消")
    }
    let timestamp = now();
    let changed = db.execute(
        "UPDATE transcription_jobs
         SET status='cancelled',stage='cancelled',cancel_requested_at=?2,completed_at=?2,updated_at=?2
         WHERE id=?1 AND status IN ('queued','running','finalizing')",
        params![job_id, timestamp],
    )?;
    if changed != 1 {
        bail!("transcription_job_not_cancellable: 当前转写任务不能取消")
    }
    if let Some(pid) = job.worker_pid
        && pid != std::process::id()
        && crate::util::process_is_active(pid)
    {
        let _ = crate::util::terminate_process_tree_by_id(pid);
    }
    db.execute(
        "UPDATE transcription_jobs SET worker_pid=NULL WHERE id=?1 AND status='cancelled'",
        [job_id],
    )?;
    load(db, job_id)
}

pub fn resume(
    db: &mut Connection,
    job_id: &str,
    start_delay_ms: Option<u64>,
) -> Result<TranscriptionJob> {
    let job = load(db, job_id)?;
    if !matches!(job.status.as_str(), "cancelled" | "failed" | "interrupted") {
        bail!("transcription_job_not_resumable: 当前转写任务不能继续")
    }
    let project_value = project::load(db, &job.project_id)?;
    let source_sha256 = hash_file(Path::new(&project_value.media.source_path))?;
    if source_sha256 != project_value.media.sha256 {
        bail!("transcription_source_changed: 原始媒体内容与项目记录不一致，请重新定位媒体")
    }
    let prepared_result = result_path(job_id).is_file()
        || job.result_run_id.as_deref().is_some_and(|run_id| {
            db.query_row(
                "SELECT status='prepared' FROM transcription_runs WHERE id=?1",
                [run_id],
                |row| row.get::<_, bool>(0),
            )
            .unwrap_or(false)
        });
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(active) = latest_active(&tx, &job.project_id)? {
        bail!(
            "transcription_active_job_exists: 项目已有等待处理的转写任务：{}",
            active.id
        )
    }
    tx.execute(
        "UPDATE transcription_jobs
         SET status='queued',stage='queued',result_run_id=CASE WHEN ?2 THEN result_run_id ELSE NULL END,
             base_version_id=CASE WHEN ?2 THEN base_version_id ELSE ?3 END,
             source_sha256=CASE WHEN ?2 THEN source_sha256 ELSE ?4 END,
             input_audio_sha256=CASE WHEN ?2 THEN input_audio_sha256 ELSE NULL END,
             cancel_requested_at=NULL,error_message=NULL,
             completed_at=NULL,worker_pid=NULL,attempt_count=attempt_count+1,updated_at=?5
         WHERE id=?1",
        params![
            job_id,
            prepared_result,
            project_value.history.current_version_id,
            source_sha256,
            now()
        ],
    )?;
    tx.commit()?;
    if let Err(error) = spawn_worker(job_id, start_delay_ms) {
        db.execute(
            "UPDATE transcription_jobs SET status='failed',stage='failed',error_message=?2,completed_at=?3,updated_at=?3 WHERE id=?1",
            params![job_id, error.to_string(), now()],
        )?;
        return Err(error).context("无法继续 MOSS 转写");
    }
    load(db, job_id)
}

pub fn apply_candidate(
    db: &mut Connection,
    job_id: &str,
    expected_current_version: &str,
    confirm_replace: bool,
) -> Result<TranscriptionJob> {
    if !confirm_replace {
        bail!("transcription_apply_confirmation_required: 必须明确确认替换当前字幕和说话人轨")
    }
    let job = load(db, job_id)?;
    if job.status != "awaiting_apply" {
        bail!("transcription_result_not_ready: 当前任务没有等待应用的结果")
    }
    let (run_id, raw_path, result_sha256) = prepared_run(db, job_id)?
        .ok_or_else(|| anyhow!("transcription_result_not_ready: 未找到已准备的转写结果"))?;
    let raw = read_verified_result(&raw_path, &result_sha256)?;
    let imported = parsed_segments(&job.provider_id, &raw)?;
    let project_value = project::load(db, &job.project_id)?;
    finalize_result(
        db,
        &job,
        &project_value,
        &run_id,
        &raw_path,
        &raw,
        &imported,
        Some(expected_current_version),
    )?;
    load(db, job_id)
}

pub fn discard_candidate(db: &mut Connection, job_id: &str) -> Result<TranscriptionJob> {
    let job = load(db, job_id)?;
    if job.status != "awaiting_apply" {
        bail!("transcription_result_not_ready: 当前任务没有等待丢弃的转写结果")
    }
    let (run_id, raw_path, _) = prepared_run(db, job_id)?
        .ok_or_else(|| anyhow!("transcription_result_not_ready: 未找到已准备的转写结果"))?;
    let timestamp = now();
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let run_changed = tx.execute(
        "UPDATE transcription_runs SET status='discarded' WHERE id=?1 AND status='prepared'",
        [&run_id],
    )?;
    let job_changed = tx.execute(
        "UPDATE transcription_jobs
         SET status='discarded',stage='discarded',error_message=NULL,worker_pid=NULL,
             completed_at=?2,updated_at=?2
         WHERE id=?1 AND status='awaiting_apply'",
        params![job_id, timestamp],
    )?;
    if run_changed != 1 || job_changed != 1 {
        bail!("transcription_result_not_ready: 转写候选结果已被处理")
    }
    tx.commit()?;
    let _ = fs::remove_file(raw_path);
    load(db, job_id)
}

pub fn reconcile_interrupted(db: &Connection) -> Result<()> {
    let jobs = db.prepare("SELECT id,worker_pid,updated_at FROM transcription_jobs WHERE status IN ('queued','running','finalizing')")?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<u32>>(1)?, row.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, pid, updated_at) in jobs {
        let stale = chrono::DateTime::parse_from_rfc3339(&updated_at)
            .map(|value| {
                chrono::Utc::now()
                    .signed_duration_since(value.with_timezone(&chrono::Utc))
                    .num_seconds()
                    >= 5
            })
            .unwrap_or(true);
        if stale && !pid.is_some_and(crate::util::process_is_active) {
            db.execute("UPDATE transcription_jobs SET status='interrupted',stage='interrupted',error_message='上次 MOSS 转写进程已中断，可以显式继续。',worker_pid=NULL,updated_at=?2 WHERE id=?1", params![id, now()])?;
        }
    }
    cleanup_orphaned_artifacts(db)?;
    Ok(())
}

fn cleanup_orphaned_artifacts(db: &Connection) -> Result<()> {
    let cache_dir = crate::db::home_dir().join("cache").join("transcription");
    if cache_dir.is_dir() {
        for entry in fs::read_dir(&cache_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("wav") {
                continue;
            }
            let Some(job_id) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let active = db
                .query_row(
                    "SELECT status IN ('queued','running','finalizing') FROM transcription_jobs WHERE id=?1",
                    [job_id],
                    |row| row.get::<_, bool>(0),
                )
                .optional()?
                .unwrap_or(false);
            if !active {
                let _ = fs::remove_file(path);
            }
        }
    }

    let runs_dir = crate::db::home_dir().join("transcription-runs");
    if runs_dir.is_dir() {
        for entry in fs::read_dir(&runs_dir)? {
            let path = entry?.path();
            let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if let Some(job_id) = file_name.strip_suffix(".json.partial") {
                let active = db
                    .query_row(
                        "SELECT status IN ('queued','running','finalizing') FROM transcription_jobs WHERE id=?1",
                        [job_id],
                        |row| row.get::<_, bool>(0),
                    )
                    .optional()?
                    .unwrap_or(false);
                if !active {
                    let _ = fs::remove_file(path);
                }
                continue;
            }
            if !file_name.ends_with(".json") {
                continue;
            }
            let referenced = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM transcription_runs WHERE raw_result_path=?1)",
                [path.to_string_lossy().as_ref()],
                |row| row.get::<_, bool>(0),
            )?;
            let recoverable_job = file_name.strip_suffix(".json").is_some_and(|job_id| {
                db.query_row(
                    "SELECT status IN ('queued','running','finalizing','cancelled','interrupted','failed') FROM transcription_jobs WHERE id=?1",
                    [job_id],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap_or(false)
            });
            if !referenced && !recoverable_job {
                let _ = fs::remove_file(path);
            }
        }
    }
    Ok(())
}

fn spawn_worker(job_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let delay = start_delay_ms.map(|value| value.to_string());
    let mut args = vec!["__transcription_worker", job_id];
    if let Some(delay) = delay.as_deref() {
        args.push(delay);
    }
    crate::util::spawn_detached_current(&args)?;
    Ok(())
}

pub fn run_worker(job_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let mut database = db::open()?;
    let claimed = database.execute(
        "UPDATE transcription_jobs SET status='running',stage='preparing_audio',worker_pid=?2,updated_at=?3 WHERE id=?1 AND status='queued'",
        params![job_id, std::process::id(), now()],
    )?;
    if claimed == 0 {
        return Ok(());
    }
    if let Some(delay) = start_delay_ms {
        thread::sleep(Duration::from_millis(delay));
    }
    let result = execute_job(&mut database, job_id);
    match result {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().starts_with("transcription_cancelled") => {
            let timestamp = now();
            database.execute(
                "UPDATE transcription_jobs SET status='cancelled',stage='cancelled',worker_pid=NULL,completed_at=?2,updated_at=?2 WHERE id=?1 AND status!='cancelled'",
                params![job_id, timestamp],
            )?;
            Ok(())
        }
        Err(error) => {
            let timestamp = now();
            database.execute(
                "UPDATE transcription_jobs SET status='failed',stage='failed',error_message=?2,worker_pid=NULL,completed_at=?3,updated_at=?3 WHERE id=?1 AND status!='cancelled'",
                params![job_id, error.to_string(), timestamp],
            )?;
            Err(error)
        }
    }
}

fn execute_job(db: &mut Connection, job_id: &str) -> Result<String> {
    ensure_not_cancelled(db, job_id)?;
    let mut job = load(db, job_id)?;
    let project_value = project::load(db, &job.project_id)?;
    if let Some((run_id, raw_path, result_sha256)) = prepared_run(db, job_id)? {
        mark_finalizing(db, job_id)?;
        let raw = read_verified_result(&raw_path, &result_sha256)?;
        let imported = parsed_segments(&job.provider_id, &raw)?;
        job = load(db, job_id)?;
        finalize_result(
            db,
            &job,
            &project_value,
            &run_id,
            &raw_path,
            &raw,
            &imported,
            None,
        )?;
        return Ok(run_id);
    }

    let raw_path = result_path(job_id);
    if raw_path.is_file()
        && let Ok(raw) = fs::read_to_string(&raw_path)
        && let Ok(imported) = parsed_segments(&job.provider_id, &raw)
    {
        mark_finalizing(db, job_id)?;
        job = load(db, job_id)?;
        let run_id = prepare_result(db, &job, &raw_path, &raw, &imported, None)?;
        finalize_result(
            db,
            &job,
            &project_value,
            &run_id,
            &raw_path,
            &raw,
            &imported,
            None,
        )?;
        return Ok(run_id);
    }
    if raw_path.is_file() {
        let _ = fs::remove_file(&raw_path);
    }

    let cache_dir = crate::db::home_dir().join("cache").join("transcription");
    fs::create_dir_all(&cache_dir)?;
    let wav_path = cache_dir.join(format!("{}.wav", job.id));
    let _wav_guard = TemporaryFile::new(wav_path.clone());
    extract_audio(Path::new(&project_value.media.source_path), &wav_path)?;
    let input_audio_sha256 = hash_file(&wav_path)?;
    db.execute(
        "UPDATE transcription_jobs SET input_audio_sha256=?2,updated_at=?3 WHERE id=?1 AND status='running'",
        params![job_id, input_audio_sha256, now()],
    )?;
    ensure_not_cancelled(db, job_id)?;
    db.execute(
        "UPDATE transcription_jobs SET stage='requesting_model',updated_at=?2 WHERE id=?1",
        params![job_id, now()],
    )?;
    let raw = request_moss(&job, &wav_path)?;
    ensure_not_cancelled(db, job_id)?;
    mark_finalizing(db, job_id)?;
    let imported = parsed_segments(&job.provider_id, &raw)?;
    atomic_write_result(&raw_path, raw.as_bytes())?;
    job = load(db, job_id)?;
    let run_id = prepare_result(db, &job, &raw_path, &raw, &imported, None)?;
    finalize_result(
        db,
        &job,
        &project_value,
        &run_id,
        &raw_path,
        &raw,
        &imported,
        None,
    )?;
    Ok(run_id)
}

fn parsed_segments(provider_id: &str, raw: &str) -> Result<Vec<ImportedSegment>> {
    Ok(provider_for(provider_id)?
        .parse(raw)?
        .into_iter()
        .map(|segment| ImportedSegment {
            id: new_id("s"),
            start: segment.start,
            end: segment.end,
            speaker: segment.speaker,
            text: segment.text,
        })
        .collect())
}

fn validate_imported(segments: &[ImportedSegment], duration: Option<f64>) -> Result<()> {
    if segments.is_empty() {
        bail!("transcription_response_invalid: 转写提供方没有返回字幕分段")
    }
    let mut previous_start = -1.0;
    for segment in segments {
        if !segment.start.is_finite()
            || !segment.end.is_finite()
            || segment.start < 0.0
            || segment.end <= segment.start
        {
            bail!("transcription_timing_invalid: 转写提供方返回了无效时间范围")
        }
        if segment.start + 0.001 < previous_start {
            bail!("transcription_timing_invalid: 转写分段未按时间排序")
        }
        if duration.is_some_and(|duration| segment.end > duration + 2.0) {
            bail!("transcription_timing_invalid: 转写分段超出媒体时长")
        }
        if segment.text.trim().is_empty() {
            bail!("transcription_response_invalid: 转写提供方返回了空字幕段")
        }
        previous_start = segment.start;
    }
    Ok(())
}

fn result_path(job_id: &str) -> PathBuf {
    crate::db::home_dir()
        .join("transcription-runs")
        .join(format!("{job_id}.json"))
}

fn atomic_write_result(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("transcription_import_failed: 转写结果目录无效"))?;
    fs::create_dir_all(parent)?;
    let partial = path.with_extension("json.partial");
    let _partial_guard = TemporaryFile::new(partial.clone());
    fs::write(&partial, bytes).context("transcription_import_failed: 无法保存 MOSS 临时响应")?;
    fs::rename(&partial, path).context("transcription_import_failed: 无法提交 MOSS 原始响应")?;
    Ok(())
}

fn mark_finalizing(db: &Connection, job_id: &str) -> Result<()> {
    let changed = db.execute(
        "UPDATE transcription_jobs
         SET status='finalizing',stage='validating_result',updated_at=?2
         WHERE id=?1 AND status IN ('running','finalizing') AND cancel_requested_at IS NULL",
        params![job_id, now()],
    )?;
    if changed != 1 {
        ensure_not_cancelled(db, job_id)?;
        bail!("transcription_job_state_invalid: 转写任务状态不允许准备结果")
    }
    Ok(())
}

fn prepared_run(db: &Connection, job_id: &str) -> Result<Option<(String, PathBuf, String)>> {
    db.query_row(
        "SELECT id,raw_result_path,result_sha256 FROM transcription_runs
         WHERE job_id=?1 AND status='prepared'",
        [job_id],
        |row| {
            Ok((
                row.get(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
                row.get(2)?,
            ))
        },
    )
    .optional()
    .map_err(Into::into)
}

fn read_verified_result(path: &Path, expected_sha256: &str) -> Result<String> {
    let raw = fs::read_to_string(path)
        .context("transcription_result_not_ready: 已保存的转写结果不存在或无法读取")?;
    let actual = format!("{:x}", Sha256::digest(raw.as_bytes()));
    if actual != expected_sha256 {
        bail!("transcription_result_not_ready: 已保存的转写结果校验失败")
    }
    Ok(raw)
}

fn ensure_not_cancelled(db: &Connection, job_id: &str) -> Result<()> {
    let cancelled: bool = db.query_row(
        "SELECT status='cancelled' OR cancel_requested_at IS NOT NULL FROM transcription_jobs WHERE id=?1",
        [job_id], |row| row.get(0),
    )?;
    if cancelled {
        bail!("transcription_cancelled: 转写任务已取消")
    }
    Ok(())
}

fn extract_audio(source: &Path, wav: &Path) -> Result<()> {
    let ffmpeg = tool_path("SIAOCUT_FFMPEG", "ffmpeg");
    let output = hidden_command(&ffmpeg)
        .args(["-y", "-i"])
        .arg(source)
        .args(["-vn", "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"])
        .arg(wav)
        .output()
        .with_context(|| format!("无法启动 FFmpeg：{ffmpeg}"))?;
    if !output.status.success() {
        bail!(
            "transcription_import_failed: FFmpeg 音频提取失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
    Ok(())
}

fn request_moss(job: &TranscriptionJob, wav: &Path) -> Result<String> {
    provider_for(&job.provider_id)?.transcribe(ProviderRequest {
        endpoint: &job.endpoint,
        model_id: &job.model_id,
        language: job.language.as_deref(),
        prompt: job.prompt.as_deref(),
        hotwords: &job.hotwords,
        audio_path: wav,
    })
}

#[cfg(test)]
fn import_result(
    db: &mut Connection,
    job: &TranscriptionJob,
    project_value: &crate::model::Project,
    run_id: &str,
    raw_path: &Path,
    raw: &str,
    segments: &[ImportedSegment],
) -> Result<()> {
    let run_id = prepare_result(db, job, raw_path, raw, segments, Some(run_id))?;
    finalize_result(
        db,
        job,
        project_value,
        &run_id,
        raw_path,
        raw,
        segments,
        None,
    )?;
    Ok(())
}

fn prepare_result(
    db: &mut Connection,
    job: &TranscriptionJob,
    raw_path: &Path,
    raw: &str,
    segments: &[ImportedSegment],
    requested_run_id: Option<&str>,
) -> Result<String> {
    validate_imported(segments, None)?;
    let generated_at = now();
    let track = build_track(segments, &job.model_id, &generated_at);
    let run_id = requested_run_id
        .map(str::to_owned)
        .unwrap_or_else(|| new_id("trun"));
    let reviews = build_review_items(&job.project_id, &run_id, segments, &generated_at);
    let warning_count = reviews
        .iter()
        .filter(|item| item.severity == "warning")
        .count() as i64;
    let duration_seconds = segments.last().map(|segment| segment.end);
    let raw_hash = format!("{:x}", Sha256::digest(raw.as_bytes()));
    let source_sha256 = job
        .source_sha256
        .as_deref()
        .ok_or_else(|| anyhow!("transcription_result_not_ready: 转写任务缺少源媒体校验值"))?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(existing) = tx
        .query_row(
            "SELECT id,result_sha256 FROM transcription_runs WHERE job_id=?1 AND status='prepared'",
            [&job.id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing.1 != raw_hash {
            bail!("transcription_result_not_ready: 已准备结果与恢复文件不一致")
        }
        tx.execute(
            "UPDATE transcription_jobs SET result_run_id=?2,updated_at=?3 WHERE id=?1",
            params![job.id, existing.0, now()],
        )?;
        tx.commit()?;
        return Ok(existing.0);
    }
    let changed = tx.execute(
        "UPDATE transcription_jobs SET result_run_id=?2,updated_at=?3
         WHERE id=?1 AND status='finalizing' AND cancel_requested_at IS NULL",
        params![job.id, run_id, now()],
    )?;
    if changed != 1 {
        bail!("transcription_cancelled: 转写任务已取消或状态已经变化")
    }
    tx.execute(
        "INSERT INTO transcription_runs(
            id,project_id,job_id,provider_id,model_id,status,base_version_id,source_sha256,
            input_audio_sha256,result_sha256,raw_result_path,segment_count,speaker_count,
            has_word_timings,applied_version_id,created_at,duration_seconds,warning_count
         ) VALUES(?1,?2,?3,?4,?5,'prepared',?6,?7,?8,?9,?10,?11,?12,0,NULL,?13,?14,?15)",
        params![
            run_id,
            job.project_id,
            job.id,
            job.provider_id,
            job.model_id,
            job.base_version_id,
            source_sha256,
            job.input_audio_sha256,
            raw_hash,
            raw_path.to_string_lossy(),
            segments.len() as i64,
            track.speakers.len() as i64,
            generated_at,
            duration_seconds,
            warning_count
        ],
    )?;
    tx.commit()?;
    Ok(run_id)
}

#[allow(clippy::too_many_arguments)]
fn finalize_result(
    db: &mut Connection,
    job: &TranscriptionJob,
    project_value: &crate::model::Project,
    run_id: &str,
    _raw_path: &Path,
    _raw: &str,
    segments: &[ImportedSegment],
    expected_current_version: Option<&str>,
) -> Result<FinalizationOutcome> {
    validate_imported(segments, project_value.media.duration_seconds)?;
    let source_sha256 = hash_file(Path::new(&project_value.media.source_path))?;
    if job.source_sha256.as_deref() != Some(source_sha256.as_str())
        || source_sha256 != project_value.media.sha256
    {
        bail!("transcription_source_changed: 转写期间原始媒体内容发生变化，结果未应用")
    }
    let generated_at = now();
    let track = build_track(segments, &job.model_id, &generated_at);
    let reviews = build_review_items(&job.project_id, run_id, segments, &generated_at);

    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let status: String = tx.query_row(
        "SELECT status FROM transcription_jobs WHERE id=?1",
        [&job.id],
        |row| row.get(0),
    )?;
    let current_version_id = project::current_version_id(&tx, &job.project_id)?;
    if let Some(expected) = expected_current_version {
        if status != "awaiting_apply" {
            bail!("transcription_result_not_ready: 当前任务没有等待应用的结果")
        }
        if current_version_id.as_deref() != Some(expected) {
            bail!("transcription_apply_version_mismatch: 项目在确认后再次发生变化，请重新检查影响")
        }
    } else {
        if status == "cancelled" {
            bail!("transcription_cancelled: 转写任务已取消")
        }
        if status != "finalizing" {
            bail!("transcription_job_state_invalid: 转写任务状态不允许导入结果")
        }
        if current_version_id != job.base_version_id {
            tx.execute(
                "UPDATE transcription_jobs
                 SET status='awaiting_apply',stage='awaiting_apply',result_run_id=?2,
                     error_message='transcription_project_changed: 转写期间项目已被修改，结果等待确认。',
                     worker_pid=NULL,completed_at=NULL,updated_at=?3
                 WHERE id=?1 AND status='finalizing'",
                params![job.id, run_id, now()],
            )?;
            tx.commit()?;
            return Ok(FinalizationOutcome::AwaitingApply);
        }
    }

    let transaction_result = (|| -> Result<()> {
        tx.execute(
            "DELETE FROM segments WHERE project_id=?1",
            [&job.project_id],
        )?;
        for segment in segments {
            project::assert_segment(segment.start, segment.end, &segment.text)
                .context("transcription_import_failed: 无法导入 MOSS 字幕段")?;
            tx.execute(
                "INSERT INTO segments(id,project_id,start_seconds,end_seconds,text,confidence) VALUES(?1,?2,?3,?4,?5,NULL)",
                params![segment.id, job.project_id, segment.start, segment.end, segment.text],
            )?;
        }
        tx.execute(
            "UPDATE translations SET status='stale' WHERE project_id=?1",
            [&job.project_id],
        )?;
        crate::speaker::replace_track_tx(&tx, &job.project_id, Some(&track))?;
        tx.execute(
            "DELETE FROM transcription_review_items WHERE run_id=?1",
            [run_id],
        )?;
        for item in &reviews {
            tx.execute(
                "INSERT INTO transcription_review_items(id,project_id,run_id,segment_id,severity,kind,message,status,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,'open',?8)",
                params![item.id, item.project_id, item.run_id, item.segment_id, item.severity, item.kind, item.message, item.created_at],
            )?;
        }
        let version =
            project::snapshot_in_transaction(&tx, &job.project_id, "MOSS 多人长音频转写")?;
        let completed_at = now();
        let run_changed = tx.execute(
            "UPDATE transcription_runs SET status='applied',applied_version_id=?2 WHERE id=?1 AND status='prepared'",
            params![run_id, version.id],
        )?;
        if run_changed != 1 {
            bail!("transcription_result_not_ready: 转写候选结果已被处理")
        }
        let changed = tx.execute(
            "UPDATE transcription_jobs
             SET status='completed',stage='completed',result_run_id=?2,error_message=NULL,
                 worker_pid=NULL,completed_at=?3,updated_at=?3
             WHERE id=?1 AND status IN ('finalizing','awaiting_apply') AND cancel_requested_at IS NULL",
            params![job.id, run_id, completed_at],
        )?;
        if changed != 1 {
            bail!("transcription_cancelled: 转写任务已取消")
        }
        Ok(())
    })();
    transaction_result.context("transcription_import_failed: MOSS 结果未写入项目")?;
    tx.commit()?;
    Ok(FinalizationOutcome::Completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::tempdir;

    #[test]
    fn loopback_validation_rejects_remote_and_credentials() {
        assert_eq!(
            validate_loopback_endpoint("http://localhost:8000/").unwrap(),
            "http://localhost:8000"
        );
        assert!(validate_loopback_endpoint("https://127.0.0.1:8000").is_err());
        assert!(validate_loopback_endpoint("http://example.com:8000").is_err());
        assert!(validate_loopback_endpoint("http://user@127.0.0.1:8000").is_err());
        assert!(validate_loopback_endpoint("http://127.0.0.1:8000/v1").is_err());
    }

    #[test]
    fn structured_export_requires_warning_confirmation_and_preserves_evidence() {
        let temp = tempdir().unwrap();
        let mut database = db::open_at(&temp.path().join("structured-export.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut database, &media, Some("Meeting".into())).unwrap();
        let segment =
            project::add_segment(&mut database, &project.id, 0.0, 1.0, "你好".into(), None)
                .unwrap();
        let timestamp = now();
        database.execute("INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,language,prompt,hotwords_json,status,stage,result_run_id,cancel_requested_at,error_message,created_at,updated_at,completed_at,worker_pid,attempt_count) VALUES('job-export',?1,'moss_openai','http://127.0.0.1:8000','moss-test',NULL,NULL,'[]','completed','completed','run-export',NULL,NULL,?2,?2,?2,NULL,1)", params![&project.id, &timestamp]).unwrap();
        database.execute("INSERT INTO transcription_runs(id,project_id,job_id,provider_id,model_id,source_sha256,result_sha256,raw_result_path,segment_count,speaker_count,has_word_timings,created_at) VALUES('run-export',?1,'job-export','moss_openai','moss-test','source','result','result.json',1,0,0,?2)", params![&project.id, &timestamp]).unwrap();
        database.execute("INSERT INTO transcription_review_items(id,project_id,run_id,segment_id,severity,kind,message,status,created_at) VALUES('review-export',?1,'run-export',?2,'warning','rapid_speaker_switch','check','open',?3)", params![&project.id, &segment.id, &timestamp]).unwrap();

        let rejected = render_structured_export(&database, &project.id, "json", true, false)
            .unwrap_err()
            .to_string();
        assert!(rejected.contains("warning_confirmation_required"));
        let (content, audit) =
            render_structured_export(&database, &project.id, "json", true, true).unwrap();
        let value: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["transcript"]["segments"][0]["id"], segment.id);
        assert_eq!(value["review"]["openWarningCount"], 1);
        assert_eq!(audit["warningsConfirmed"], true);
    }

    #[test]
    fn parses_official_compact_transcript_shape() {
        let value = r#"{"text":"[0.48][S01]Welcome everyone[1.66][12.26][S02]The pipeline is ready[13.81]"}"#;
        let segments = MOSS_PROVIDER.parse(value).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].speaker, "S01");
        assert_eq!(segments[1].start, 12.26);
        assert_eq!(segments[1].text, "The pipeline is ready");
    }

    #[test]
    fn imports_transcript_and_speakers_atomically() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("sample.wav");
        fs::write(&media, b"media").unwrap();
        let mut database = crate::db::open_at(&temp.path().join("test.db")).unwrap();
        let project_value =
            crate::project::create(&mut database, &media, Some("sample".into())).unwrap();
        let timestamp = now();
        database.execute("INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,hotwords_json,status,stage,base_version_id,source_sha256,created_at,updated_at) VALUES('job',?1,?2,?3,?4,'[]','finalizing','validating_result',?5,?6,?7,?7)", params![project_value.id, PROVIDER_ID, DEFAULT_ENDPOINT, DEFAULT_MODEL_ID, project_value.history.current_version_id, project_value.media.sha256, timestamp]).unwrap();
        let job = load(&database, "job").unwrap();
        let raw_path = temp.path().join("run.json");
        let segments = vec![
            ImportedSegment {
                id: "s-one".into(),
                start: 0.0,
                end: 1.0,
                speaker: "S01".into(),
                text: "你好".into(),
            },
            ImportedSegment {
                id: "s-two".into(),
                start: 1.0,
                end: 2.0,
                speaker: "S02".into(),
                text: "你好。".into(),
            },
        ];
        import_result(
            &mut database,
            &job,
            &project_value,
            "run",
            &raw_path,
            "{}",
            &segments,
        )
        .unwrap();
        let loaded = crate::project::load(&database, &project_value.id).unwrap();
        assert_eq!(loaded.transcript.segments.len(), 2);
        let track = crate::speaker::load_track(&database, &project_value.id).unwrap();
        assert_eq!(track.provider_id, PROVIDER_ID);
        assert_eq!(track.speakers.len(), 2);
        assert_eq!(track.associations.len(), 2);
        let completed = load(&database, "job").unwrap();
        assert_eq!(completed.status, "completed");
        assert_eq!(completed.result_run_id.as_deref(), Some("run"));
    }

    #[test]
    fn finalization_rolls_back_project_run_and_job_on_import_failure() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("rollback.wav");
        fs::write(&media, b"media").unwrap();
        let mut database = crate::db::open_at(&temp.path().join("rollback.db")).unwrap();
        let project_value =
            crate::project::create(&mut database, &media, Some("rollback".into())).unwrap();
        crate::project::add_segment(
            &mut database,
            &project_value.id,
            0.0,
            1.0,
            "original".into(),
            None,
        )
        .unwrap();
        let project_value = crate::project::load(&database, &project_value.id).unwrap();
        let timestamp = now();
        database.execute("INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,hotwords_json,status,stage,base_version_id,source_sha256,created_at,updated_at) VALUES('job-rollback',?1,?2,?3,?4,'[]','finalizing','validating_result',?5,?6,?7,?7)", params![project_value.id, PROVIDER_ID, DEFAULT_ENDPOINT, DEFAULT_MODEL_ID, project_value.history.current_version_id, project_value.media.sha256, timestamp]).unwrap();
        let job = load(&database, "job-rollback").unwrap();
        let duplicate_segments = vec![
            ImportedSegment {
                id: "duplicate".into(),
                start: 0.0,
                end: 1.0,
                speaker: "S01".into(),
                text: "first".into(),
            },
            ImportedSegment {
                id: "duplicate".into(),
                start: 1.0,
                end: 2.0,
                speaker: "S02".into(),
                text: "second".into(),
            },
        ];

        let error = import_result(
            &mut database,
            &job,
            &project_value,
            "run-rollback",
            &temp.path().join("run.json"),
            "{}",
            &duplicate_segments,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("transcription_import_failed"));
        let current = crate::project::load(&database, &project_value.id).unwrap();
        assert_eq!(current.transcript.segments.len(), 1);
        assert_eq!(current.transcript.segments[0].text, "original");
        assert_eq!(
            load(&database, "job-rollback").unwrap().status,
            "finalizing"
        );
        let runs: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM transcription_runs WHERE job_id='job-rollback'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(runs, 1);
        let run_status: String = database
            .query_row(
                "SELECT status FROM transcription_runs WHERE job_id='job-rollback'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(run_status, "prepared");
    }

    #[test]
    fn project_changes_preserve_a_candidate_until_explicit_replace() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("pending.wav");
        fs::write(&media, b"media").unwrap();
        let mut database = crate::db::open_at(&temp.path().join("pending.db")).unwrap();
        let base = crate::project::create(&mut database, &media, Some("pending".into())).unwrap();
        let timestamp = now();
        database.execute("INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,hotwords_json,status,stage,base_version_id,source_sha256,created_at,updated_at) VALUES('job-pending',?1,?2,?3,?4,'[]','finalizing','validating_result',?5,?6,?7,?7)", params![base.id, PROVIDER_ID, DEFAULT_ENDPOINT, DEFAULT_MODEL_ID, base.history.current_version_id, base.media.sha256, timestamp]).unwrap();
        crate::project::add_segment(&mut database, &base.id, 0.0, 1.0, "人工编辑".into(), None)
            .unwrap();
        let job = load(&database, "job-pending").unwrap();
        let raw = r#"{"segments":[{"start":0.0,"end":1.0,"speaker":"S01","text":"模型结果。"}]}"#;
        let raw_path = temp.path().join("job-pending.json");
        fs::write(&raw_path, raw).unwrap();
        let segments = vec![ImportedSegment {
            id: "candidate-segment".into(),
            start: 0.0,
            end: 1.0,
            speaker: "S01".into(),
            text: "模型结果。".into(),
        }];

        import_result(
            &mut database,
            &job,
            &base,
            "run-pending",
            &raw_path,
            raw,
            &segments,
        )
        .unwrap();

        let pending = load(&database, "job-pending").unwrap();
        assert_eq!(pending.status, "awaiting_apply");
        assert_eq!(pending.candidate.as_ref().unwrap().segment_count, 1);
        assert_eq!(
            crate::project::load(&database, &base.id)
                .unwrap()
                .transcript
                .segments[0]
                .text,
            "人工编辑"
        );
        let expected = crate::project::current_version_id(&database, &base.id)
            .unwrap()
            .unwrap();
        let confirmation_error = apply_candidate(&mut database, "job-pending", &expected, false)
            .unwrap_err()
            .to_string();
        assert!(confirmation_error.contains("transcription_apply_confirmation_required"));

        let applied = apply_candidate(&mut database, "job-pending", &expected, true).unwrap();
        assert_eq!(applied.status, "completed");
        assert_eq!(
            crate::project::load(&database, &base.id)
                .unwrap()
                .transcript
                .segments[0]
                .text,
            "模型结果。"
        );
        let undone = crate::project::undo(&mut database, &base.id).unwrap();
        assert_eq!(undone.transcript.segments[0].text, "人工编辑");
    }

    #[test]
    fn candidate_apply_rechecks_version_and_discard_removes_result() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("discard.wav");
        fs::write(&media, b"media").unwrap();
        let mut database = crate::db::open_at(&temp.path().join("discard.db")).unwrap();
        let base = crate::project::create(&mut database, &media, Some("discard".into())).unwrap();
        let timestamp = now();
        database.execute("INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,hotwords_json,status,stage,base_version_id,source_sha256,created_at,updated_at) VALUES('job-discard',?1,?2,?3,?4,'[]','finalizing','validating_result',?5,?6,?7,?7)", params![base.id, PROVIDER_ID, DEFAULT_ENDPOINT, DEFAULT_MODEL_ID, base.history.current_version_id, base.media.sha256, timestamp]).unwrap();
        crate::project::add_segment(&mut database, &base.id, 0.0, 1.0, "first edit".into(), None)
            .unwrap();
        let job = load(&database, "job-discard").unwrap();
        let raw = r#"{"segments":[{"start":0.0,"end":1.0,"speaker":"S01","text":"candidate"}]}"#;
        let raw_path = temp.path().join("job-discard.json");
        fs::write(&raw_path, raw).unwrap();
        let segments = parsed_segments(PROVIDER_ID, raw).unwrap();
        import_result(
            &mut database,
            &job,
            &base,
            "run-discard",
            &raw_path,
            raw,
            &segments,
        )
        .unwrap();
        let stale_expected = crate::project::current_version_id(&database, &base.id)
            .unwrap()
            .unwrap();
        crate::project::add_segment(
            &mut database,
            &base.id,
            1.0,
            2.0,
            "second edit".into(),
            None,
        )
        .unwrap();

        let error = apply_candidate(&mut database, "job-discard", &stale_expected, true)
            .unwrap_err()
            .to_string();
        assert!(error.contains("transcription_apply_version_mismatch"));
        assert_eq!(
            load(&database, "job-discard").unwrap().status,
            "awaiting_apply"
        );

        let discarded = discard_candidate(&mut database, "job-discard").unwrap();
        assert_eq!(discarded.status, "discarded");
        assert!(!raw_path.exists());
        let run_status: String = database
            .query_row(
                "SELECT status FROM transcription_runs WHERE id='run-discard'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(run_status, "discarded");
    }

    #[test]
    fn temporary_file_guard_removes_partial_and_wav_files() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("temporary.wav");
        fs::write(&path, b"temporary").unwrap();
        {
            let _guard = TemporaryFile::new(path.clone());
            assert!(path.exists());
        }
        assert!(!path.exists());
    }

    #[test]
    fn prepared_result_finalizes_after_interruption_without_requesting_provider() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("recover.wav");
        fs::write(&media, b"media").unwrap();
        let mut database = crate::db::open_at(&temp.path().join("recover.db")).unwrap();
        let project_value =
            crate::project::create(&mut database, &media, Some("recover".into())).unwrap();
        let timestamp = now();
        database.execute("INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,hotwords_json,status,stage,base_version_id,source_sha256,created_at,updated_at) VALUES('job-recover',?1,?2,?3,?4,'[]','finalizing','validating_result',?5,?6,?7,?7)", params![project_value.id, PROVIDER_ID, DEFAULT_ENDPOINT, DEFAULT_MODEL_ID, project_value.history.current_version_id, project_value.media.sha256, timestamp]).unwrap();
        let raw = r#"{"segments":[{"start":0.0,"end":1.0,"speaker":"S01","text":"recovered"}]}"#;
        let raw_path = temp.path().join("job-recover.json");
        fs::write(&raw_path, raw).unwrap();
        let segments = parsed_segments(PROVIDER_ID, raw).unwrap();
        let job = load(&database, "job-recover").unwrap();
        let run_id = prepare_result(
            &mut database,
            &job,
            &raw_path,
            raw,
            &segments,
            Some("run-recover"),
        )
        .unwrap();
        database.execute("UPDATE transcription_jobs SET status='interrupted',stage='interrupted',worker_pid=NULL WHERE id='job-recover'", []).unwrap();

        database.execute("UPDATE transcription_jobs SET status='running',stage='preparing_audio' WHERE id='job-recover'", []).unwrap();
        let (prepared_id, prepared_path, expected_hash) =
            prepared_run(&database, "job-recover").unwrap().unwrap();
        assert_eq!(prepared_id, run_id);
        mark_finalizing(&database, "job-recover").unwrap();
        let recovered_raw = read_verified_result(&prepared_path, &expected_hash).unwrap();
        let recovered_segments = parsed_segments(PROVIDER_ID, &recovered_raw).unwrap();
        let recovered_job = load(&database, "job-recover").unwrap();
        finalize_result(
            &mut database,
            &recovered_job,
            &project_value,
            &run_id,
            &prepared_path,
            &recovered_raw,
            &recovered_segments,
            None,
        )
        .unwrap();

        assert_eq!(load(&database, "job-recover").unwrap().status, "completed");
        assert_eq!(
            crate::project::load(&database, &project_value.id)
                .unwrap()
                .transcript
                .segments[0]
                .text,
            "recovered"
        );
    }
}
