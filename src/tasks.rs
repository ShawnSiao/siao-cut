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

#[cfg(test)]
pub fn create(
    db: &mut Connection,
    project_id: &str,
    kind: &str,
    language: Option<String>,
) -> Result<Task> {
    create_with_locale(db, project_id, kind, language, "zh-CN")
}

pub fn create_with_locale(
    db: &mut Connection,
    project_id: &str,
    kind: &str,
    language: Option<String>,
    instruction_locale: &str,
) -> Result<Task> {
    create_for_workflow(db, project_id, kind, language, None, instruction_locale)
}

pub(crate) fn create_for_workflow(
    db: &mut Connection,
    project_id: &str,
    kind: &str,
    language: Option<String>,
    workflow_id: Option<&str>,
    instruction_locale: &str,
) -> Result<Task> {
    if ![
        "polish",
        "translate",
        "summary",
        "proofread",
        "edit",
        "cut",
        "punctuate",
        "speaker_names",
    ]
    .contains(&kind)
    {
        bail!("任务类型不受支持")
    }
    if kind == "translate" && language.is_none() {
        bail!("翻译任务需要 --lang")
    }
    if !["zh-CN", "en-US"].contains(&instruction_locale) {
        bail!("instruction_locale_invalid: --locale 必须为 zh-CN 或 en-US")
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
        last_activity: None,
        base_version_id: project.history.current_version_id.clone(),
        progress: 0.0,
        error_message: None,
        error_code: None,
        attempt_count: 0,
        cancel_requested_at: None,
        workflow_id: workflow_id.map(str::to_owned),
        instruction_locale: instruction_locale.to_owned(),
    };
    db.execute(
        "INSERT INTO tasks(id,project_id,kind,language,status,created_at,base_version_id,progress,attempt_count,workflow_id,instruction_locale) VALUES(?1,?2,?3,?4,?5,?6,?7,0,0,?8,?9)",
        params![&task.id, project_id, &task.kind, &task.language, &task.status, &task.created_at, &task.base_version_id, &task.workflow_id, &task.instruction_locale],
    )?;
    append_event(db, &task.id, project_id, "queued", Some(0.0), "任务已创建")?;
    Ok(task)
}

pub fn reconcile_expired(db: &mut Connection) -> Result<usize> {
    let expired = {
        let mut statement = db.prepare(
            "SELECT id,project_id,cancel_requested_at FROM tasks WHERE status IN ('claimed','running') AND lease_expires_at IS NOT NULL AND lease_expires_at < ?1",
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

pub fn claim(
    db: &mut Connection,
    worker: &str,
    requested_task_id: Option<&str>,
) -> Result<Option<(Project, Task, Value)>> {
    reconcile_expired(db)?;
    let candidate: Option<(String, String)> = db
        .query_row(
            "SELECT project_id,id FROM tasks WHERE status='queued' AND (?1 IS NULL OR id=?1) ORDER BY created_at LIMIT 1",
            [requested_task_id],
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
    let instructions = task_instructions(&task.kind, &task.instruction_locale);
    let include_words = !matches!(task.kind.as_str(), "punctuate" | "speaker_names");
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
            if include_words {
                json!({"id":segment.id,"text":segment.text,"start":segment.start,"end":segment.end,"confidence":segment.confidence,"words":words})
            } else {
                json!({"id":segment.id,"text":segment.text,"start":segment.start,"end":segment.end})
            }
        })
        .collect::<Vec<_>>();
    let response_schema = if task.kind == "speaker_names" {
        json!({
            "baseVersionId": "Original version ID returned with the task",
            "speakers": [{
                "speakerId": "Speaker ID from speakerEvidence",
                "before": "Current speaker label",
                "after": "Proposed display name",
                "reason": "Text evidence supporting the proposed name",
                "confidence": "0 to 1"
            }]
        })
    } else if task.instruction_locale == "en-US" {
        json!({
            "baseVersionId": "Original version ID returned with the task",
            "patches": [{
                "segmentId": "Subtitle segment ID",
                "before": "Original text from the task baseline",
                "after": "Proposed text; may be empty for cut tasks",
                "reason": "Verifiable reason for the change",
                "confidence": "0 to 1"
            }]
        })
    } else {
        json!({
            "baseVersionId": "任务返回的原始版本 ID",
            "patches": [{
                "segmentId": "字幕段 ID",
                "before": "任务基线中的原文",
                "after": "建议文本；cut 可为空",
                "reason": "可验证的修改原因",
                "confidence": "0 到 1"
            }]
        })
    };
    let speaker_evidence = if task.kind == "speaker_names" {
        let track = crate::speaker::load_track(db, &project_id)?;
        Some(json!({
            "speakers": track.speakers.iter().map(|speaker| json!({
                "id": speaker.id,
                "sourceLabel": speaker.source_label,
                "label": speaker.label
            })).collect::<Vec<_>>(),
            "associations": track.associations.iter().map(|association| json!({
                "segmentId": association.segment_id,
                "speakerId": association.speaker_id,
                "source": association.source
            })).collect::<Vec<_>>()
        }))
    } else {
        None
    };
    let payload = json!({
        "taskId": task.id,
        "projectId": project.id,
        "kind": task.kind,
        "language": task.language,
        "instructionLocale": task.instruction_locale,
        "contentLanguage": project.transcript.source_language,
        "baseVersionId": task.base_version_id,
        "instructions": instructions,
        "segments": segments,
        "speakerEvidence": speaker_evidence,
        "responseSchema": response_schema
    });
    Ok(Some((project, task, payload)))
}

fn task_instructions(kind: &str, instruction_locale: &str) -> &'static str {
    if instruction_locale == "en-US" {
        return match kind {
            "translate" => "Translate each segment while preserving meaning and terminology.",
            "polish" => {
                "Polish each segment and correct clear transcription errors without changing facts."
            }
            "proofread" => {
                "Proofread each segment and correct spelling, punctuation, and clear transcription errors."
            }
            "punctuate" => {
                "Add punctuation and sentence casing only. Preserve every spoken word and return reviewable segment patches."
            }
            "speaker_names" => {
                "Infer speaker display names only from self-introductions or explicit textual evidence. Return reviewable speaker suggestions and never guess."
            }
            "edit" => {
                "Identify repetition, tangents, and failed takes, then propose reviewable text changes."
            }
            "cut" => {
                "Identify complete semantic spans that could be removed. Never cut inside a word."
            }
            _ => "Provide a concise summary without inventing facts.",
        };
    }
    match kind {
        "translate" => "逐段翻译，保留原意与术语。",
        "polish" => "逐段润色，纠正明显转写错误，不删改事实。",
        "proofread" => "逐段校对，修正错别字、标点和明显转写错误。",
        "punctuate" => "只补充标点和必要的句首大小写，保留全部口述词，按字幕段返回待审建议。",
        "speaker_names" => {
            "只根据自我介绍或明确文本证据推断人物显示名；证据不足时不要猜测，结果必须等待人工审阅。"
        }
        "edit" => "识别重复、跑题和失败重录，给出可审阅的文本修改。",
        "cut" => "识别应删除的完整语义片段，禁止切入词中。",
        _ => "用中文给出短摘要，不捏造事实。",
    }
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
    if !["claimed", "running"].contains(&status.as_str()) || owner.as_deref() != Some(worker) {
        bail!("任务未由当前 Agent 领取")
    }
    let expires_at = (Utc::now() + Duration::minutes(LEASE_MINUTES)).to_rfc3339();
    db.execute(
        "UPDATE tasks SET status='running',progress=?2,lease_expires_at=?3 WHERE id=?1",
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
    if !["claimed", "running"].contains(&status.as_str()) || owner.as_deref() != Some(worker) {
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
        "claimed" | "running" => {
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
    if !["claimed", "running"].contains(&status.as_str()) || owner.as_deref() != Some(worker) {
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
    fn english_instruction_locale_is_persisted_and_localizes_claim_contract() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let task = create_with_locale(&mut db, &project.id, "proofread", None, "en-US").unwrap();
        assert_eq!(task.instruction_locale, "en-US");

        let claimed = claim(&mut db, "english-worker", None).unwrap().unwrap();
        assert_eq!(claimed.1.instruction_locale, "en-US");
        assert_eq!(claimed.2["instructionLocale"], "en-US");
        assert_eq!(claimed.2["contentLanguage"], "auto");
        assert!(
            claimed.2["instructions"]
                .as_str()
                .unwrap()
                .starts_with("Proofread each segment")
        );
        assert_eq!(
            claimed.2["responseSchema"]["patches"][0]["segmentId"],
            "Subtitle segment ID"
        );
    }

    #[test]
    fn agent_patch_submit_waits_for_review_before_updating_project() {
        let (_temp, mut db, project, segment_id) = fixture();
        let task = create(&mut db, &project.id, "translate", Some("en".into())).unwrap();
        let claim = claim(&mut db, "test-agent", None).unwrap().unwrap();
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
    fn speaker_name_agent_uses_text_structure_and_waits_for_review() {
        let (_temp, mut db, project, segment_id) = fixture();
        let timestamp = now();
        db.execute("INSERT INTO speaker_tracks(project_id,status,runtime_version,segmentation_model,embedding_model,generated_at,provider_id,model_id,source_kind) VALUES(?1,'ready','test','end-to-end','end-to-end',?2,'moss_openai','moss-test','end_to_end')", params![&project.id, &timestamp]).unwrap();
        db.execute("INSERT INTO speakers(id,project_id,source_label,label,color_index,created_at) VALUES('speaker-test',?1,'S00','Speaker 1',0,?2)", params![&project.id, &timestamp]).unwrap();
        db.execute("INSERT INTO segment_speakers(project_id,segment_id,speaker_id,source,confidence,updated_at) VALUES(?1,?2,'speaker-test','moss_end_to_end',NULL,?3)", params![&project.id, &segment_id, &timestamp]).unwrap();

        let task = create(&mut db, &project.id, "speaker_names", None).unwrap();
        let claimed = claim(&mut db, "name-agent", Some(&task.id))
            .unwrap()
            .unwrap();
        let serialized = claimed.2.to_string();
        assert!(!serialized.contains(&project.media.source_path));
        assert!(claimed.2["segments"][0].get("words").is_none());
        assert_eq!(
            claimed.2["speakerEvidence"]["associations"][0]["speakerId"],
            "speaker-test"
        );
        let base = claimed.2["baseVersionId"].as_str().unwrap();
        submit(
            &mut db,
            &task.id,
            "name-agent",
            json!({"baseVersionId":base,"speakers":[{"speakerId":"speaker-test","before":"Speaker 1","after":"李明","reason":"字幕中明确自我介绍","confidence":0.92}]}),
        )
        .unwrap();
        let before: String = db
            .query_row(
                "SELECT label FROM speakers WHERE id='speaker-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(before, "Speaker 1");
        patches::review_all(&mut db, &task.id, "apply").unwrap();
        let after: String = db
            .query_row(
                "SELECT label FROM speakers WHERE id='speaker-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(after, "李明");
    }

    #[test]
    fn version_conflict_stages_three_way_diff_without_overwriting_human_edit() {
        let (_temp, mut db, project, segment_id) = fixture();
        let task = create(&mut db, &project.id, "polish", None).unwrap();
        let claim = claim(&mut db, "test-agent", None).unwrap().unwrap();
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
        claim(&mut db, "test-agent", None).unwrap();
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

    #[test]
    fn targeted_claim_never_locks_another_queued_task() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let first = create(&mut db, &project.id, "summary", None).unwrap();
        let second = create(&mut db, &project.id, "proofread", None).unwrap();

        let claimed = claim(&mut db, "external-agent", Some(&second.id))
            .unwrap()
            .unwrap();
        assert_eq!(claimed.1.id, second.id);
        assert_eq!(
            find_task(&project::load(&db, &project.id).unwrap(), &first.id)
                .unwrap()
                .status,
            "queued"
        );

        assert!(
            claim(&mut db, "external-agent", Some("missing-task"))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            find_task(&project::load(&db, &project.id).unwrap(), &first.id)
                .unwrap()
                .status,
            "queued"
        );
    }

    #[test]
    fn heartbeat_marks_a_claimed_task_running_and_records_activity() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        claim(&mut db, "external-agent", Some(&task.id)).unwrap();

        let running = heartbeat(
            &mut db,
            &task.id,
            "external-agent",
            0.05,
            Some("开始处理任务"),
        )
        .unwrap();
        assert_eq!(running.status, "running");
        let loaded = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(loaded.last_activity.unwrap().kind, "progress");
    }
}
