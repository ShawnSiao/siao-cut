//! Consent is a durable capability for one immutable outbound task and one run.
pub use crate::ai_approval_contract::*;
use crate::{
    agent::{api_executor, execution::ExecutionTarget},
    agent_runner, project, tasks,
    util::{new_id, now},
    workflows,
    write_transaction::WriteTransaction,
};
use anyhow::{Result, anyhow, bail};
#[path = "ai_approval_target.rs"]
mod target;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use target::configuration;

pub fn execute(db: &mut Connection, request: AiApprovalRequest) -> Result<Value> {
    match request {
        AiApprovalRequest::Preview { spec } => Ok(json!({"aiSendPreview": preview(db, spec)?})),
        AiApprovalRequest::Execute { approval_id } => {
            let (run, launch) = consume(db, &approval_id)?;
            let run = if launch {
                agent_runner::launch_queued_run(db, &run.id, None)?
            } else {
                run
            };
            Ok(json!({"agentRun": run}))
        }
    }
}

fn build_payload(db: &mut Connection, spec: &AiSendSpec) -> Result<Value> {
    let project = project::load(db, &spec.project_id)?;
    if project.history.current_version_id.as_deref() != Some(&spec.expected_version_id) {
        bail!("ai_approval_stale: 项目版本已变化，请重新预检并确认")
    }
    let payload = if let Some(id) = &spec.task_id {
        let task = project
            .tasks
            .iter()
            .find(|task| &task.id == id)
            .ok_or_else(|| anyhow!("ai_approval_stale: 授权任务不存在"))?;
        if task.kind != spec.kind
            || task.language != spec.language
            || task.instruction_locale != spec.instruction_locale
            || task.status != "queued"
        {
            bail!("ai_approval_stale: 任务范围或状态已变化，请重新预检")
        }
        if task.base_version_id == project.history.current_version_id {
            tasks::build_claim_payload(db, &project, task)?
        } else {
            let mut tx = WriteTransaction::begin(db)?;
            let fresh = tasks::create_for_workflow(
                &mut tx,
                &spec.project_id,
                &spec.kind,
                spec.language.clone(),
                task.workflow_id.as_deref(),
                &spec.instruction_locale,
            )?;
            tasks::build_claim_payload(&tx, &project, &fresh)?
        }
    } else {
        // Reuse the real builder, but roll back the temporary task and all events.
        let mut tx = WriteTransaction::begin(db)?;
        let task = tasks::create_with_locale(
            &mut tx,
            &spec.project_id,
            &spec.kind,
            spec.language.clone(),
            &spec.instruction_locale,
        )?;
        tasks::build_claim_payload(&tx, &project, &task)?
    };
    api_executor::remote_payload(&payload)
}

pub fn preview(db: &mut Connection, spec: AiSendSpec) -> Result<AiSendPreview> {
    let mut tx = WriteTransaction::begin(db)?;
    let payload = build_payload(&mut tx, &spec)?;
    let segments = payload["segments"]
        .as_array()
        .ok_or_else(|| anyhow!("invalid_request: 字幕为空"))?;
    if segments.is_empty() {
        bail!("invalid_request: 字幕为空")
    }
    let target = configuration(&spec.target)?;
    let payload_json = serde_json::to_string(&payload)?;
    let preview = AiSendPreview {
        approval_id: new_id("approval"),
        payload_hash: format!("{:x}", Sha256::digest(payload_json.as_bytes())),
        receiver: target.receiver,
        endpoint: target.endpoint,
        model: target.model,
        receiver_verified: target.verified,
        configuration_revision: target.revision,
        segment_count: segments.len().try_into()?,
        character_count: segments
            .iter()
            .map(|s| s["text"].as_str().unwrap_or_default().chars().count())
            .sum::<usize>()
            .try_into()?,
        start_time: segments
            .iter()
            .filter_map(|s| s["start"].as_f64())
            .reduce(f64::min)
            .unwrap_or(0.),
        end_time: segments
            .iter()
            .filter_map(|s| s["end"].as_f64())
            .reduce(f64::max)
            .unwrap_or(0.),
        spec,
        payload_json,
    };
    tx.execute("INSERT INTO ai_send_approvals(id,project_id,base_version_id,preview_json,created_at) VALUES(?1,?2,?3,?4,?5)",
        params![preview.approval_id, preview.spec.project_id, preview.spec.expected_version_id, serde_json::to_string(&preview)?, now()])?;
    tx.commit()?;
    Ok(preview)
}

fn load(db: &Connection, id: &str) -> Result<AiSendPreview> {
    let raw: String = db
        .query_row(
            "SELECT preview_json FROM ai_send_approvals WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("ai_approval_not_found: 发送授权不存在，请重新预检"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn validate(db: &Connection, preview: &AiSendPreview) -> Result<()> {
    if project::current_version_id(db, &preview.spec.project_id)?.as_deref()
        != Some(&preview.spec.expected_version_id)
        || configuration(&preview.spec.target)?.revision != preview.configuration_revision
    {
        bail!("ai_approval_stale: 内容或执行配置已变化，请重新预检并确认")
    }
    Ok(())
}

fn consume(db: &mut Connection, id: &str) -> Result<(crate::model::AgentRun, bool)> {
    consume_with(db, id, |db, task, target| {
        agent_runner::enqueue_with_execution(db, task, None, target)
    })
}

fn consume_with(
    db: &mut Connection,
    id: &str,
    enqueue: impl FnOnce(&mut Connection, &str, ExecutionTarget) -> Result<crate::model::AgentRun>,
) -> Result<(crate::model::AgentRun, bool)> {
    let mut tx = WriteTransaction::begin(db)?;
    let preview = load(&tx, id)?;
    let existing: Option<String> = tx.query_row(
        "SELECT run_id FROM ai_send_approvals WHERE id=?1",
        [id],
        |r| r.get(0),
    )?;
    if let Some(run) = existing {
        return Ok((agent_runner::load(&tx, &run)?, false));
    }
    validate(&tx, &preview)?;
    if serde_json::to_string(&build_payload(&mut tx, &preview.spec)?)? != preview.payload_json {
        bail!("ai_approval_stale: 实际发送范围已变化，请重新预检并确认")
    }
    let task_id = if let Some(id) = preview.spec.task_id.clone() {
        let p = project::load(&tx, &preview.spec.project_id)?;
        let old = p
            .tasks
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow!("ai_approval_stale: 任务不存在"))?;
        if old.base_version_id == p.history.current_version_id {
            id
        } else {
            let fresh = tasks::create_for_workflow(
                &mut tx,
                &p.id,
                &preview.spec.kind,
                preview.spec.language.clone(),
                old.workflow_id.as_deref(),
                &preview.spec.instruction_locale,
            )?;
            tx.execute("UPDATE tasks SET status='cancelled',cancel_requested_at=?2,completed_at=?2 WHERE id=?1 AND status='queued'", params![id, now()])?;
            tx.execute("UPDATE auto_workflows SET agent_task_id=?2 WHERE agent_task_id=?1 AND status='awaiting_authorization'", params![id, fresh.id])?;
            tx.execute(
                "UPDATE workflows SET task_id=?2 WHERE task_id=?1",
                params![id, fresh.id],
            )?;
            fresh.id
        }
    } else {
        workflows::create_with_locale(
            &mut tx,
            &preview.spec.project_id,
            &preview.spec.kind,
            preview.spec.language.clone(),
            &preview.spec.instruction_locale,
        )?
        .task_id
    };
    let run = enqueue(&mut tx, &task_id, preview.spec.target)?;
    tx.execute(
        "UPDATE ai_send_approvals SET run_id=?2 WHERE id=?1 AND run_id IS NULL",
        params![id, run.id],
    )?;
    tx.execute("UPDATE auto_workflows SET status='needs_agent',ai_authorized=1,updated_at=?2 WHERE agent_task_id=?1 AND status='awaiting_authorization'", params![task_id, now()])?;
    tx.commit()?;
    Ok((run, true))
}

fn for_run(db: &Connection, run_id: &str) -> Result<Option<AiSendPreview>> {
    let id: Option<String> = db
        .query_row(
            "SELECT id FROM ai_send_approvals WHERE run_id=?1",
            [run_id],
            |r| r.get(0),
        )
        .optional()?;
    id.map(|id| load(db, &id)).transpose()
}

pub(crate) fn payload_for_run(
    db: &Connection,
    run: &crate::model::AgentRun,
    payload: &Value,
) -> Result<Value> {
    let Some(preview) = for_run(db, &run.id)? else {
        return Ok(payload.clone());
    };
    validate(db, &preview)?;
    if ExecutionTarget::from_run(run)? != preview.spec.target
        || serde_json::to_string(&api_executor::remote_payload(payload)?)? != preview.payload_json
    {
        bail!("ai_approval_stale: 执行内容与已确认载荷不一致")
    }
    Ok(serde_json::from_str(&preview.payload_json)?)
}

pub(crate) fn authorize_dispatch(
    db: &Connection,
    run: &crate::model::AgentRun,
    batch_id: &str,
) -> Result<()> {
    let Some(preview) = for_run(db, &run.id)? else {
        return Ok(());
    };
    validate(db, &preview)?;
    if db.execute("INSERT OR IGNORE INTO ai_approval_dispatches(approval_id,batch_id,dispatched_at) VALUES(?1,?2,?3)", params![preview.approval_id, batch_id, now()])? == 0 {
        bail!("ai_dispatch_uncertain: 此批次已发起过调用，不能自动重复发送；请核对运行结果后新建任务并重新授权")
    }
    Ok(())
}

pub(crate) fn validate_resume(db: &Connection, run_id: &str) -> Result<()> {
    let Some(preview) = for_run(db, run_id)? else {
        return Ok(());
    };
    validate(db, &preview)?;
    let uncertain: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM ai_approval_dispatches d
         LEFT JOIN agent_run_batches b ON b.id=d.batch_id
         WHERE d.approval_id=?1 AND (b.id IS NULL OR b.status!='completed' OR b.result_json IS NULL))",
        [&preview.approval_id], |r| r.get(0),
    )?;
    if uncertain {
        bail!("ai_dispatch_uncertain: 上次调用可能已消耗额度，请核对后新建任务并重新授权")
    }
    Ok(())
}

#[cfg(test)]
#[path = "ai_approval_tests.rs"]
mod tests;
