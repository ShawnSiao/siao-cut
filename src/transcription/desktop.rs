//! Versioned desktop transcription application service. CLI adapters share the job executor.
use super::*;
use crate::write_transaction::WriteTransaction;
use serde_json::{Value, json};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TranscriptionCommand {
    Start {
        mutation_id: String,
        project_id: String,
        expected_version_id: String,
        model_path: String,
        language: String,
    },
    List {
        project_id: Option<String>,
    },
    Get {
        job_id: String,
    },
    Preview {
        job_id: String,
        offset: u32,
    },
    Cancel {
        job_id: String,
    },
    Retry {
        mutation_id: String,
        job_id: String,
    },
    Apply {
        mutation_id: String,
        job_id: String,
        expected_version_id: String,
    },
    Discard {
        mutation_id: String,
        job_id: String,
    },
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    pub job_id: String,
    pub project_id: String,
    pub kind: String,
    pub status: String,
    pub stage: String,
    pub progress: Option<f64>,
    pub attempt: u32,
    pub result: Option<String>,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub available_actions: Vec<String>,
}

pub fn summary(job: &TranscriptionJob) -> JobSummary {
    let status = if job.cancel_requested_at.is_some()
        && matches!(job.status.as_str(), "running" | "queued" | "finalizing")
    {
        "cancelling"
    } else {
        &job.status
    };
    let actions: &[&str] = match status {
        "queued" | "running" | "finalizing" => &["cancel"],
        "awaiting_apply" => &["review", "discard"],
        "failed" | "interrupted" | "cancelled" => &["retry"],
        "completed" => &["open_result"],
        _ => &[],
    };
    JobSummary {
        job_id: job.id.clone(),
        project_id: job.project_id.clone(),
        kind: job.provider_id.clone(),
        status: status.into(),
        stage: job.stage.clone(),
        progress: None,
        attempt: job.attempt_count,
        result: match status {
            "completed" => Some("applied".into()),
            "awaiting_apply" => Some("candidate".into()),
            _ => None,
        },
        error_code: job.error_code.clone(),
        error: job.error_message.clone(),
        available_actions: actions.iter().map(|s| (*s).into()).collect(),
    }
}

fn response(db: &Connection, id: &str) -> Result<Value> {
    let job = load(db, id)?;
    Ok(json!({ "transcriptionJob": job, "jobSummary": summary(&job) }))
}

pub fn execute(db: &mut Connection, request: TranscriptionCommand) -> Result<Value> {
    execute_impl(db, request, true)
}

pub(super) fn execute_impl(
    db: &mut Connection,
    request: TranscriptionCommand,
    spawn: bool,
) -> Result<Value> {
    match &request {
        TranscriptionCommand::List { project_id } => {
            let jobs = list(db, project_id.as_deref())?;
            return Ok(
                json!({"transcriptionJobs": jobs, "jobSummaries": jobs.iter().map(summary).collect::<Vec<_>>() }),
            );
        }
        TranscriptionCommand::Get { job_id } => return response(db, job_id),
        TranscriptionCommand::Preview { job_id, offset } => {
            let job = load(db, job_id)?;
            let (_, path, hash) = prepared_run(db, job_id)?
                .ok_or_else(|| anyhow!("transcription_result_not_ready: 候选结果不可用"))?;
            let raw = read_verified_result(&path, &hash)?;
            let segments = parsed_segments(&job.provider_id, &raw)?;
            let overwritten: u32 = db.query_row(
                "SELECT count(*) FROM segments WHERE project_id=?1",
                [&job.project_id],
                |row| row.get(0),
            )?;
            return Ok(
                json!({ "candidatePreview": { "jobId": job_id, "versionId": project::current_version_id(db, &job.project_id)?, "overwrittenSegments": overwritten, "total": segments.len(), "offset": offset,
                "segments": segments.into_iter().skip(*offset as usize).take(50).map(|item| json!({"start":item.start,"end":item.end,"text":item.text})).collect::<Vec<_>>() } }),
            );
        }
        TranscriptionCommand::Cancel { job_id } => {
            cancel(db, job_id)?;
            return response(db, job_id);
        }
        _ => {}
    }
    let mutation_id = match &request {
        TranscriptionCommand::Start { mutation_id, .. }
        | TranscriptionCommand::Retry { mutation_id, .. }
        | TranscriptionCommand::Apply { mutation_id, .. }
        | TranscriptionCommand::Discard { mutation_id, .. } => mutation_id,
        _ => unreachable!(),
    };
    if mutation_id.is_empty() || mutation_id.len() > 256 {
        bail!("invalid_request: mutationId 无效")
    }
    let request_json = serde_json::to_string(&request)?;
    let mut tx = WriteTransaction::begin(db)?;
    if let Some((stored, id)) = tx
        .query_row(
            "SELECT request_json,job_id FROM transcription_commands WHERE mutation_id=?1",
            [mutation_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if stored != request_json {
            bail!("mutation_id_reused: 请求标识不能用于不同任务")
        }
        let result = response(&tx, &id)?;
        tx.commit()?;
        return Ok(result);
    }
    let mut launch = None;
    let id = match &request {
        TranscriptionCommand::Start {
            project_id,
            expected_version_id,
            model_path,
            language,
            ..
        } => {
            if !["auto", "zh", "en"].contains(&language.as_str()) || model_path.trim().is_empty() {
                bail!("invalid_request: 本地转写参数无效")
            }
            if let Some(job) = latest_active(&tx, project_id)? {
                job.id
            } else {
                let project = project::load(&tx, project_id)?;
                if project.history.current_version_id.as_deref() != Some(expected_version_id) {
                    bail!("project_version_conflict: 项目已变化，请重新启动转写")
                }
                let id = new_id("transcription");
                tx.execute("INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,language,status,stage,base_version_id,source_sha256,created_at,updated_at) VALUES(?1,?2,?3,'',?4,?5,'queued','queued',?6,?7,?8,?8)",
                    params![id, project_id, whisper::PROVIDER, model_path, clean_language(Some(language)), expected_version_id, project.media.sha256, now()])?;
                launch = Some((id.clone(), 1));
                id
            }
        }
        TranscriptionCommand::Retry { job_id, .. } => {
            let job = enqueue_retry(&mut tx, job_id)?;
            launch = Some((job.id.clone(), job.attempt_count));
            job.id
        }
        TranscriptionCommand::Apply {
            job_id,
            expected_version_id,
            ..
        } => {
            apply_candidate(&mut tx, job_id, expected_version_id, true)?;
            job_id.clone()
        }
        TranscriptionCommand::Discard { job_id, .. } => {
            discard_candidate(&mut tx, job_id)?;
            job_id.clone()
        }
        _ => unreachable!(),
    };
    tx.execute("INSERT INTO transcription_commands(mutation_id,request_json,job_id,created_at) VALUES(?1,?2,?3,?4)", params![mutation_id, request_json, id, now()])?;
    tx.commit()?;
    if spawn && let Some((job_id, attempt)) = launch {
        launch_job(db, &job_id, attempt, None)?;
    }
    response(db, &id)
}

pub(super) fn launch_job(
    db: &Connection,
    id: &str,
    attempt: u32,
    delay: Option<u64>,
) -> Result<()> {
    if let Err(error) = spawn_worker(id, attempt, delay) {
        db.execute("UPDATE transcription_jobs SET status='failed',stage='failed',error_message=?2,updated_at=?3 WHERE id=?1 AND status='queued' AND attempt_count=?4", params![id, error.to_string(), now(), attempt])?;
    }
    Ok(())
}
