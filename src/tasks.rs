use crate::{
    model::{AgentPatchSet, Lease, Project, Task, TaskEvent},
    patches, project,
    util::{new_id, now},
};
use anyhow::{Result, anyhow, bail};
use chrono::{Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

type SubmissionRow = (
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

const LEASE_MINUTES: i64 = 10;

pub fn create(
    db: &mut Connection,
    project_id: &str,
    kind: &str,
    language: Option<String>,
) -> Result<Task> {
    create_for_workflow(db, project_id, kind, language, None)
}

pub(crate) fn create_for_workflow(
    db: &mut Connection,
    project_id: &str,
    kind: &str,
    language: Option<String>,
    workflow_id: Option<&str>,
) -> Result<Task> {
    if !["polish", "translate", "summary", "proofread", "edit", "cut"].contains(&kind) {
        bail!("任务类型必须为 polish、translate、summary、proofread、edit 或 cut")
    }
    if kind == "translate" && language.is_none() {
        bail!("翻译任务需要 --lang")
    }
    let project = project::load(db, project_id)?;
    let task = Task {
        id: new_id("t"),
        kind: kind.to_owned(),
        language,
        status: "queued".to_owned(),
        created_at: now(),
        completed_at: None,
        lease: None,
        base_version_id: project.history.current_version_id.clone(),
        progress: 0.0,
        error_message: None,
        attempt_count: 0,
        cancel_requested_at: None,
        workflow_id: workflow_id.map(str::to_owned),
    };
    db.execute(
        "INSERT INTO tasks(id,project_id,kind,language,status,created_at,base_version_id,progress,attempt_count,workflow_id) VALUES(?1,?2,?3,?4,?5,?6,?7,0,0,?8)",
        params![&task.id, project_id, &task.kind, &task.language, &task.status, &task.created_at, &task.base_version_id, &task.workflow_id],
    )?;
    append_event(db, &task.id, project_id, "queued", Some(0.0), "任务已创建")?;
    Ok(task)
}

pub fn reconcile_expired(db: &mut Connection) -> Result<usize> {
    let expired = {
        let mut statement = db.prepare(
            "SELECT id,project_id,cancel_requested_at FROM tasks WHERE status='claimed' AND lease_expires_at IS NOT NULL AND lease_expires_at < ?1",
        )?;
        statement
            .query_map([now()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (task_id, project_id, cancel_requested_at) in &expired {
        let (status, kind, message) = if cancel_requested_at.is_some() {
            ("cancelled", "cancelled", "任务已按取消请求结束")
        } else {
            ("interrupted", "interrupted", "任务租约过期，可重新排队")
        };
        db.execute(
            "UPDATE tasks SET status=?2,lease_worker=NULL,lease_id=NULL,lease_expires_at=NULL WHERE id=?1",
            params![task_id, status],
        )?;
        append_event(db, task_id, project_id, kind, None, message)?;
    }
    Ok(expired.len())
}

pub fn claim(db: &mut Connection, worker: &str) -> Result<Option<(Project, Task, Value)>> {
    reconcile_expired(db)?;
    let candidate: Option<(String, String)> = db
        .query_row(
            "SELECT project_id,id FROM tasks WHERE status='queued' ORDER BY created_at LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((project_id, task_id)) = candidate else {
        return Ok(None);
    };
    let lease = Lease {
        worker: worker.to_owned(),
        id: new_id("lease"),
        expires_at: (Utc::now() + Duration::minutes(LEASE_MINUTES)).to_rfc3339(),
    };
    let changed = db.execute(
        "UPDATE tasks SET status='claimed',lease_worker=?3,lease_id=?4,lease_expires_at=?5,attempt_count=attempt_count+1,error_message=NULL WHERE id=?1 AND project_id=?2 AND status='queued'",
        params![&task_id, &project_id, &lease.worker, &lease.id, &lease.expires_at],
    )?;
    if changed == 0 {
        return Ok(None);
    }
    append_event(
        db,
        &task_id,
        &project_id,
        "claimed",
        None,
        "Agent 已领取任务",
    )?;
    db.execute(
        "UPDATE workflows SET status='running',updated_at=?2 WHERE task_id=?1",
        params![&task_id, now()],
    )?;
    let project = project::load(db, &project_id)?;
    let task = find_task(&project, &task_id)?;
    let instructions = match task.kind.as_str() {
        "translate" => "逐段翻译，保留原意与术语。",
        "polish" => "逐段润色，纠正明显转写错误，不删改事实。",
        "proofread" => "逐段校对，修正错别字、标点和明显转写错误。",
        "edit" => "识别重复、跑题和失败重录，给出可审阅的文本修改。",
        "cut" => "识别应删除的完整语义片段，禁止切入词中。",
        _ => "用中文给出短摘要，不捏造事实。",
    };
    let segments = project
        .transcript
        .segments
        .iter()
        .map(|segment| {
            let words = project
                .transcript
                .words
                .iter()
                .filter(|word| word.segment_id == segment.id)
                .map(|word| json!({"text":word.text,"start":word.start,"end":word.end,"confidence":word.confidence}))
                .collect::<Vec<_>>();
            json!({"id":segment.id,"text":segment.text,"start":segment.start,"end":segment.end,"confidence":segment.confidence,"words":words})
        })
        .collect::<Vec<_>>();
    let payload = json!({
        "taskId": task.id,
        "projectId": project.id,
        "kind": task.kind,
        "language": task.language,
        "baseVersionId": task.base_version_id,
        "instructions": instructions,
        "segments": segments,
        "responseSchema": {
            "baseVersionId": "任务返回的原始版本 ID",
            "patches": [{
                "segmentId": "字幕段 ID",
                "before": "任务基线中的原文",
                "after": "建议文本；cut 可为空",
                "reason": "可验证的修改原因",
                "confidence": "0 到 1"
            }]
        }
    });
    Ok(Some((project, task, payload)))
}

pub fn heartbeat(
    db: &mut Connection,
    task_id: &str,
    worker: &str,
    progress: f64,
    message: Option<&str>,
) -> Result<Task> {
    if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
        bail!("任务进度必须在 0 到 1 之间")
    }
    let (project_id, owner, status): (String, Option<String>, String) = db
        .query_row(
            "SELECT project_id,lease_worker,status FROM tasks WHERE id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    if status != "claimed" || owner.as_deref() != Some(worker) {
        bail!("任务未由当前 Agent 领取")
    }
    let expires_at = (Utc::now() + Duration::minutes(LEASE_MINUTES)).to_rfc3339();
    db.execute(
        "UPDATE tasks SET progress=?2,lease_expires_at=?3 WHERE id=?1",
        params![task_id, progress, expires_at],
    )?;
    append_event(
        db,
        task_id,
        &project_id,
        "progress",
        Some(progress),
        message.unwrap_or("任务继续处理"),
    )?;
    find_task(&project::load(db, &project_id)?, task_id)
}

pub fn fail(db: &mut Connection, task_id: &str, worker: &str, message: &str) -> Result<Task> {
    if message.trim().is_empty() {
        bail!("失败原因不能为空")
    }
    let (project_id, owner, status): (String, Option<String>, String) = db
        .query_row(
            "SELECT project_id,lease_worker,status FROM tasks WHERE id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    if status != "claimed" || owner.as_deref() != Some(worker) {
        bail!("任务未由当前 Agent 领取")
    }
    db.execute(
        "UPDATE tasks SET status='failed',error_message=?2,lease_worker=NULL,lease_id=NULL,lease_expires_at=NULL WHERE id=?1",
        params![task_id, message],
    )?;
    append_event(db, task_id, &project_id, "failed", None, message)?;
    find_task(&project::load(db, &project_id)?, task_id)
}

pub fn retry(db: &mut Connection, task_id: &str) -> Result<Task> {
    let project_id: String = db
        .query_row(
            "SELECT project_id FROM tasks WHERE id=?1 AND status IN ('failed','interrupted')",
            [task_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("只有失败或中断的任务可以重试"))?;
    let latest_version = project::current_version_id(db, &project_id)?;
    db.execute(
        "UPDATE tasks SET status='queued',progress=0,error_message=NULL,cancel_requested_at=NULL,base_version_id=?2 WHERE id=?1",
        params![task_id, latest_version],
    )?;
    append_event(
        db,
        task_id,
        &project_id,
        "queued",
        Some(0.0),
        "任务已重新排队",
    )?;
    find_task(&project::load(db, &project_id)?, task_id)
}

pub fn cancel(db: &mut Connection, task_id: &str) -> Result<Task> {
    let (project_id, status): (String, String) = db
        .query_row(
            "SELECT project_id,status FROM tasks WHERE id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    match status.as_str() {
        "queued" | "failed" | "interrupted" => {
            db.execute(
                "UPDATE tasks SET status='cancelled',cancel_requested_at=?2 WHERE id=?1",
                params![task_id, now()],
            )?;
            append_event(db, task_id, &project_id, "cancelled", None, "任务已取消")?;
        }
        "claimed" => {
            db.execute(
                "UPDATE tasks SET cancel_requested_at=?2 WHERE id=?1",
                params![task_id, now()],
            )?;
            append_event(
                db,
                task_id,
                &project_id,
                "cancel_requested",
                None,
                "已请求取消，等待 Agent 停止",
            )?;
        }
        _ => bail!("当前任务状态不能取消：{status}"),
    }
    find_task(&project::load(db, &project_id)?, task_id)
}

pub fn events(db: &Connection, task_id: &str, after: i64) -> Result<Vec<TaskEvent>> {
    Ok(db
        .prepare("SELECT id,task_id,project_id,kind,progress,message,created_at FROM task_events WHERE task_id=?1 AND id>?2 ORDER BY id")?
        .query_map(params![task_id, after], |row| {
            Ok(TaskEvent {
                id: row.get(0)?,
                task_id: row.get(1)?,
                project_id: row.get(2)?,
                kind: row.get(3)?,
                progress: row.get(4)?,
                message: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn project_id(db: &Connection, task_id: &str) -> Result<String> {
    db.query_row(
        "SELECT project_id FROM tasks WHERE id=?1",
        [task_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| anyhow!("任务不存在：{task_id}"))
}

pub fn submit(
    db: &mut Connection,
    task_id: &str,
    worker: &str,
    response: Value,
) -> Result<(String, Task, AgentPatchSet)> {
    let row: Option<SubmissionRow> = db
        .query_row(
            "SELECT project_id,kind,language,status,lease_expires_at,lease_worker,base_version_id,cancel_requested_at FROM tasks WHERE id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
        )
        .optional()?;
    let Some((
        project_id,
        kind,
        language,
        status,
        expires_at,
        owner,
        base_version_id,
        cancel_requested_at,
    )) = row
    else {
        bail!("任务不存在：{task_id}")
    };
    if status != "claimed" || owner.as_deref() != Some(worker) {
        bail!("任务未由当前 Agent 领取")
    }
    if cancel_requested_at.is_some() {
        bail!("task_cancel_requested: 任务已请求取消，不能提交结果")
    }
    if expires_at
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc) < Utc::now())
        .unwrap_or(true)
    {
        bail!("任务租约已过期，请重新领取")
    }
    let response_base = response
        .get("baseVersionId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("任务响应缺少 baseVersionId"))?;
    if base_version_id.as_deref() != Some(response_base) {
        bail!("task_base_version_mismatch: Agent 响应版本与任务基线不一致")
    }
    let patch_set = patches::stage(
        db,
        task_id,
        &project_id,
        &kind,
        language.as_deref(),
        response_base,
        &response,
    )?;
    crate::auto_workflow::agent_result_ready(db, task_id)?;
    append_event(
        db,
        task_id,
        &project_id,
        "review",
        Some(1.0),
        "Agent 结果已提交，等待人工审阅",
    )?;
    Ok((
        project_id.clone(),
        find_task(&project::load(db, &project_id)?, task_id)?,
        patch_set,
    ))
}

fn find_task(project: &Project, task_id: &str) -> Result<Task> {
    project
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))
}

fn append_event(
    db: &Connection,
    task_id: &str,
    project_id: &str,
    kind: &str,
    progress: Option<f64>,
    message: &str,
) -> Result<()> {
    db.execute(
        "INSERT INTO task_events(task_id,project_id,kind,progress,message,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![task_id, project_id, kind, progress, message, now()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, project};
    use std::fs;
    use tempfile::tempdir;

    fn fixture() -> (tempfile::TempDir, Connection, Project, String) {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("core.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut db, &media, Some("test".into())).unwrap();
        let segment =
            project::add_segment(&mut db, &project.id, 0.0, 1.0, "你好".into(), None).unwrap();
        (temp, db, project, segment.id)
    }

    #[test]
    fn agent_patch_submit_waits_for_review_before_updating_project() {
        let (_temp, mut db, project, segment_id) = fixture();
        let task = create(&mut db, &project.id, "translate", Some("en".into())).unwrap();
        let claim = claim(&mut db, "test-agent").unwrap().unwrap();
        let base = claim.2["baseVersionId"].as_str().unwrap();
        submit(
            &mut db,
            &task.id,
            "test-agent",
            json!({"baseVersionId":base,"patches":[{"segmentId":segment_id,"before":"你好","after":"Hello","reason":"翻译为英语","confidence":0.98}]}),
        )
        .unwrap();
        let staged = project::load(&db, &project.id).unwrap();
        assert!(!staged.translations.contains_key("en"));
        assert_eq!(staged.tasks.last().unwrap().status, "review");
        assert_eq!(staged.patch_sets[0].items[0].status, "pending");
        patches::review_all(&mut db, &task.id, "apply").unwrap();
        let updated = project::load(&db, &project.id).unwrap();
        assert_eq!(updated.translations["en"].segments[0].text, "Hello");
        assert_eq!(updated.tasks.last().unwrap().status, "done");
        assert_eq!(
            events(&db, &task.id, 0).unwrap().last().unwrap().kind,
            "completed"
        );
    }

    #[test]
    fn version_conflict_stages_three_way_diff_without_overwriting_human_edit() {
        let (_temp, mut db, project, segment_id) = fixture();
        let task = create(&mut db, &project.id, "polish", None).unwrap();
        let claim = claim(&mut db, "test-agent").unwrap().unwrap();
        let base = claim.2["baseVersionId"].as_str().unwrap();
        project::edit_segment(&mut db, &project.id, &segment_id, "人工修改".into()).unwrap();
        let submitted = submit(
            &mut db,
            &task.id,
            "test-agent",
            json!({"baseVersionId":base,"patches":[{"segmentId":segment_id,"before":"你好","after":"Agent 修改","reason":"修正表达","confidence":0.8}]}),
        )
        .unwrap();
        assert_eq!(submitted.2.items[0].status, "conflict");
        assert_eq!(submitted.2.items[0].current_text, "人工修改");
        assert_eq!(
            project::load(&db, &project.id).unwrap().transcript.segments[0].text,
            "人工修改"
        );
        patches::review_all(&mut db, &task.id, "keep").unwrap();
        assert_eq!(
            project::load(&db, &project.id).unwrap().transcript.segments[0].text,
            "人工修改"
        );
    }

    #[test]
    fn task_recovery_marks_expired_lease_interrupted_and_retries() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        claim(&mut db, "test-agent").unwrap();
        db.execute(
            "UPDATE tasks SET lease_expires_at='2000-01-01T00:00:00+00:00' WHERE id=?1",
            [&task.id],
        )
        .unwrap();
        assert_eq!(reconcile_expired(&mut db).unwrap(), 1);
        let interrupted = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(interrupted.status, "interrupted");
        assert_eq!(retry(&mut db, &task.id).unwrap().status, "queued");
    }
}
