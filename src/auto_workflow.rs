use crate::{
    cuts, db, export, media,
    model::{AutoWorkflow, AutoWorkflowEvent, SubtitleMode},
    project, source_import, tasks,
    util::{new_id, now},
    video_export::{self, ExportRequest},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    env,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub enum WorkflowInput {
    Local {
        media: PathBuf,
        title: Option<String>,
    },
    Url {
        url: String,
        confirmed_media_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct StartRequest {
    pub input: WorkflowInput,
    pub model: PathBuf,
    pub transcribe_language: Option<String>,
    pub translation_language: Option<String>,
    pub output: PathBuf,
    pub burn_subtitles: bool,
    pub subtitle_mode: SubtitleMode,
    pub start_delay_ms: Option<u64>,
}

pub fn start(db: &mut Connection, request: StartRequest) -> Result<AutoWorkflow> {
    let start_delay_ms = request.start_delay_ms;
    let (workflow, created) = insert_with_flag(db, request)?;
    if !created {
        return Ok(workflow);
    }
    if let Err(error) = spawn_worker(&workflow.id, start_delay_ms) {
        fail(db, &workflow.id, &error)?;
        return Err(error);
    }
    Ok(workflow)
}

#[cfg(test)]
fn insert(db: &mut Connection, request: StartRequest) -> Result<AutoWorkflow> {
    insert_with_flag(db, request).map(|(workflow, _)| workflow)
}

fn insert_with_flag(db: &mut Connection, request: StartRequest) -> Result<(AutoWorkflow, bool)> {
    let StartRequest {
        input,
        model,
        transcribe_language,
        translation_language,
        output,
        burn_subtitles,
        subtitle_mode,
        start_delay_ms: _,
    } = request;
    if !model.is_file() {
        bail!(
            "auto_workflow_model_missing: 模型不存在：{}",
            model.display()
        )
    }
    if output.extension().and_then(|value| value.to_str()) != Some("mp4") {
        bail!("auto_workflow_output_invalid: 自动工作流输出必须使用 .mp4 扩展名")
    }
    if translation_language.is_none()
        && matches!(
            subtitle_mode,
            SubtitleMode::Translated | SubtitleMode::Bilingual
        )
    {
        bail!("auto_workflow_translation_required: 译文或双语输出必须指定 --translate")
    }
    let model = model.canonicalize()?;
    let output = absolute_path(&output)?;
    let (input_kind, input_value, title, confirmed_media_id) = match input {
        WorkflowInput::Local { media, title } => {
            if !media.is_file() {
                bail!(
                    "auto_workflow_media_missing: 媒体文件不存在：{}",
                    media.display()
                )
            }
            (
                "local",
                media.canonicalize()?.to_string_lossy().to_string(),
                title,
                None,
            )
        }
        WorkflowInput::Url {
            url,
            confirmed_media_id,
        } => {
            if confirmed_media_id.trim().is_empty() {
                bail!("auto_workflow_confirmation_required: URL 输入必须提供确认后的站点媒体 ID")
            }
            ("url", url, None, Some(confirmed_media_id.trim().to_owned()))
        }
    };
    let existing = db
        .query_row(
            "SELECT id FROM auto_workflows
             WHERE input_kind=?1 AND input_value=?2 AND output_path=?3
               AND status IN ('queued','running','needs_agent','needs_review','failed','interrupted')
             ORDER BY created_at DESC LIMIT 1",
            params![input_kind, input_value, output.to_string_lossy()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        return Ok((load(db, &existing)?, false));
    }
    let id = new_id("auto");
    let timestamp = now();
    db.execute(
        "INSERT INTO auto_workflows(
             id,input_kind,input_value,title,confirmed_media_id,model_path,
             transcribe_language,translation_language,output_path,burn_subtitles,
             subtitle_mode,status,current_stage,progress,created_at,updated_at,attempt_count
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'queued','import',0,?12,?12,1)",
        params![
            &id,
            input_kind,
            input_value,
            title,
            confirmed_media_id,
            model.to_string_lossy(),
            transcribe_language,
            translation_language,
            output.to_string_lossy(),
            burn_subtitles,
            subtitle_mode.as_str(),
            &timestamp,
        ],
    )?;
    append_event(db, &id, "import", "queued", 0.0, "自动工作流已创建")?;
    Ok((load(db, &id)?, true))
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

pub fn load(db: &Connection, workflow_id: &str) -> Result<AutoWorkflow> {
    db.query_row(
        "SELECT id,input_kind,input_value,title,confirmed_media_id,project_id,source_import_id,
                model_path,transcribe_language,translation_language,output_path,burn_subtitles,
                subtitle_mode,status,current_stage,progress,transcript_version_id,agent_task_id,
                export_job_id,audit_json,cancel_requested_at,error_message,created_at,updated_at,
                completed_at,worker_pid,attempt_count
         FROM auto_workflows WHERE id=?1",
        [workflow_id],
        |row| {
            let audit: Option<String> = row.get(19)?;
            Ok(AutoWorkflow {
                id: row.get(0)?,
                input_kind: row.get(1)?,
                input_value: row.get(2)?,
                title: row.get(3)?,
                confirmed_media_id: row.get(4)?,
                project_id: row.get(5)?,
                source_import_id: row.get(6)?,
                model_path: row.get(7)?,
                transcribe_language: row.get(8)?,
                translation_language: row.get(9)?,
                output_path: row.get(10)?,
                burn_subtitles: row.get(11)?,
                subtitle_mode: SubtitleMode::parse(&row.get::<_, String>(12)?)
                    .ok_or(rusqlite::Error::InvalidQuery)?,
                status: row.get(13)?,
                current_stage: row.get(14)?,
                progress: row.get(15)?,
                transcript_version_id: row.get(16)?,
                agent_task_id: row.get(17)?,
                export_job_id: row.get(18)?,
                audit: audit.and_then(|value| serde_json::from_str(&value).ok()),
                cancel_requested_at: row.get(20)?,
                error_message: row.get(21)?,
                created_at: row.get(22)?,
                updated_at: row.get(23)?,
                completed_at: row.get(24)?,
                worker_pid: row.get(25)?,
                attempt_count: row.get::<_, i64>(26)? as u32,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("auto_workflow_not_found: 自动工作流不存在：{workflow_id}"))
}

pub fn list(db: &Connection) -> Result<Vec<AutoWorkflow>> {
    db.prepare("SELECT id FROM auto_workflows ORDER BY created_at DESC")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| load(db, &id))
        .collect()
}

pub fn events(db: &Connection, workflow_id: &str, after: i64) -> Result<Vec<AutoWorkflowEvent>> {
    load(db, workflow_id)?;
    Ok(db
        .prepare(
            "SELECT id,workflow_id,stage,status,progress,message,created_at
             FROM auto_workflow_events WHERE workflow_id=?1 AND id>?2 ORDER BY id",
        )?
        .query_map(params![workflow_id, after], |row| {
            Ok(AutoWorkflowEvent {
                id: row.get(0)?,
                workflow_id: row.get(1)?,
                stage: row.get(2)?,
                status: row.get(3)?,
                progress: row.get(4)?,
                message: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub(crate) fn agent_result_ready(db: &Connection, task_id: &str) -> Result<()> {
    let workflow_id = db
        .query_row(
            "SELECT id FROM auto_workflows WHERE agent_task_id=?1 AND status='needs_agent'",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(workflow_id) = workflow_id {
        set_state(
            db,
            &workflow_id,
            "review",
            "needs_review",
            0.7,
            "Agent 结果和粗剪建议等待人工确认",
        )?;
    }
    Ok(())
}

pub fn cancel(db: &mut Connection, workflow_id: &str) -> Result<AutoWorkflow> {
    let workflow = load(db, workflow_id)?;
    if matches!(workflow.status.as_str(), "cancelled" | "completed") {
        bail!(
            "auto_workflow_not_cancellable: 自动工作流当前状态不能取消：{}",
            workflow.status
        )
    }
    let timestamp = now();
    db.execute(
        "UPDATE auto_workflows SET cancel_requested_at=?2,updated_at=?2 WHERE id=?1",
        params![workflow_id, &timestamp],
    )?;
    if let Some(job_id) = workflow.source_import_id.as_deref()
        && source_import::load(db, job_id)
            .is_ok_and(|job| matches!(job.status.as_str(), "queued" | "running"))
    {
        let _ = source_import::cancel(db, job_id);
    }
    if let Some(job_id) = workflow.export_job_id.as_deref()
        && video_export::load(db, job_id)
            .is_ok_and(|job| matches!(job.status.as_str(), "queued" | "running"))
    {
        let _ = video_export::cancel(db, job_id);
    }
    if let Some(task_id) = workflow.agent_task_id.as_deref()
        && project_id_for_task(db, task_id).is_some()
    {
        let _ = tasks::cancel(db, task_id);
    }
    if let Some(worker_pid) = workflow.worker_pid
        && worker_pid != std::process::id()
        && crate::util::process_is_active(worker_pid)
    {
        let _ = crate::util::terminate_process_tree_by_id(worker_pid);
    }
    finish_cancelled(db, workflow_id, "自动工作流已取消")?;
    load(db, workflow_id)
}

fn project_id_for_task(db: &Connection, task_id: &str) -> Option<String> {
    db.query_row(
        "SELECT project_id FROM tasks WHERE id=?1 AND status IN ('queued','claimed','failed','interrupted')",
        [task_id],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn continue_workflow(db: &mut Connection, workflow_id: &str) -> Result<AutoWorkflow> {
    let mut workflow = load(db, workflow_id)?;
    match workflow.status.as_str() {
        "queued" | "running" => return Ok(workflow),
        "completed" | "cancelled" => bail!(
            "auto_workflow_not_resumable: 自动工作流当前状态不能继续：{}",
            workflow.status
        ),
        "needs_agent" => {
            let task_id = workflow
                .agent_task_id
                .as_deref()
                .ok_or_else(|| anyhow!("auto_workflow_state_invalid: 缺少 Agent 任务"))?;
            let task_status: String =
                db.query_row("SELECT status FROM tasks WHERE id=?1", [task_id], |row| {
                    row.get(0)
                })?;
            match task_status.as_str() {
                "queued" | "claimed" => return Ok(workflow),
                "failed" | "interrupted" => {
                    tasks::retry(db, task_id)?;
                    db.execute(
                        "UPDATE auto_workflows SET error_message=NULL,updated_at=?2 WHERE id=?1",
                        params![workflow_id, now()],
                    )?;
                    return load(db, workflow_id);
                }
                "review" => {
                    set_state(
                        db,
                        workflow_id,
                        "review",
                        "needs_review",
                        0.7,
                        "Agent 结果和粗剪建议等待人工确认",
                    )?;
                    return load(db, workflow_id);
                }
                "done" => {}
                _ => bail!("auto_workflow_state_invalid: Agent 任务状态无效：{task_status}"),
            }
        }
        "needs_review" => {}
        "failed" | "interrupted" => {
            resume_child_job(db, &workflow)?;
        }
        _ => bail!("auto_workflow_state_invalid: 未知状态：{}", workflow.status),
    }
    workflow = load(db, workflow_id)?;
    if workflow.status == "needs_agent" && review_pending(db, &workflow)? {
        set_state(
            db,
            workflow_id,
            "review",
            "needs_review",
            0.7,
            "Agent 结果已审阅；粗剪建议仍等待人工确认",
        )?;
        return load(db, workflow_id);
    }
    if workflow.status == "needs_review" && review_pending(db, &workflow)? {
        bail!("auto_workflow_review_pending: 仍有 Agent 修改或粗剪建议等待人工处理")
    }
    let next_stage = if matches!(workflow.current_stage.as_str(), "translate" | "review") {
        "audit"
    } else {
        workflow.current_stage.as_str()
    };
    db.execute(
        "UPDATE auto_workflows
         SET status='queued',current_stage=?2,cancel_requested_at=NULL,error_message=NULL,
             completed_at=NULL,worker_pid=NULL,attempt_count=attempt_count+1,updated_at=?3
         WHERE id=?1",
        params![workflow_id, next_stage, now()],
    )?;
    append_event(
        db,
        workflow_id,
        next_stage,
        "queued",
        workflow.progress,
        "自动工作流显式继续",
    )?;
    spawn_worker(workflow_id, None)?;
    load(db, workflow_id)
}

fn resume_child_job(db: &Connection, workflow: &AutoWorkflow) -> Result<()> {
    if workflow.current_stage == "import"
        && let Some(job_id) = workflow.source_import_id.as_deref()
    {
        let job = source_import::load(db, job_id)?;
        if matches!(job.status.as_str(), "failed" | "interrupted" | "cancelled") {
            source_import::resume(db, job_id)?;
        }
    }
    if workflow.current_stage == "export"
        && let Some(job_id) = workflow.export_job_id.as_deref()
    {
        let job = video_export::load(db, job_id)?;
        if matches!(job.status.as_str(), "failed" | "interrupted") {
            video_export::retry(db, job_id)?;
        }
    }
    Ok(())
}

fn review_pending(db: &Connection, workflow: &AutoWorkflow) -> Result<bool> {
    let Some(project_id) = workflow.project_id.as_deref() else {
        return Ok(false);
    };
    let proposed: i64 = db.query_row(
        "SELECT COUNT(*) FROM edits WHERE project_id=?1 AND status='proposed'",
        [project_id],
        |row| row.get(0),
    )?;
    let unresolved = if let Some(task_id) = workflow.agent_task_id.as_deref() {
        db.query_row(
            "SELECT COUNT(*) FROM agent_patch_items i
             JOIN agent_patch_sets s ON s.id=i.patch_set_id
             WHERE s.task_id=?1 AND i.status IN ('pending','conflict')",
            [task_id],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    Ok(proposed > 0 || unresolved > 0)
}

pub fn reconcile_interrupted(db: &Connection) -> Result<()> {
    let workflows = db
        .prepare("SELECT id FROM auto_workflows WHERE status IN ('queued','running')")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for workflow_id in workflows {
        let workflow = load(db, &workflow_id)?;
        let stale = chrono::DateTime::parse_from_rfc3339(&workflow.updated_at)
            .map(|time| {
                chrono::Utc::now()
                    .signed_duration_since(time.with_timezone(&chrono::Utc))
                    .num_seconds()
                    >= 5
            })
            .unwrap_or(true);
        let worker_alive = workflow
            .worker_pid
            .is_some_and(crate::util::process_is_active);
        if stale && !worker_alive {
            db.execute(
                "UPDATE auto_workflows
                 SET status='interrupted',worker_pid=NULL,error_message='自动工作流进程已中断；需要显式继续。',updated_at=?2
                 WHERE id=?1",
                params![&workflow_id, now()],
            )?;
            append_event(
                db,
                &workflow_id,
                &workflow.current_stage,
                "interrupted",
                workflow.progress,
                "自动工作流进程已中断",
            )?;
        }
    }
    Ok(())
}

fn spawn_worker(workflow_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let delay = start_delay_ms.map(|value| value.to_string());
    let mut arguments = vec!["__auto_worker", workflow_id];
    if let Some(delay) = delay.as_deref() {
        arguments.push(delay);
    }
    crate::util::spawn_detached_current(&arguments).context("无法启动自动工作流")?;
    Ok(())
}

pub fn run_worker(workflow_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let mut db = db::open()?;
    let timestamp = now();
    db.execute(
        "UPDATE auto_workflows
         SET status='running',worker_pid=?2,error_message=NULL,updated_at=?3
         WHERE id=?1 AND status IN ('queued','running','interrupted','failed')",
        params![workflow_id, std::process::id(), &timestamp],
    )?;
    if let Some(delay) = start_delay_ms {
        thread::sleep(Duration::from_millis(delay));
    }
    if let Err(error) = run_steps(&mut db, workflow_id) {
        if load(&db, workflow_id).is_ok_and(|workflow| workflow.status != "cancelled") {
            let _ = fail(&db, workflow_id, &error);
        }
        return Err(error);
    }
    Ok(())
}

fn run_steps(db: &mut Connection, workflow_id: &str) -> Result<()> {
    loop {
        let workflow = load(db, workflow_id)?;
        if workflow.cancel_requested_at.is_some() {
            finish_cancelled(db, workflow_id, "自动工作流已按取消请求停止")?;
            return Ok(());
        }
        match workflow.current_stage.as_str() {
            "import" => run_import(db, &workflow)?,
            "transcribe" => run_transcribe(db, &workflow)?,
            "suggestions" => {
                if run_suggestions(db, &workflow)? {
                    return Ok(());
                }
            }
            "translate" => {
                if sync_agent_stage(db, &workflow)? {
                    return Ok(());
                }
            }
            "review" => {
                if review_pending(db, &workflow)? {
                    set_state(
                        db,
                        workflow_id,
                        "review",
                        "needs_review",
                        0.7,
                        "Agent 结果和粗剪建议等待人工确认",
                    )?;
                    return Ok(());
                }
                set_state(db, workflow_id, "audit", "running", 0.78, "人工确认已完成")?;
            }
            "audit" => run_audit(db, &workflow)?,
            "export" => {
                if poll_export(db, &workflow)? {
                    return Ok(());
                }
            }
            "complete" => return Ok(()),
            stage => bail!("auto_workflow_state_invalid: 未知阶段：{stage}"),
        }
    }
}

fn run_import(db: &mut Connection, workflow: &AutoWorkflow) -> Result<()> {
    if workflow.input_kind == "local" {
        let project_id = deterministic_id("p-auto", &workflow.id);
        let project = project::create_with_id(
            db,
            Path::new(&workflow.input_value),
            workflow.title.clone(),
            &project_id,
        )?;
        db.execute(
            "UPDATE auto_workflows SET project_id=?2 WHERE id=?1",
            params![&workflow.id, &project.id],
        )?;
        set_state(
            db,
            &workflow.id,
            "transcribe",
            "running",
            0.15,
            "本地媒体已导入",
        )?;
        return Ok(());
    }
    let job_id = deterministic_id("src-auto", &workflow.id);
    let confirmed = workflow
        .confirmed_media_id
        .as_deref()
        .ok_or_else(|| anyhow!("auto_workflow_state_invalid: URL 输入缺少确认媒体 ID"))?;
    let job =
        source_import::start_with_job_id(db, &workflow.input_value, confirmed, None, &job_id)?;
    db.execute(
        "UPDATE auto_workflows SET source_import_id=?2,updated_at=?3 WHERE id=?1",
        params![&workflow.id, &job.id, now()],
    )?;
    loop {
        let current = load(db, &workflow.id)?;
        if current.cancel_requested_at.is_some() {
            if source_import::load(db, &job.id)
                .is_ok_and(|job| matches!(job.status.as_str(), "queued" | "running"))
            {
                let _ = source_import::cancel(db, &job.id);
            }
            finish_cancelled(db, &workflow.id, "URL 导入已取消")?;
            return Ok(());
        }
        let source = source_import::load(db, &job.id)?;
        match source.status.as_str() {
            "queued" | "running" | "finalizing" => {
                update_progress(db, &workflow.id, 0.02 + source.progress * 0.13)?;
                thread::sleep(POLL_INTERVAL);
            }
            "completed" => {
                let project_id = source.project_id.ok_or_else(|| {
                    anyhow!("auto_workflow_state_invalid: URL 导入完成但没有项目")
                })?;
                db.execute(
                    "UPDATE auto_workflows SET project_id=?2 WHERE id=?1",
                    params![&workflow.id, project_id],
                )?;
                set_state(
                    db,
                    &workflow.id,
                    "transcribe",
                    "running",
                    0.15,
                    "URL 媒体已导入",
                )?;
                return Ok(());
            }
            "cancelled" => {
                finish_cancelled(db, &workflow.id, "URL 导入已取消")?;
                return Ok(());
            }
            "failed" | "interrupted" => bail!(
                "auto_workflow_source_failed: {}",
                source
                    .error_message
                    .unwrap_or_else(|| "URL 导入未完成".into())
            ),
            status => bail!("auto_workflow_state_invalid: URL 导入状态无效：{status}"),
        }
    }
}

fn run_transcribe(db: &mut Connection, workflow: &AutoWorkflow) -> Result<()> {
    let project_id = workflow
        .project_id
        .as_deref()
        .ok_or_else(|| anyhow!("auto_workflow_state_invalid: 转录阶段缺少项目"))?;
    if let Some(version_id) = transcription_version(db, project_id)? {
        db.execute(
            "UPDATE auto_workflows SET transcript_version_id=?2 WHERE id=?1",
            params![&workflow.id, version_id],
        )?;
    } else {
        media::transcribe(
            db,
            project_id,
            Path::new(&workflow.model_path),
            workflow.transcribe_language.as_deref(),
        )?;
        let version_id = transcription_version(db, project_id)?
            .ok_or_else(|| anyhow!("auto_workflow_state_invalid: 转录完成但缺少版本证据"))?;
        db.execute(
            "UPDATE auto_workflows SET transcript_version_id=?2 WHERE id=?1",
            params![&workflow.id, version_id],
        )?;
    }
    set_state(
        db,
        &workflow.id,
        "suggestions",
        "running",
        0.5,
        "本地转录已完成",
    )?;
    Ok(())
}

fn transcription_version(db: &Connection, project_id: &str) -> Result<Option<String>> {
    db.query_row(
        "SELECT id FROM versions WHERE project_id=?1 AND reason='whisper.cpp 本地转录' ORDER BY history_index DESC LIMIT 1",
        [project_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn run_suggestions(db: &mut Connection, workflow: &AutoWorkflow) -> Result<bool> {
    let project_id = workflow
        .project_id
        .as_deref()
        .ok_or_else(|| anyhow!("auto_workflow_state_invalid: 粗剪阶段缺少项目"))?;
    let suggestions = cuts::detect(db, project_id)?;
    if let Some(language) = workflow.translation_language.clone() {
        let task_id = workflow
            .agent_task_id
            .clone()
            .or_else(|| task_for_workflow(db, &workflow.id).ok().flatten())
            .map(Ok)
            .unwrap_or_else(|| {
                tasks::create_for_workflow(
                    db,
                    project_id,
                    "translate",
                    Some(language),
                    Some(&workflow.id),
                )
                .map(|task| task.id)
            })?;
        db.execute(
            "UPDATE auto_workflows SET agent_task_id=?2 WHERE id=?1",
            params![&workflow.id, task_id],
        )?;
        set_state(
            db,
            &workflow.id,
            "translate",
            "needs_agent",
            0.62,
            &format!(
                "已生成 {} 条粗剪建议；翻译任务等待 Agent",
                suggestions.len()
            ),
        )?;
        return Ok(true);
    }
    if suggestions.is_empty() {
        set_state(db, &workflow.id, "audit", "running", 0.78, "未发现粗剪建议")?;
        Ok(false)
    } else {
        set_state(
            db,
            &workflow.id,
            "review",
            "needs_review",
            0.7,
            &format!("{} 条粗剪建议等待人工确认", suggestions.len()),
        )?;
        Ok(true)
    }
}

fn task_for_workflow(db: &Connection, workflow_id: &str) -> Result<Option<String>> {
    db.query_row(
        "SELECT id FROM tasks WHERE workflow_id=?1 ORDER BY created_at LIMIT 1",
        [workflow_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn sync_agent_stage(db: &mut Connection, workflow: &AutoWorkflow) -> Result<bool> {
    let task_id = workflow
        .agent_task_id
        .as_deref()
        .ok_or_else(|| anyhow!("auto_workflow_state_invalid: 翻译阶段缺少 Agent 任务"))?;
    let status: String =
        db.query_row("SELECT status FROM tasks WHERE id=?1", [task_id], |row| {
            row.get(0)
        })?;
    match status.as_str() {
        "queued" | "claimed" | "failed" | "interrupted" => {
            set_state(
                db,
                &workflow.id,
                "translate",
                "needs_agent",
                0.62,
                "翻译任务等待 Agent 完成",
            )?;
            Ok(true)
        }
        "review" => {
            set_state(
                db,
                &workflow.id,
                "review",
                "needs_review",
                0.7,
                "Agent 结果和粗剪建议等待人工确认",
            )?;
            Ok(true)
        }
        "done" => {
            set_state(
                db,
                &workflow.id,
                "review",
                "running",
                0.72,
                "Agent 修改已审阅",
            )?;
            Ok(false)
        }
        "cancelled" => bail!("auto_workflow_agent_cancelled: Agent 任务已取消"),
        _ => bail!("auto_workflow_state_invalid: Agent 任务状态无效：{status}"),
    }
}

fn run_audit(db: &mut Connection, workflow: &AutoWorkflow) -> Result<()> {
    let project_id = workflow
        .project_id
        .as_deref()
        .ok_or_else(|| anyhow!("auto_workflow_state_invalid: 审计阶段缺少项目"))?;
    let project = project::load(db, project_id)?;
    let report = export::audit(&project);
    db.execute(
        "UPDATE auto_workflows SET audit_json=?2,updated_at=?3 WHERE id=?1",
        params![&workflow.id, serde_json::to_string(&report)?, now()],
    )?;
    if report["ready"] != true {
        bail!("auto_workflow_audit_failed: 导出前审计未通过")
    }
    let job_id = deterministic_id("x-auto", &workflow.id);
    let job = video_export::load(db, &job_id).or_else(|_| {
        video_export::create(
            db,
            project_id,
            ExportRequest {
                output: Path::new(&workflow.output_path),
                burn_subtitles: workflow.burn_subtitles,
                language: workflow.translation_language.clone(),
                subtitle_mode: workflow.subtitle_mode,
                start_delay_ms: None,
                job_id: Some(job_id.clone()),
            },
        )
    })?;
    db.execute(
        "UPDATE auto_workflows SET export_job_id=?2 WHERE id=?1",
        params![&workflow.id, &job.id],
    )?;
    set_state(
        db,
        &workflow.id,
        "export",
        "running",
        0.85,
        "审计通过，开始导出",
    )?;
    Ok(())
}

fn poll_export(db: &Connection, workflow: &AutoWorkflow) -> Result<bool> {
    let job_id = workflow
        .export_job_id
        .as_deref()
        .ok_or_else(|| anyhow!("auto_workflow_state_invalid: 导出阶段缺少任务"))?;
    loop {
        let current = load(db, &workflow.id)?;
        if current.cancel_requested_at.is_some() {
            if video_export::load(db, job_id)
                .is_ok_and(|job| matches!(job.status.as_str(), "queued" | "running"))
            {
                let _ = video_export::cancel(db, job_id);
            }
            finish_cancelled(db, &workflow.id, "视频导出已取消")?;
            return Ok(true);
        }
        let job = video_export::load(db, job_id)?;
        match job.status.as_str() {
            "queued" | "running" => {
                update_progress(db, &workflow.id, 0.85 + job.progress * 0.14)?;
                thread::sleep(POLL_INTERVAL);
            }
            "completed" => {
                complete(db, &workflow.id)?;
                return Ok(true);
            }
            "cancelled" => {
                finish_cancelled(db, &workflow.id, "视频导出已取消")?;
                return Ok(true);
            }
            "failed" | "interrupted" => bail!(
                "auto_workflow_export_failed: {}",
                job.error_message.unwrap_or_else(|| "视频导出未完成".into())
            ),
            status => bail!("auto_workflow_state_invalid: 导出任务状态无效：{status}"),
        }
    }
}

fn deterministic_id(prefix: &str, workflow_id: &str) -> String {
    format!("{prefix}-{}", workflow_id.trim_start_matches("auto-"))
}

fn set_state(
    db: &Connection,
    workflow_id: &str,
    stage: &str,
    status: &str,
    progress: f64,
    message: &str,
) -> Result<()> {
    let timestamp = now();
    let worker_pid = if matches!(status, "needs_agent" | "needs_review") {
        None
    } else {
        Some(std::process::id())
    };
    db.execute(
        "UPDATE auto_workflows
         SET current_stage=?2,status=?3,progress=?4,worker_pid=?5,updated_at=?6
         WHERE id=?1",
        params![workflow_id, stage, status, progress, worker_pid, &timestamp],
    )?;
    append_event(db, workflow_id, stage, status, progress, message)
}

fn update_progress(db: &Connection, workflow_id: &str, progress: f64) -> Result<()> {
    db.execute(
        "UPDATE auto_workflows SET progress=?2,updated_at=?3 WHERE id=?1",
        params![workflow_id, progress.clamp(0.0, 0.99), now()],
    )?;
    Ok(())
}

fn complete(db: &Connection, workflow_id: &str) -> Result<()> {
    let timestamp = now();
    db.execute(
        "UPDATE auto_workflows
         SET status='completed',current_stage='complete',progress=1,worker_pid=NULL,
             completed_at=?2,updated_at=?2,error_message=NULL WHERE id=?1",
        params![workflow_id, &timestamp],
    )?;
    append_event(
        db,
        workflow_id,
        "complete",
        "completed",
        1.0,
        "自动工作流已完成",
    )
}

fn finish_cancelled(db: &Connection, workflow_id: &str, message: &str) -> Result<()> {
    let timestamp = now();
    db.execute(
        "UPDATE auto_workflows
         SET status='cancelled',worker_pid=NULL,completed_at=?2,updated_at=?2 WHERE id=?1",
        params![workflow_id, &timestamp],
    )?;
    let workflow = load(db, workflow_id)?;
    append_event(
        db,
        workflow_id,
        &workflow.current_stage,
        "cancelled",
        workflow.progress,
        message,
    )
}

fn fail(db: &Connection, workflow_id: &str, error: &anyhow::Error) -> Result<()> {
    let timestamp = now();
    db.execute(
        "UPDATE auto_workflows
         SET status='failed',worker_pid=NULL,error_message=?2,updated_at=?3 WHERE id=?1 AND status!='cancelled'",
        params![workflow_id, error.to_string(), &timestamp],
    )?;
    let workflow = load(db, workflow_id)?;
    append_event(
        db,
        workflow_id,
        &workflow.current_stage,
        "failed",
        workflow.progress,
        &error.to_string(),
    )
}

fn append_event(
    db: &Connection,
    workflow_id: &str,
    stage: &str,
    status: &str,
    progress: f64,
    message: &str,
) -> Result<()> {
    db.execute(
        "INSERT INTO auto_workflow_events(workflow_id,stage,status,progress,message,created_at)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![workflow_id, stage, status, progress, message, now()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project;
    use std::fs;
    use tempfile::tempdir;

    fn fixture() -> (tempfile::TempDir, Connection, PathBuf, PathBuf, PathBuf) {
        let temp = tempdir().unwrap();
        let db = db::open_at(&temp.path().join("auto.db")).unwrap();
        let media = temp.path().join("talk.wav");
        let model = temp.path().join("model.bin");
        let output = temp.path().join("out.mp4");
        fs::write(&media, b"audio").unwrap();
        fs::write(&model, b"model").unwrap();
        (temp, db, media, model, output)
    }

    fn insert_local(
        db: &mut Connection,
        media: &Path,
        model: &Path,
        output: &Path,
        translation: Option<&str>,
    ) -> AutoWorkflow {
        insert(
            db,
            StartRequest {
                input: WorkflowInput::Local {
                    media: media.to_path_buf(),
                    title: Some("Auto fixture".into()),
                },
                model: model.to_path_buf(),
                transcribe_language: Some("auto".into()),
                translation_language: translation.map(str::to_owned),
                output: output.to_path_buf(),
                burn_subtitles: false,
                subtitle_mode: SubtitleMode::Source,
                start_delay_ms: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn auto_workflow_persists_contract_events_and_idempotent_local_import() {
        let (_temp, mut db, media, model, output) = fixture();
        let workflow = insert_local(&mut db, &media, &model, &output, None);
        assert_eq!(workflow.status, "queued");
        assert_eq!(workflow.current_stage, "import");
        run_import(&mut db, &workflow).unwrap();
        let imported = load(&db, &workflow.id).unwrap();
        assert_eq!(imported.current_stage, "transcribe");
        let project_id = imported.project_id.clone().unwrap();
        let project_count: i64 = db
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(project_count, 1);
        db.execute(
            "UPDATE auto_workflows SET current_stage='import' WHERE id=?1",
            [&workflow.id],
        )
        .unwrap();
        let retry_import = load(&db, &workflow.id).unwrap();
        run_import(&mut db, &retry_import).unwrap();
        assert_eq!(
            load(&db, &workflow.id).unwrap().project_id,
            Some(project_id)
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM projects", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(events(&db, &workflow.id, 0).unwrap().len() >= 3);
    }

    #[test]
    fn auto_workflow_recovery_reuses_transcript_version_and_requires_review() {
        let (_temp, mut db, media, model, output) = fixture();
        let workflow = insert_local(&mut db, &media, &model, &output, None);
        run_import(&mut db, &workflow).unwrap();
        let imported = load(&db, &workflow.id).unwrap();
        let project_id = imported.project_id.clone().unwrap();
        let segment =
            project::add_segment(&mut db, &project_id, 0.0, 1.2, "we um start".into(), None)
                .unwrap();
        for (ordinal, (id, text, start, end)) in [
            ("w1", "we", 0.0, 0.25),
            ("w2", "um", 0.35, 0.55),
            ("w3", "start", 0.7, 1.1),
        ]
        .into_iter()
        .enumerate()
        {
            db.execute(
                "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    id,
                    &project_id,
                    &segment.id,
                    start,
                    end,
                    text,
                    ordinal as i64
                ],
            )
            .unwrap();
        }
        let version = project::snapshot(&db, &project_id, "whisper.cpp 本地转录").unwrap();
        let interrupted = load(&db, &workflow.id).unwrap();
        run_transcribe(&mut db, &interrupted).unwrap();
        let transcribed = load(&db, &workflow.id).unwrap();
        assert_eq!(transcribed.transcript_version_id, Some(version.id));
        assert_eq!(transcribed.current_stage, "suggestions");
        assert!(run_suggestions(&mut db, &transcribed).unwrap());
        let review = load(&db, &workflow.id).unwrap();
        assert_eq!(review.status, "needs_review");
        assert_eq!(review.current_stage, "review");
        assert!(
            continue_workflow(&mut db, &workflow.id)
                .unwrap_err()
                .to_string()
                .contains("auto_workflow_review_pending")
        );
        let project = project::load(&db, &project_id).unwrap();
        assert_eq!(project.edits.len(), 1);
        assert_eq!(project.edits[0].status, "proposed");
    }

    #[test]
    fn auto_workflow_translation_waits_for_agent_and_never_applies_content() {
        let (_temp, mut db, media, model, output) = fixture();
        let workflow = insert_local(&mut db, &media, &model, &output, Some("zh"));
        run_import(&mut db, &workflow).unwrap();
        let imported = load(&db, &workflow.id).unwrap();
        let project_id = imported.project_id.clone().unwrap();
        let segment =
            project::add_segment(&mut db, &project_id, 0.0, 1.0, "hello".into(), None).unwrap();
        let version = project::snapshot(&db, &project_id, "whisper.cpp 本地转录").unwrap();
        db.execute(
            "UPDATE auto_workflows SET current_stage='suggestions',transcript_version_id=?2 WHERE id=?1",
            params![&workflow.id, &version.id],
        )
        .unwrap();
        let transcribed = load(&db, &workflow.id).unwrap();
        assert!(run_suggestions(&mut db, &transcribed).unwrap());
        let waiting = load(&db, &workflow.id).unwrap();
        assert_eq!(waiting.status, "needs_agent");
        assert!(waiting.agent_task_id.is_some());
        let project = project::load(&db, &project_id).unwrap();
        assert!(project.translations.is_empty());
        assert!(project.edits.is_empty());
        let claim = tasks::claim(&mut db, "auto-agent").unwrap().unwrap();
        let base = claim.2["baseVersionId"].as_str().unwrap();
        tasks::submit(
            &mut db,
            waiting.agent_task_id.as_deref().unwrap(),
            "auto-agent",
            serde_json::json!({
                "baseVersionId": base,
                "patches": [{
                    "segmentId": segment.id,
                    "before": "hello",
                    "after": "你好",
                    "reason": "翻译",
                    "confidence": 0.99
                }]
            }),
        )
        .unwrap();
        assert_eq!(load(&db, &workflow.id).unwrap().status, "needs_review");
        assert!(
            project::load(&db, &project_id)
                .unwrap()
                .translations
                .is_empty()
        );
    }

    #[test]
    fn auto_workflow_reconciles_a_stale_worker_as_interrupted() {
        let (_temp, mut db, media, model, output) = fixture();
        let workflow = insert_local(&mut db, &media, &model, &output, None);
        db.execute(
            "UPDATE auto_workflows
             SET status='running',worker_pid=4294967295,updated_at='2000-01-01T00:00:00+00:00'
             WHERE id=?1",
            [&workflow.id],
        )
        .unwrap();
        reconcile_interrupted(&db).unwrap();
        let interrupted = load(&db, &workflow.id).unwrap();
        assert_eq!(interrupted.status, "interrupted");
        assert!(interrupted.worker_pid.is_none());
        assert!(interrupted.error_message.unwrap().contains("显式继续"));
        assert_eq!(
            events(&db, &workflow.id, 0).unwrap().last().unwrap().status,
            "interrupted"
        );
    }
}
