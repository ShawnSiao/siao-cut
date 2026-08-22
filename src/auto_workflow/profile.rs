use super::{POLL_INTERVAL, finish_cancelled, load, set_state, update_progress};
use crate::{
    audio_analysis,
    model::{AutoWorkflow, SubtitleMode, WorkflowProfile},
    util::now,
};
use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, params};
use std::thread;

pub(super) fn validate_start_profile(
    profile: WorkflowProfile,
    has_translation: bool,
    has_ai_execution: bool,
    subtitle_mode: SubtitleMode,
) -> Result<()> {
    if profile == WorkflowProfile::Draft
        && (has_translation || has_ai_execution || subtitle_mode != SubtitleMode::Source)
    {
        bail!("auto_workflow_profile_invalid: 快速初稿禁止翻译、AI 执行和非原文字幕模式")
    }
    if profile == WorkflowProfile::Delivery {
        audio_analysis::ensure_available()?;
    }
    Ok(())
}

pub(super) fn stage_start(profile: WorkflowProfile, stage: &str) -> f64 {
    match (profile, stage) {
        (WorkflowProfile::Draft, "transcribe") => 0.15,
        (WorkflowProfile::Draft, "audit") => 0.70,
        (WorkflowProfile::Draft, "export") => 0.75,
        (WorkflowProfile::Balanced, "transcribe") => 0.15,
        (WorkflowProfile::Balanced, "suggestions") => 0.45,
        (WorkflowProfile::Balanced, "translate" | "review") => 0.50,
        (WorkflowProfile::Balanced, "audit") => 0.75,
        (WorkflowProfile::Balanced, "export") => 0.80,
        (WorkflowProfile::Delivery, "transcribe") => 0.10,
        (WorkflowProfile::Delivery, "analyze") => 0.40,
        (WorkflowProfile::Delivery, "suggestions") => 0.55,
        (WorkflowProfile::Delivery, "translate" | "review") => 0.60,
        (WorkflowProfile::Delivery, "audit") => 0.80,
        (WorkflowProfile::Delivery, "export") => 0.85,
        (_, "complete") => 1.0,
        _ => 0.0,
    }
}

pub(super) fn after_transcription(profile: WorkflowProfile) -> (&'static str, &'static str) {
    match profile {
        WorkflowProfile::Draft => (
            "audit",
            "本地转录已完成；快速初稿不运行粗剪检测或 AI，结果未经建议审阅",
        ),
        WorkflowProfile::Balanced => ("suggestions", "本地转录已完成"),
        WorkflowProfile::Delivery => ("analyze", "本地转录已完成"),
    }
}

pub(super) fn poll_audio_analysis(db: &Connection, workflow: &AutoWorkflow) -> Result<bool> {
    let project_id = workflow
        .project_id
        .as_deref()
        .ok_or_else(|| anyhow!("auto_workflow_state_invalid: 音频分析阶段缺少项目"))?;
    let job = if let Some(job_id) = workflow.audio_analysis_job_id.as_deref() {
        audio_analysis::load(db, job_id)?
    } else {
        let job = audio_analysis::start(db, project_id, None)?;
        db.execute(
            "UPDATE auto_workflows SET audio_analysis_job_id=?2,updated_at=?3 WHERE id=?1",
            params![&workflow.id, &job.id, now()],
        )?;
        job
    };
    loop {
        let current = load(db, &workflow.id)?;
        if current.cancel_requested_at.is_some() {
            if audio_analysis::load(db, &job.id)
                .is_ok_and(|child| matches!(child.status.as_str(), "queued" | "running"))
            {
                let _ = audio_analysis::cancel(db, &job.id);
            }
            finish_cancelled(db, &workflow.id, "本地音频分析已取消")?;
            return Ok(true);
        }
        let child = audio_analysis::load(db, &job.id)?;
        match child.status.as_str() {
            "queued" | "running" => {
                let start = stage_start(workflow.profile, "analyze");
                let end = stage_start(workflow.profile, "suggestions");
                update_progress(db, &workflow.id, start + child.progress * (end - start))?;
                thread::sleep(POLL_INTERVAL);
            }
            "completed" => {
                set_state(
                    db,
                    &workflow.id,
                    "suggestions",
                    "running",
                    stage_start(workflow.profile, "suggestions"),
                    "本地音频分析已完成",
                )?;
                return Ok(false);
            }
            "cancelled" => {
                finish_cancelled(db, &workflow.id, "本地音频分析已取消")?;
                return Ok(true);
            }
            "failed" | "interrupted" => bail!(
                "auto_workflow_audio_analysis_failed: {}",
                child
                    .error_message
                    .unwrap_or_else(|| "本地音频分析未完成".into())
            ),
            status => bail!("auto_workflow_state_invalid: 音频分析任务状态无效：{status}"),
        }
    }
}
