use crate::{
    model::{AgentPatchSet, Lease, Project, Task, TaskEvent},
    patches, project, translation,
    util::{new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{Duration, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

type SubmissionRow = (
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

const LEASE_MINUTES: i64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimPayloadFile {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub newly_claimed: bool,
}

struct ClaimOutcome {
    project: Project,
    task: Task,
    payload: Value,
    payload_file: Option<ClaimPayloadFile>,
}

#[derive(Debug, Clone)]
struct ClaimSnapshot {
    project_id: String,
    task_id: String,
    status: String,
    stored_payload: Option<String>,
    lease_worker: Option<String>,
    lease_id: Option<String>,
    lease_expires_at: Option<String>,
    attempt_count: i64,
}

struct PreparedClaimFile {
    output: PathBuf,
    staging: Option<PathBuf>,
    sha256: String,
    bytes: u64,
    newly_claimed: bool,
}

struct PublishedClaimFile {
    metadata: ClaimPayloadFile,
    output: PathBuf,
    rollback_path: Option<PathBuf>,
    finalized: bool,
}

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_CLAIM_COMMIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_NEXT_CLAIM_ROLLBACK_CLEANUP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static CLAIM_ROLLBACK_CLEANUP_ATTEMPTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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
    let segment_ids = if kind == "translate" {
        let language = language.as_deref().expect("translation language checked");
        let selected = translation::target_segment_ids(&project, language);
        if selected.is_empty() {
            bail!("translation_up_to_date: 当前译文没有过期段或质量失败段")
        }
        selected
    } else {
        project
            .transcript
            .segments
            .iter()
            .map(|segment| segment.id.clone())
            .collect()
    };
    let glossary_version = (kind == "translate").then_some(project.glossary.version);
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
    let tx = db.transaction()?;
    tx.execute(
        "INSERT INTO tasks(id,project_id,kind,language,status,created_at,base_version_id,progress,attempt_count,workflow_id,instruction_locale,glossary_version) VALUES(?1,?2,?3,?4,?5,?6,?7,0,0,?8,?9,?10)",
        params![&task.id, project_id, &task.kind, &task.language, &task.status, &task.created_at, &task.base_version_id, &task.workflow_id, &task.instruction_locale, glossary_version.map(i64::from)],
    )?;
    store_task_segments(&tx, &project, &task.id, &segment_ids)?;
    append_event(&tx, &task.id, project_id, "queued", Some(0.0), "任务已创建")?;
    tx.commit()?;
    Ok(task)
}

pub fn reconcile_expired(db: &mut Connection) -> Result<usize> {
    let cutoff = now();
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let expired = {
        let mut statement = tx.prepare(
            "SELECT id,project_id,cancel_requested_at,lease_expires_at
             FROM tasks
             WHERE status IN ('claimed','running')
               AND lease_expires_at IS NOT NULL
               AND lease_expires_at < ?1",
        )?;
        statement
            .query_map([&cutoff], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut reconciled = 0;
    for (task_id, project_id, cancel_requested_at, lease_expires_at) in &expired {
        let (status, kind, message) = if cancel_requested_at.is_some() {
            ("cancelled", "cancelled", "任务已按取消请求结束")
        } else {
            ("interrupted", "interrupted", "任务租约过期，可重新排队")
        };
        let changed = tx.execute(
            "UPDATE tasks
             SET status=?2,lease_worker=NULL,lease_id=NULL,lease_expires_at=NULL,
                 claim_payload_json=NULL
             WHERE id=?1
               AND status IN ('claimed','running')
               AND lease_expires_at=?3
               AND lease_expires_at < ?4",
            params![task_id, status, lease_expires_at, &cutoff],
        )?;
        if changed == 0 {
            continue;
        }
        set_workflow_status(&tx, task_id, status)?;
        append_event(&tx, task_id, project_id, kind, None, message)?;
        reconciled += 1;
    }
    tx.commit()?;
    Ok(reconciled)
}

pub fn claim(
    db: &mut Connection,
    worker: &str,
    requested_task_id: Option<&str>,
) -> Result<Option<(Project, Task, Value)>> {
    claim_with_lease(db, worker, requested_task_id, None)
}

pub fn claim_with_lease(
    db: &mut Connection,
    worker: &str,
    requested_task_id: Option<&str>,
    reissue_lease_id: Option<&str>,
) -> Result<Option<(Project, Task, Value)>> {
    Ok(
        claim_internal(db, worker, requested_task_id, reissue_lease_id, None)?
            .map(|outcome| (outcome.project, outcome.task, outcome.payload)),
    )
}

pub fn claim_to_file(
    db: &mut Connection,
    worker: &str,
    requested_task_id: Option<&str>,
    reissue_lease_id: Option<&str>,
    payload_output: &Path,
) -> Result<Option<(Project, Task, ClaimPayloadFile)>> {
    Ok(claim_internal(
        db,
        worker,
        requested_task_id,
        reissue_lease_id,
        Some(payload_output),
    )?
    .map(|outcome| {
        (
            outcome.project,
            outcome.task,
            outcome
                .payload_file
                .expect("claim payload file exists when an output path was supplied"),
        )
    }))
}

fn claim_internal(
    db: &mut Connection,
    worker: &str,
    requested_task_id: Option<&str>,
    reissue_lease_id: Option<&str>,
    payload_output: Option<&Path>,
) -> Result<Option<ClaimOutcome>> {
    reconcile_expired(db)?;
    let initial_cutoff = now();
    let Some(snapshot) = select_claim_snapshot(
        db,
        worker,
        requested_task_id,
        reissue_lease_id,
        &initial_cutoff,
    )?
    else {
        return Ok(None);
    };
    let initial_project = project::load(db, &snapshot.project_id)?;
    let initial_task = find_task(&initial_project, &snapshot.task_id)?;
    let newly_claimed = snapshot.status == "queued";
    let lease = if newly_claimed {
        Lease {
            worker: worker.to_owned(),
            id: new_id("lease"),
            expires_at: (Utc::now() + Duration::minutes(LEASE_MINUTES)).to_rfc3339(),
        }
    } else {
        Lease {
            worker: snapshot
                .lease_worker
                .clone()
                .ok_or_else(|| anyhow!("task_lease_mismatch: 当前任务租约无效"))?,
            id: snapshot
                .lease_id
                .clone()
                .ok_or_else(|| anyhow!("task_lease_mismatch: 当前任务租约无效"))?,
            expires_at: snapshot
                .lease_expires_at
                .clone()
                .ok_or_else(|| anyhow!("task_lease_mismatch: 当前任务租约无效"))?,
        }
    };
    let initial_version_id = initial_project.history.current_version_id.clone();
    let initial_glossary_version = initial_project.glossary.version;
    let attempt_count = if newly_claimed {
        snapshot.attempt_count + 1
    } else {
        snapshot.attempt_count
    };
    let payload = claim_payload_for_snapshot(db, &initial_project, &initial_task, &snapshot)?;
    let payload = attach_claim_lease(payload, &lease.id, attempt_count)?;
    let payload_json = serde_json::to_string(&payload)?;
    let mut prepared_file = payload_output
        .map(|path| prepare_claim_payload(path, &payload, newly_claimed))
        .transpose()?;
    let claim_result = (|| -> Result<Option<ClaimOutcome>> {
        let cutoff = now();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(current) = select_claim_snapshot(
            &tx,
            worker,
            Some(&snapshot.task_id),
            reissue_lease_id,
            &cutoff,
        )?
        else {
            return Ok(None);
        };
        if current.status != snapshot.status
            || current.project_id != snapshot.project_id
            || current.attempt_count != snapshot.attempt_count
            || current.lease_id != snapshot.lease_id
            || current.stored_payload != snapshot.stored_payload
        {
            bail!("task_lease_mismatch: 任务领取状态已变化，请重新读取当前任务")
        }
        if project::current_version_id(&tx, &current.project_id)?.as_deref()
            != initial_version_id.as_deref()
        {
            bail!("task_payload_unavailable: 准备负载期间任务文本发生变化")
        }
        if initial_task.kind == "translate" {
            let current_glossary_version: i64 = tx.query_row(
                "SELECT current_version FROM project_glossaries WHERE project_id=?1",
                [&current.project_id],
                |row| row.get(0),
            )?;
            if current_glossary_version != i64::from(initial_glossary_version) {
                bail!("glossary_version_conflict: 准备负载期间术语表发生变化")
            }
        }
        let current_payload = payload.clone();
        if newly_claimed {
            let changed = tx.execute(
                "UPDATE tasks
                 SET status='claimed',lease_worker=?3,lease_id=?4,lease_expires_at=?5,
                     attempt_count=attempt_count+1,error_message=NULL,claim_payload_json=?6
                 WHERE id=?1 AND project_id=?2 AND status='queued'
                   AND attempt_count=?7",
                params![
                    &current.task_id,
                    &current.project_id,
                    &lease.worker,
                    &lease.id,
                    &lease.expires_at,
                    &payload_json,
                    current.attempt_count
                ],
            )?;
            if changed == 0 {
                bail!("task_lease_mismatch: 任务已被其他 Agent 领取")
            }
            append_event(
                &tx,
                &current.task_id,
                &current.project_id,
                "claimed",
                None,
                "Agent 已领取任务",
            )?;
            set_workflow_status(&tx, &current.task_id, "running")?;
        } else if current.stored_payload.as_deref() != Some(payload_json.as_str()) {
            let changed = tx.execute(
                "UPDATE tasks SET claim_payload_json=?5
                 WHERE id=?1 AND status IN ('claimed','running')
                   AND lease_worker=?2 AND lease_id=?3 AND lease_expires_at>=?4",
                params![&current.task_id, worker, &lease.id, &cutoff, &payload_json],
            )?;
            if changed == 0 {
                bail!("task_lease_mismatch: 当前租约已失效，不能重取负载")
            }
        }
        let mut published_file = prepared_file
            .as_mut()
            .map(PreparedClaimFile::publish)
            .transpose()?;
        if let Err(commit_error) = commit_claim_transaction(tx) {
            if let Some(published) = published_file.as_mut()
                && let Err(rollback_error) = published.rollback()
            {
                return Err(commit_error.context(format!(
                    "任务领取事务失败，且恢复原负载文件失败：{rollback_error}"
                )));
            }
            return Err(commit_error);
        }
        let payload_file = published_file
            .as_mut()
            .map(PublishedClaimFile::finalize_after_commit);
        let claimed_project = project::load(db, &current.project_id)?;
        let claimed_task = find_task(&claimed_project, &current.task_id)?;
        Ok(Some(ClaimOutcome {
            project: claimed_project,
            task: claimed_task,
            payload: current_payload,
            payload_file,
        }))
    })();
    match claim_result {
        Err(error) => {
            if let Some(prepared) = prepared_file.as_mut()
                && let Err(cleanup_error) = prepared.cleanup()
            {
                return Err(error.context(format!("清理任务负载暂存文件失败：{cleanup_error}")));
            }
            Err(error)
        }
        outcome => outcome,
    }
}

fn select_claim_snapshot(
    db: &Connection,
    worker: &str,
    requested_task_id: Option<&str>,
    reissue_lease_id: Option<&str>,
    cutoff: &str,
) -> Result<Option<ClaimSnapshot>> {
    let snapshot = if let Some(task_id) = requested_task_id {
        db.query_row(
            "SELECT project_id,id,status,claim_payload_json,lease_worker,lease_id,
                    lease_expires_at,attempt_count
             FROM tasks WHERE id=?1",
            [task_id],
            |row| {
                Ok(ClaimSnapshot {
                    project_id: row.get(0)?,
                    task_id: row.get(1)?,
                    status: row.get(2)?,
                    stored_payload: row.get(3)?,
                    lease_worker: row.get(4)?,
                    lease_id: row.get(5)?,
                    lease_expires_at: row.get(6)?,
                    attempt_count: row.get(7)?,
                })
            },
        )
        .optional()?
    } else {
        db.query_row(
            "SELECT project_id,id,status,claim_payload_json,lease_worker,lease_id,
                    lease_expires_at,attempt_count
             FROM tasks WHERE status='queued' ORDER BY created_at LIMIT 1",
            [],
            |row| {
                Ok(ClaimSnapshot {
                    project_id: row.get(0)?,
                    task_id: row.get(1)?,
                    status: row.get(2)?,
                    stored_payload: row.get(3)?,
                    lease_worker: row.get(4)?,
                    lease_id: row.get(5)?,
                    lease_expires_at: row.get(6)?,
                    attempt_count: row.get(7)?,
                })
            },
        )
        .optional()?
    };
    let Some(snapshot) = snapshot else {
        return Ok(None);
    };
    match snapshot.status.as_str() {
        "queued" => {
            if reissue_lease_id.is_some() {
                bail!("task_lease_mismatch: 旧租约不能领取新的任务尝试")
            }
            Ok(Some(snapshot))
        }
        "claimed" | "running" => {
            if snapshot.lease_worker.as_deref() != Some(worker)
                || snapshot.lease_id.as_deref() != reissue_lease_id
                || snapshot
                    .lease_expires_at
                    .as_deref()
                    .is_none_or(|value| value < cutoff)
            {
                bail!("task_lease_mismatch: 当前租约已失效，不能重取负载")
            }
            Ok(Some(snapshot))
        }
        _ => Ok(None),
    }
}

fn claim_payload_for_snapshot(
    db: &Connection,
    project: &Project,
    task: &Task,
    snapshot: &ClaimSnapshot,
) -> Result<Value> {
    match snapshot.stored_payload.as_deref() {
        Some(serialized) if snapshot.status != "queued" => serde_json::from_str(serialized)
            .context("task_payload_unavailable: 无法读取已领取任务的文本负载"),
        _ => build_claim_payload(db, project, task),
    }
}

fn attach_claim_lease(mut payload: Value, lease_id: &str, attempt_count: i64) -> Result<Value> {
    let object = payload
        .as_object_mut()
        .ok_or_else(|| anyhow!("task_payload_unavailable: 任务负载不是 JSON 对象"))?;
    object.insert("leaseId".into(), Value::String(lease_id.to_owned()));
    object.insert("attemptCount".into(), Value::from(attempt_count));
    Ok(payload)
}

fn build_claim_payload(db: &Connection, project: &Project, task: &Task) -> Result<Value> {
    if project.history.current_version_id != task.base_version_id {
        bail!("task_base_version_conflict: 项目版本已变化，请重新创建或重试任务")
    }
    let task_segment_ids = translation::task_segment_ids(db, &task.id)?;
    let task_segment_set = task_segment_ids.iter().collect::<BTreeSet<_>>();
    let task_glossary_version: Option<i64> = db.query_row(
        "SELECT glossary_version FROM tasks WHERE id=?1",
        [&task.id],
        |row| row.get(0),
    )?;
    if task.kind == "translate"
        && task_glossary_version != Some(i64::from(project.glossary.version))
    {
        bail!("glossary_version_conflict: 术语表版本已变化，请重新创建翻译任务")
    }
    let instructions = task_instructions(&task.kind, &task.instruction_locale);
    let include_words = !matches!(task.kind.as_str(), "punctuate" | "speaker_names");
    let mut words_by_segment = BTreeMap::<&str, Vec<Value>>::new();
    if include_words {
        for word in &project.transcript.words {
            if task_segment_set.contains(&word.segment_id) {
                words_by_segment
                    .entry(word.segment_id.as_str())
                    .or_default()
                    .push(json!({
                        "text":word.text,
                        "start":word.start,
                        "end":word.end,
                        "confidence":word.confidence
                    }));
            }
        }
    }
    let translations_by_segment = task
        .language
        .as_deref()
        .and_then(|language| project.translations.get(language))
        .map(|translation| {
            translation
                .segments
                .iter()
                .map(|segment| (segment.segment_id.as_str(), segment))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let segments = project
        .transcript
        .segments
        .iter()
        .filter(|segment| task_segment_set.contains(&segment.id))
        .map(|segment| {
            let words = words_by_segment
                .remove(segment.id.as_str())
                .unwrap_or_default();
            let current_translation = translations_by_segment.get(segment.id.as_str()).copied();
            if include_words {
                json!({"id":segment.id,"text":segment.text,"sourceHash":translation::source_hash(&segment.text),"currentTranslation":current_translation.map(|item| item.text.as_str()),"translationStatus":current_translation.map(|item| item.status.as_str()),"start":segment.start,"end":segment.end,"confidence":segment.confidence,"words":words})
            } else {
                json!({"id":segment.id,"text":segment.text,"sourceHash":translation::source_hash(&segment.text),"start":segment.start,"end":segment.end})
            }
        })
        .collect::<Vec<_>>();
    if segments.len() != task_segment_set.len() {
        bail!("agent_batch_incomplete: 任务基线中的字幕段已变化，请重新创建任务")
    }
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
        let track = crate::speaker::load_track(db, &project.id)?;
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
    let translation_context = if task.kind == "translate" {
        let language = task
            .language
            .as_deref()
            .ok_or_else(|| anyhow!("翻译任务缺少目标语言"))?;
        Some(json!({
            "targetLanguage": language,
            "glossaryVersion": project.glossary.version,
            "glossary": translation::glossary_entries_for_language(&project.glossary, language),
            "subtitleConstraints": {
                "maxLineCharacters": 42,
                "maxLines": if language.to_ascii_lowercase().starts_with("en") { 2 } else { 0 },
                "maxCharactersPerSecond": 20
            },
            "segmentIds": task_segment_ids
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
        "translationContext": translation_context,
        "speakerEvidence": speaker_evidence,
        "responseSchema": response_schema
    });
    Ok(payload)
}

fn prepare_claim_payload(
    output: &Path,
    payload: &Value,
    newly_claimed: bool,
) -> Result<PreparedClaimFile> {
    if !output.is_absolute() {
        bail!("task_payload_output_invalid: --payload-output 必须为绝对路径")
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| anyhow!("task_payload_output_invalid: 无法确定负载输出目录"))?;
    fs::create_dir_all(parent).context("task_payload_output_failed: 无法创建任务负载输出目录")?;
    let parent = parent
        .canonicalize()
        .context("task_payload_output_failed: 无法确认任务负载输出目录")?;
    let file_name = output
        .file_name()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("task_payload_output_invalid: 任务负载文件名无效"))?;
    let output = parent.join(file_name);
    if output.exists() && !output.is_file() {
        bail!("task_payload_output_failed: 任务负载输出路径不是普通文件")
    }
    let bytes = serde_json::to_vec_pretty(payload)?;
    let staging = parent.join(format!(".siaocut-claim-{}.partial", new_id("payload")));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staging)
        .context("task_payload_output_failed: 无法创建任务负载暂存文件")?;
    let write_result = (|| -> Result<()> {
        file.write_all(&bytes)
            .context("task_payload_output_failed: 无法写入完整任务负载")?;
        file.sync_all()
            .context("task_payload_output_failed: 无法将完整任务负载同步到磁盘")?;
        Ok(())
    })();
    if let Err(error) = write_result {
        return match fs::remove_file(&staging) {
            Ok(()) => Err(error),
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
                Err(error)
            }
            Err(cleanup_error) => {
                Err(error.context(format!("清理任务负载暂存文件失败：{cleanup_error}")))
            }
        };
    }
    Ok(PreparedClaimFile {
        output,
        staging: Some(staging),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        bytes: bytes.len() as u64,
        newly_claimed,
    })
}

impl PreparedClaimFile {
    fn publish(&mut self) -> Result<PublishedClaimFile> {
        let staging = self
            .staging
            .as_deref()
            .ok_or_else(|| anyhow!("task_payload_output_failed: 任务负载暂存文件已发布"))?;
        let rollback_path = self.output.is_file().then(|| {
            self.output
                .parent()
                .expect("prepared payload output has a parent")
                .join(format!(".siaocut-claim-{}.rollback", new_id("payload")))
        });
        recoverable_atomic_publish(staging, &self.output, rollback_path.as_deref())
            .context("task_payload_output_failed: 无法原子发布完整任务负载文件")?;
        self.staging = None;
        Ok(PublishedClaimFile {
            metadata: ClaimPayloadFile {
                path: self.output.clone(),
                sha256: self.sha256.clone(),
                bytes: self.bytes,
                newly_claimed: self.newly_claimed,
            },
            output: self.output.clone(),
            rollback_path,
            finalized: false,
        })
    }

    fn cleanup(&mut self) -> Result<()> {
        let Some(staging) = self.staging.take() else {
            return Ok(());
        };
        match fs::remove_file(&staging) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(anyhow::Error::new(error).context(format!(
                "task_payload_output_failed: 无法清理暂存文件 {}",
                staging.display()
            ))),
        }
    }
}

impl PublishedClaimFile {
    fn rollback(&mut self) -> Result<()> {
        if self.finalized {
            return Ok(());
        }
        if let Some(rollback_path) = self.rollback_path.as_deref() {
            atomic_publish(rollback_path, &self.output)
                .context("task_payload_output_failed: 领取事务回滚后无法恢复原任务负载文件")?;
        } else {
            match fs::remove_file(&self.output) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(anyhow::Error::new(error)
                        .context("task_payload_output_failed: 领取事务回滚后无法删除新负载文件"));
                }
            }
        }
        self.rollback_path = None;
        self.finalized = true;
        Ok(())
    }

    fn finalize_after_commit(&mut self) -> ClaimPayloadFile {
        self.finalized = true;
        let _ = self.cleanup_rollback_file();
        self.metadata.clone()
    }

    fn cleanup_rollback_file(&mut self) -> Result<()> {
        let Some(rollback_path) = self.rollback_path.as_deref() else {
            return Ok(());
        };
        match remove_claim_rollback_file(rollback_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(anyhow::Error::new(error).context(format!(
                    "task_payload_output_failed: 无法清理负载回滚文件 {}",
                    rollback_path.display()
                )));
            }
        }
        self.rollback_path = None;
        Ok(())
    }
}

impl Drop for PublishedClaimFile {
    fn drop(&mut self) {
        if self.finalized {
            let _ = self.cleanup_rollback_file();
        } else {
            let _ = self.rollback();
        }
    }
}

impl Drop for PreparedClaimFile {
    fn drop(&mut self) {
        if let Some(staging) = self.staging.take() {
            let _ = fs::remove_file(staging);
        }
    }
}

#[cfg(windows)]
fn recoverable_atomic_publish(
    staging: &Path,
    output: &Path,
    rollback_path: Option<&Path>,
) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACEFILE_WRITE_THROUGH,
        ReplaceFileW,
    };

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let staging_wide = wide(staging);
    let output_wide = wide(output);
    let rollback_wide = rollback_path.map(wide);
    let succeeded = unsafe {
        if output.is_file() {
            ReplaceFileW(
                output_wide.as_ptr(),
                staging_wide.as_ptr(),
                rollback_wide
                    .as_ref()
                    .map_or(std::ptr::null(), |path| path.as_ptr()),
                REPLACEFILE_WRITE_THROUGH,
                std::ptr::null(),
                std::ptr::null(),
            )
        } else {
            MoveFileExW(
                staging_wide.as_ptr(),
                output_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if succeeded == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn recoverable_atomic_publish(
    staging: &Path,
    output: &Path,
    rollback_path: Option<&Path>,
) -> Result<()> {
    if let Some(rollback_path) = rollback_path {
        fs::hard_link(output, rollback_path).context("无法为已有任务负载创建同目录回滚链接")?;
        if let Err(publish_error) = fs::rename(staging, output) {
            return match fs::remove_file(rollback_path) {
                Ok(()) => Err(publish_error.into()),
                Err(cleanup_error) => Err(anyhow::Error::new(publish_error)
                    .context(format!("发布失败，且无法清理回滚链接：{cleanup_error}"))),
            };
        }
    } else {
        fs::rename(staging, output)?;
    }
    Ok(())
}

fn atomic_publish(staging: &Path, output: &Path) -> Result<()> {
    recoverable_atomic_publish(staging, output, None)
}

fn remove_claim_rollback_file(path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    {
        CLAIM_ROLLBACK_CLEANUP_ATTEMPTS.with(|attempts| attempts.set(attempts.get() + 1));
        if FAIL_NEXT_CLAIM_ROLLBACK_CLEANUP.with(|flag| flag.replace(false)) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected claim rollback cleanup failure",
            ));
        }
    }
    fs::remove_file(path)
}

fn commit_claim_transaction(tx: Transaction<'_>) -> Result<()> {
    #[cfg(test)]
    if FAIL_NEXT_CLAIM_COMMIT.with(|flag| flag.replace(false)) {
        tx.rollback()?;
        bail!("task_payload_output_failed: 注入的任务领取事务提交失败")
    }
    tx.commit()
        .context("task_payload_output_failed: 无法提交任务领取事务")
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
    lease_id: &str,
    progress: f64,
    message: Option<&str>,
) -> Result<Task> {
    if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
        bail!("任务进度必须在 0 到 1 之间")
    }
    let cutoff = now();
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (project_id, owner, current_lease_id, status, lease_expires_at, current_progress): (
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        f64,
    ) = tx
        .query_row(
            "SELECT project_id,lease_worker,lease_id,status,lease_expires_at,progress
             FROM tasks WHERE id=?1",
            [task_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    if !["claimed", "running"].contains(&status.as_str())
        || owner.as_deref() != Some(worker)
        || current_lease_id.as_deref() != Some(lease_id)
        || lease_expires_at
            .as_deref()
            .is_none_or(|value| value < cutoff.as_str())
    {
        bail!("task_lease_mismatch: 当前任务租约已失效")
    }
    let effective_progress = current_progress.max(progress);
    let expires_at = (Utc::now() + Duration::minutes(LEASE_MINUTES)).to_rfc3339();
    let changed = tx.execute(
        "UPDATE tasks SET status='running',progress=?2,lease_expires_at=?3
         WHERE id=?1 AND status IN ('claimed','running')
           AND lease_worker=?4 AND lease_id=?5 AND lease_expires_at>=?6",
        params![
            task_id,
            effective_progress,
            expires_at,
            worker,
            lease_id,
            &cutoff
        ],
    )?;
    if changed == 0 {
        bail!("task_lease_mismatch: 当前任务租约已失效")
    }
    append_event(
        &tx,
        task_id,
        &project_id,
        "progress",
        Some(effective_progress),
        message.unwrap_or("任务继续处理"),
    )?;
    tx.commit()?;
    find_task(&project::load(db, &project_id)?, task_id)
}

pub fn fail(
    db: &mut Connection,
    task_id: &str,
    worker: &str,
    lease_id: &str,
    message: &str,
) -> Result<Task> {
    if message.trim().is_empty() {
        bail!("失败原因不能为空")
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let cutoff = now();
    let (project_id, owner, current_lease_id, status, lease_expires_at): (
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    ) = tx
        .query_row(
            "SELECT project_id,lease_worker,lease_id,status,lease_expires_at
             FROM tasks WHERE id=?1",
            [task_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    if !["claimed", "running"].contains(&status.as_str())
        || owner.as_deref() != Some(worker)
        || current_lease_id.as_deref() != Some(lease_id)
        || lease_expires_at
            .as_deref()
            .is_none_or(|value| value < cutoff.as_str())
    {
        bail!("task_lease_mismatch: 当前任务租约已失效")
    }
    let changed = tx.execute(
        "UPDATE tasks SET status='failed',error_message=?2,lease_worker=NULL,
             lease_id=NULL,lease_expires_at=NULL,claim_payload_json=NULL
         WHERE id=?1 AND status IN ('claimed','running') AND lease_worker=?3
           AND lease_id=?4 AND lease_expires_at>=?5",
        params![task_id, message, worker, lease_id, &cutoff],
    )?;
    if changed == 0 {
        bail!("task_lease_mismatch: 当前任务租约已失效")
    }
    set_workflow_status(&tx, task_id, "failed")?;
    append_event(&tx, task_id, &project_id, "failed", None, message)?;
    tx.commit()?;
    find_task(&project::load(db, &project_id)?, task_id)
}

pub fn retry(db: &mut Connection, task_id: &str) -> Result<Task> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (project_id, kind, language): (String, String, Option<String>) = tx
        .query_row(
            "SELECT project_id,kind,language FROM tasks WHERE id=?1 AND status IN ('failed','interrupted')",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow!("只有失败或中断的任务可以重试"))?;
    let project = project::load(&tx, &project_id)?;
    let segment_ids = select_task_segments(&project, &kind, language.as_deref())?;
    let changed = tx.execute(
        "UPDATE tasks SET status='queued',progress=0,error_message=NULL,
             cancel_requested_at=NULL,base_version_id=?2,lease_worker=NULL,
             lease_id=NULL,lease_expires_at=NULL,claim_payload_json=NULL,
             glossary_version=?3
         WHERE id=?1 AND status IN ('failed','interrupted')",
        params![
            task_id,
            &project.history.current_version_id,
            (kind == "translate").then_some(i64::from(project.glossary.version))
        ],
    )?;
    if changed == 0 {
        bail!("只有失败或中断的任务可以重试")
    }
    store_task_segments(&tx, &project, task_id, &segment_ids)?;
    append_event(
        &tx,
        task_id,
        &project_id,
        "queued",
        Some(0.0),
        "任务已重新排队",
    )?;
    set_workflow_status(&tx, task_id, "waiting_agent")?;
    tx.commit()?;
    find_task(&project::load(db, &project_id)?, task_id)
}

pub(crate) fn requeue_for_runner(db: &mut Connection, task_id: &str) -> Result<Task> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let task = requeue_for_runner_in_transaction(&tx, task_id)?;
    tx.commit()?;
    Ok(task)
}

pub(crate) fn requeue_for_runner_in_transaction(
    tx: &Transaction<'_>,
    task_id: &str,
) -> Result<Task> {
    let (project_id, kind, language, expected_status, expected_lease_id): (
        String,
        String,
        Option<String>,
        String,
        Option<String>,
    ) = tx
        .query_row(
            "SELECT project_id,kind,language,status,lease_id
             FROM tasks
             WHERE id=?1 AND status IN ('failed','interrupted','cancelled')",
            [task_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| anyhow!("agent_run_not_resumable: 当前 Agent 任务不能继续"))?;
    let project = project::load(tx, &project_id)?;
    let segment_ids = select_task_segments(&project, &kind, language.as_deref())?;
    let changed = requeue_runner_task_cas(
        tx,
        task_id,
        &expected_status,
        expected_lease_id.as_deref(),
        project.history.current_version_id.as_deref(),
        (kind == "translate").then_some(i64::from(project.glossary.version)),
    )?;
    if changed == 0 {
        bail!("agent_run_not_resumable: Agent 任务状态已变化，未重新排队")
    }
    store_task_segments(tx, &project, task_id, &segment_ids)?;
    append_event(
        tx,
        task_id,
        &project_id,
        "queued",
        Some(0.0),
        "本机 Agent 任务已重新排队",
    )?;
    set_workflow_status(tx, task_id, "waiting_agent")?;
    find_task(&project::load(tx, &project_id)?, task_id)
}

fn requeue_runner_task_cas(
    tx: &Transaction<'_>,
    task_id: &str,
    expected_status: &str,
    expected_lease_id: Option<&str>,
    base_version_id: Option<&str>,
    glossary_version: Option<i64>,
) -> Result<usize> {
    Ok(tx.execute(
        "UPDATE tasks
         SET status='queued',progress=0,error_message=NULL,cancel_requested_at=NULL,
             base_version_id=?4,lease_worker=NULL,lease_id=NULL,lease_expires_at=NULL,
             claim_payload_json=NULL,glossary_version=?5
         WHERE id=?1 AND status=?2 AND lease_id IS ?3",
        params![
            task_id,
            expected_status,
            expected_lease_id,
            base_version_id,
            glossary_version
        ],
    )?)
}

pub(crate) fn finish_runner_cancel(
    db: &mut Connection,
    task_id: &str,
    expected_lease_id: Option<&str>,
) -> Result<bool> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id: String = tx
        .query_row(
            "SELECT project_id FROM tasks WHERE id=?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    let changed = tx.execute(
        "UPDATE tasks
         SET status='cancelled',cancel_requested_at=COALESCE(cancel_requested_at,?2),
             lease_worker=NULL,lease_id=NULL,lease_expires_at=NULL,claim_payload_json=NULL
         WHERE id=?1 AND status IN ('claimed','running') AND lease_id IS ?3",
        params![task_id, now(), expected_lease_id],
    )?;
    if changed > 0 {
        set_workflow_status(&tx, task_id, "cancelled")?;
        append_event(
            &tx,
            task_id,
            &project_id,
            "cancelled",
            None,
            "本机 Agent 任务已取消",
        )?;
    }
    tx.commit()?;
    Ok(changed > 0)
}

pub(crate) fn interrupt_runner(
    db: &mut Connection,
    task_id: &str,
    expected_lease_id: Option<&str>,
) -> Result<bool> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id: String = tx
        .query_row(
            "SELECT project_id FROM tasks WHERE id=?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    let changed = tx.execute(
        "UPDATE tasks
         SET status='interrupted',error_message='本机 Agent 进程意外中断；需要显式继续。',
             lease_worker=NULL,lease_id=NULL,lease_expires_at=NULL,claim_payload_json=NULL
         WHERE id=?1 AND status IN ('claimed','running') AND lease_id IS ?2",
        params![task_id, expected_lease_id],
    )?;
    if changed > 0 {
        set_workflow_status(&tx, task_id, "interrupted")?;
        append_event(
            &tx,
            task_id,
            &project_id,
            "interrupted",
            None,
            "本机 Agent 进程意外中断；需要显式继续",
        )?;
    }
    tx.commit()?;
    Ok(changed > 0)
}

pub(crate) fn fail_runner(
    db: &mut Connection,
    task_id: &str,
    worker: Option<&str>,
    expected_lease_id: Option<&str>,
    message: &str,
) -> Result<bool> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (project_id, status, owner): (String, String, Option<String>) = tx
        .query_row(
            "SELECT project_id,status,lease_worker FROM tasks WHERE id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    let eligible = status == "queued"
        || (["claimed", "running"].contains(&status.as_str()) && owner.as_deref() == worker);
    let changed = if eligible {
        let changed = tx.execute(
            "UPDATE tasks
             SET status='failed',error_message=?2,lease_worker=NULL,lease_id=NULL,
                 lease_expires_at=NULL,claim_payload_json=NULL
             WHERE id=?1 AND (
                 (status='queued' AND ?3 IS NULL)
                 OR (
                     status IN ('claimed','running')
                     AND lease_worker=?3
                     AND lease_id IS ?4
                 )
             )",
            params![task_id, message, worker, expected_lease_id],
        )?;
        if changed > 0 {
            set_workflow_status(&tx, task_id, "failed")?;
            append_event(&tx, task_id, &project_id, "failed", None, message)?;
        }
        changed
    } else {
        0
    };
    tx.commit()?;
    Ok(changed > 0)
}

pub fn cancel(db: &mut Connection, task_id: &str) -> Result<Task> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id = cancel_in_transaction(&tx, task_id)?;
    tx.commit()?;
    find_task(&project::load(db, &project_id)?, task_id)
}

pub(crate) fn cancel_in_transaction(tx: &Transaction<'_>, task_id: &str) -> Result<String> {
    let (project_id, status): (String, String) = tx
        .query_row(
            "SELECT project_id,status FROM tasks WHERE id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务不存在：{task_id}"))?;
    match status.as_str() {
        "queued" | "failed" | "interrupted" | "claimed" | "running" => {
            let changed = tx.execute(
                "UPDATE tasks
                 SET status='cancelled',cancel_requested_at=?2,lease_worker=NULL,
                     lease_id=NULL,lease_expires_at=NULL,claim_payload_json=NULL
                 WHERE id=?1 AND status=?3",
                params![task_id, now(), &status],
            )?;
            if changed == 0 {
                bail!("当前任务状态已变化，不能取消")
            }
            set_workflow_status(tx, task_id, "cancelled")?;
            append_event(
                tx,
                task_id,
                &project_id,
                "cancelled",
                None,
                "任务已取消；后续 Agent 心跳和提交将被拒绝",
            )?;
        }
        _ => bail!("当前任务状态不能取消：{status}"),
    }
    Ok(project_id)
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
    lease_id: &str,
    response: Value,
) -> Result<(String, Task, AgentPatchSet)> {
    let cutoff = now();
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let row: Option<SubmissionRow> = tx
        .query_row(
            "SELECT project_id,kind,language,status,lease_expires_at,lease_worker,
                    lease_id,base_version_id,cancel_requested_at,glossary_version
             FROM tasks WHERE id=?1",
            [task_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .optional()?;
    let Some((
        project_id,
        kind,
        language,
        status,
        expires_at,
        owner,
        current_lease_id,
        base_version_id,
        cancel_requested_at,
        glossary_version,
    )) = row
    else {
        bail!("任务不存在：{task_id}")
    };
    if cancel_requested_at.is_some() {
        bail!("task_cancel_requested: 任务已请求取消，不能提交结果")
    }
    if !["claimed", "running"].contains(&status.as_str())
        || owner.as_deref() != Some(worker)
        || current_lease_id.as_deref() != Some(lease_id)
    {
        bail!("task_lease_mismatch: 当前任务租约已失效")
    }
    if expires_at
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc) < Utc::now())
        .unwrap_or(true)
    {
        bail!("task_lease_mismatch: 任务租约已过期，请重新领取")
    }
    let response_base = response
        .get("baseVersionId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("任务响应缺少 baseVersionId"))?;
    if base_version_id.as_deref() != Some(response_base) {
        bail!("task_base_version_mismatch: Agent 响应版本与任务基线不一致")
    }
    if kind == "translate" {
        let current_glossary: i64 = tx.query_row(
            "SELECT current_version FROM project_glossaries WHERE project_id=?1",
            [&project_id],
            |row| row.get(0),
        )?;
        if glossary_version != Some(current_glossary) {
            bail!("glossary_version_conflict: 术语表版本已变化，Agent 结果未提交")
        }
    }
    let submitted_patches = response
        .get("patches")
        .or_else(|| response.get("segments"))
        .and_then(Value::as_array);
    if let Some(patches) = submitted_patches {
        let allowed = translation::task_segment_ids(&tx, task_id)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut submitted = BTreeSet::new();
        for patch in patches {
            let segment_id = patch
                .get("segmentId")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("agent_output_invalid: Agent 建议缺少字幕段 ID"))?;
            if !allowed.contains(segment_id) {
                bail!("agent_segment_unauthorized: Agent 建议包含任务范围外字幕段")
            }
            if !submitted.insert(segment_id.to_owned()) {
                bail!("agent_segment_duplicate: Agent 结果重复声明字幕段")
            }
        }
        if kind == "translate" && submitted != allowed {
            bail!("agent_batch_incomplete: 翻译任务必须逐段返回全部目标字幕")
        }
    } else if kind == "translate" {
        bail!("agent_batch_incomplete: 翻译任务缺少目标字幕结果")
    }
    let patch_set = patches::stage_in_transaction(
        &tx,
        task_id,
        &project_id,
        &kind,
        language.as_deref(),
        response_base,
        &response,
    )?;
    let changed = tx.execute(
        "UPDATE tasks
         SET status='review',progress=1,lease_worker=NULL,lease_id=NULL,
             lease_expires_at=NULL,claim_payload_json=NULL
         WHERE id=?1 AND status IN ('claimed','running')
           AND lease_worker=?2 AND lease_id=?3 AND lease_expires_at>=?4
           AND cancel_requested_at IS NULL",
        params![task_id, worker, lease_id, &cutoff],
    )?;
    if changed == 0 {
        bail!("task_lease_mismatch: 当前任务租约已失效，结果未提交")
    }
    set_workflow_status(&tx, task_id, "needs_review")?;
    crate::auto_workflow::agent_result_ready(&tx, task_id)?;
    append_event(
        &tx,
        task_id,
        &project_id,
        "review",
        Some(1.0),
        "Agent 结果已提交，等待人工审阅",
    )?;
    tx.commit()?;
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

fn select_task_segments(
    project: &Project,
    kind: &str,
    language: Option<&str>,
) -> Result<Vec<String>> {
    if kind == "translate" {
        let language = language.ok_or_else(|| anyhow!("翻译任务缺少目标语言"))?;
        let selected = translation::target_segment_ids(project, language);
        if selected.is_empty() {
            bail!("translation_up_to_date: 当前译文没有过期段或质量失败段")
        }
        return Ok(selected);
    }
    Ok(project
        .transcript
        .segments
        .iter()
        .map(|segment| segment.id.clone())
        .collect())
}

fn store_task_segments(
    db: &Connection,
    project: &Project,
    task_id: &str,
    segment_ids: &[String],
) -> Result<()> {
    db.execute("DELETE FROM task_segments WHERE task_id=?1", [task_id])?;
    for (ordinal, segment_id) in segment_ids.iter().enumerate() {
        let source = project
            .transcript
            .segments
            .iter()
            .find(|segment| segment.id == *segment_id)
            .ok_or_else(|| anyhow!("subtitle_segment_not_found: 字幕段不存在：{segment_id}"))?;
        db.execute(
            "INSERT INTO task_segments(task_id,segment_id,source_hash,ordinal) VALUES(?1,?2,?3,?4)",
            params![
                task_id,
                segment_id,
                translation::source_hash(&source.text),
                ordinal as i64
            ],
        )?;
    }
    Ok(())
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

fn set_workflow_status(db: &Connection, task_id: &str, status: &str) -> Result<()> {
    db.execute(
        "UPDATE workflows SET status=?2,updated_at=?3 WHERE task_id=?1",
        params![task_id, status, now()],
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

    fn lease_id(task: &Task) -> &str {
        task.lease.as_ref().unwrap().id.as_str()
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
            lease_id(&claim.1),
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
            lease_id(&claimed.1),
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
            lease_id(&claim.1),
            json!({"baseVersionId":base,"patches":[{"segmentId":segment_id,"before":"你好","after":"Agent 修改","reason":"修正表达","confidence":0.8}]}),
        )
        .unwrap();
        assert_eq!(submitted.2.items[0].status, "conflict");
        assert_eq!(submitted.2.items[0].current_text, "人工修改");
        assert_eq!(
            project::load(&db, &project.id).unwrap().transcript.segments[0].text,
            "人工修改"
        );
        let apply_error = patches::review_all(&mut db, &task.id, "apply")
            .unwrap_err()
            .to_string();
        assert!(apply_error.contains("patch_current_changed"));
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
    fn staged_patch_cannot_overwrite_a_later_segment_split() {
        let (_temp, mut db, project, segment_id) = fixture();
        let task = create(&mut db, &project.id, "polish", None).unwrap();
        let (_, claimed_task, payload) = claim(&mut db, "external-agent", Some(&task.id))
            .unwrap()
            .unwrap();
        submit(
            &mut db,
            &task.id,
            "external-agent",
            lease_id(&claimed_task),
            json!({
                "baseVersionId": payload["baseVersionId"],
                "patches": [{
                    "segmentId": segment_id,
                    "before": "你好",
                    "after": "Agent 修改",
                    "reason": "润色",
                    "confidence": 0.9
                }]
            }),
        )
        .unwrap();
        crate::subtitle_workbench::split(&mut db, &project.id, &segment_id, 1, 0.5).unwrap();

        let error = patches::review_all(&mut db, &task.id, "apply")
            .unwrap_err()
            .to_string();
        assert!(error.contains("patch_current_changed"));
        let updated = project::load(&db, &project.id).unwrap();
        assert_eq!(
            updated
                .transcript
                .segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect::<Vec<_>>(),
            vec!["你", "好"]
        );
        assert_eq!(
            patches::load_by_task(&db, &task.id).unwrap().items[0].status,
            "conflict"
        );
    }

    #[test]
    fn concurrent_translation_patch_sets_cannot_overwrite_the_first_review() {
        let (_temp, mut db, project, segment_id) = fixture();
        let first = create(&mut db, &project.id, "translate", Some("en".into())).unwrap();
        let second = create(&mut db, &project.id, "translate", Some("en".into())).unwrap();

        for (task, worker, translation) in [
            (&first, "first-agent", "Hello"),
            (&second, "second-agent", "Hi"),
        ] {
            let (_, claimed_task, payload) =
                claim(&mut db, worker, Some(&task.id)).unwrap().unwrap();
            submit(
                &mut db,
                &task.id,
                worker,
                lease_id(&claimed_task),
                json!({
                    "baseVersionId": payload["baseVersionId"],
                    "patches": [{
                        "segmentId": segment_id,
                        "before": "你好",
                        "after": translation,
                        "reason": "翻译为英语",
                        "confidence": 0.95
                    }]
                }),
            )
            .unwrap();
        }

        patches::review_all(&mut db, &first.id, "apply").unwrap();
        let error = patches::review_all(&mut db, &second.id, "apply")
            .unwrap_err()
            .to_string();
        assert!(error.contains("patch_current_changed"));
        assert_eq!(
            project::load(&db, &project.id).unwrap().translations["en"].segments[0].text,
            "Hello"
        );
        assert_eq!(
            patches::load_by_task(&db, &second.id).unwrap().items[0].status,
            "conflict"
        );
    }

    #[test]
    fn staged_translation_cannot_apply_after_the_glossary_changes() {
        let (_temp, mut db, project, segment_id) = fixture();
        let task = create(&mut db, &project.id, "translate", Some("en".into())).unwrap();
        let (_, claimed_task, payload) = claim(&mut db, "translation-agent", Some(&task.id))
            .unwrap()
            .unwrap();
        submit(
            &mut db,
            &task.id,
            "translation-agent",
            lease_id(&claimed_task),
            json!({
                "baseVersionId": payload["baseVersionId"],
                "patches": [{
                    "segmentId": segment_id,
                    "before": "你好",
                    "after": "Hello",
                    "reason": "翻译为英语",
                    "confidence": 0.95
                }]
            }),
        )
        .unwrap();
        db.execute(
            "UPDATE project_glossaries SET current_version=current_version+1 WHERE project_id=?1",
            [&project.id],
        )
        .unwrap();

        let error = patches::review_all(&mut db, &task.id, "apply")
            .unwrap_err()
            .to_string();
        assert!(error.contains("patch_current_changed"));
        assert!(
            !project::load(&db, &project.id)
                .unwrap()
                .translations
                .contains_key("en")
        );
        assert_eq!(
            patches::load_by_task(&db, &task.id).unwrap().items[0].status,
            "conflict"
        );
    }

    #[test]
    fn concurrent_summary_patch_sets_cannot_overwrite_the_first_review() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let first = create(&mut db, &project.id, "summary", None).unwrap();
        let second = create(&mut db, &project.id, "summary", None).unwrap();

        for (task, worker, summary) in [
            (&first, "first-agent", "第一份摘要"),
            (&second, "second-agent", "第二份摘要"),
        ] {
            let (_, claimed_task, payload) =
                claim(&mut db, worker, Some(&task.id)).unwrap().unwrap();
            submit(
                &mut db,
                &task.id,
                worker,
                lease_id(&claimed_task),
                json!({
                    "baseVersionId": payload["baseVersionId"],
                    "summary": summary,
                    "reason": "概括主要内容",
                    "confidence": 0.9
                }),
            )
            .unwrap();
        }

        patches::review_all(&mut db, &first.id, "apply").unwrap();
        let error = patches::review_all(&mut db, &second.id, "apply")
            .unwrap_err()
            .to_string();
        assert!(error.contains("patch_current_changed"));
        let summary: String = db
            .query_row(
                "SELECT text FROM summaries WHERE project_id=?1",
                [&project.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(summary, "第一份摘要");
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
    fn failed_claim_validation_leaves_task_and_workflow_untouched() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let workflow =
            crate::workflows::create(&mut db, &project.id, "translate", Some("en".into())).unwrap();
        db.execute(
            "UPDATE project_glossaries SET current_version=current_version+1 WHERE project_id=?1",
            [&project.id],
        )
        .unwrap();

        let error = claim(&mut db, "external-agent", Some(&workflow.task_id))
            .unwrap_err()
            .to_string();
        assert!(error.contains("glossary_version_conflict"));

        let task_state: (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
        ) = db
            .query_row(
                "SELECT status,lease_worker,lease_id,lease_expires_at,attempt_count FROM tasks WHERE id=?1",
                [&workflow.task_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(task_state, ("queued".into(), None, None, None, 0));
        assert_eq!(
            crate::workflows::load(&db, &workflow.id).unwrap().status,
            "waiting_agent"
        );
        let claimed_events: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM task_events WHERE task_id=?1 AND kind='claimed'",
                [&workflow.task_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(claimed_events, 0);
    }

    #[test]
    fn stale_task_base_version_is_rejected_before_lease_creation() {
        let (_temp, mut db, project, segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        project::edit_segment(
            &mut db,
            &project.id,
            &segment_id,
            "任务创建后的人工修改".into(),
        )
        .unwrap();

        let error = claim(&mut db, "external-agent", Some(&task.id))
            .unwrap_err()
            .to_string();
        assert!(error.contains("task_base_version_conflict"));
        let state: (String, Option<String>, i64) = db
            .query_row(
                "SELECT status,lease_id,attempt_count FROM tasks WHERE id=?1",
                [&task.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(state, ("queued".into(), None, 0));
    }

    #[test]
    fn partial_translation_submission_is_rejected_without_staging_a_patch_set() {
        let (_temp, mut db, project, first_segment_id) = fixture();
        project::add_segment(&mut db, &project.id, 1.0, 2.0, "第二句".into(), None).unwrap();
        let task = create(&mut db, &project.id, "translate", Some("en".into())).unwrap();
        let (_, claimed_task, payload) = claim(&mut db, "external-agent", Some(&task.id))
            .unwrap()
            .unwrap();

        let error = submit(
            &mut db,
            &task.id,
            "external-agent",
            lease_id(&claimed_task),
            json!({
                "baseVersionId": payload["baseVersionId"],
                "patches": [{
                    "segmentId": first_segment_id,
                    "before": "你好",
                    "after": "Hello",
                    "reason": "翻译为英语",
                    "confidence": 0.99
                }]
            }),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("agent_batch_incomplete"));

        let patch_sets: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM agent_patch_sets WHERE task_id=?1",
                [&task.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(patch_sets, 0);
        assert_eq!(
            find_task(&project::load(&db, &project.id).unwrap(), &task.id)
                .unwrap()
                .status,
            "claimed"
        );
    }

    #[test]
    fn heartbeat_marks_a_claimed_task_running_and_records_activity() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let (_, claimed_task, _) = claim(&mut db, "external-agent", Some(&task.id))
            .unwrap()
            .unwrap();

        let running = heartbeat(
            &mut db,
            &task.id,
            "external-agent",
            lease_id(&claimed_task),
            0.05,
            Some("开始处理任务"),
        )
        .unwrap();
        assert_eq!(running.status, "running");
        let loaded = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(loaded.last_activity.unwrap().kind, "progress");
    }

    #[test]
    fn heartbeat_progress_is_monotonic_within_the_same_lease_attempt() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let (_, claimed_task, _) = claim(&mut db, "external-agent", Some(&task.id))
            .unwrap()
            .unwrap();
        let lease_id = lease_id(&claimed_task).to_owned();

        heartbeat(
            &mut db,
            &task.id,
            "external-agent",
            &lease_id,
            0.85,
            Some("大部分已完成"),
        )
        .unwrap();
        let after_late_heartbeat = heartbeat(
            &mut db,
            &task.id,
            "external-agent",
            &lease_id,
            0.05,
            Some("迟到的低进度心跳"),
        )
        .unwrap();

        assert_eq!(after_late_heartbeat.progress, 0.85);
        let latest_event = events(&db, &task.id, 0).unwrap().pop().unwrap();
        assert_eq!(latest_event.kind, "progress");
        assert_eq!(latest_event.progress, Some(0.85));
        assert_eq!(latest_event.message, "迟到的低进度心跳");
        let loaded = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(loaded.progress, 0.85);
        assert_eq!(loaded.last_activity.unwrap().progress, Some(0.85));
    }

    #[test]
    fn cancelling_a_running_external_task_is_immediate_and_final() {
        let (_temp, mut db, project, segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let (_, claimed_task, payload) = claim(&mut db, "external-agent", Some(&task.id))
            .unwrap()
            .unwrap();
        heartbeat(
            &mut db,
            &task.id,
            "external-agent",
            lease_id(&claimed_task),
            0.05,
            Some("开始处理任务"),
        )
        .unwrap();

        let cancelled = cancel(&mut db, &task.id).unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert!(cancelled.cancel_requested_at.is_some());
        let lease: (Option<String>, Option<String>, Option<String>) = db
            .query_row(
                "SELECT lease_worker,lease_id,lease_expires_at FROM tasks WHERE id=?1",
                [&task.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(lease, (None, None, None));

        let heartbeat_error = heartbeat(
            &mut db,
            &task.id,
            "external-agent",
            lease_id(&claimed_task),
            0.5,
            Some("不应再接受进度"),
        )
        .unwrap_err()
        .to_string();
        assert!(heartbeat_error.contains("task_lease_mismatch"));

        let submit_error = submit(
            &mut db,
            &task.id,
            "external-agent",
            lease_id(&claimed_task),
            json!({
                "baseVersionId": payload["baseVersionId"],
                "patches": [{
                    "segmentId": segment_id,
                    "before": "你好",
                    "after": "不应被提交",
                    "reason": "任务已取消",
                    "confidence": 1.0
                }]
            }),
        )
        .unwrap_err()
        .to_string();
        assert!(submit_error.contains("task_cancel_requested"));
        let staged_patch_sets: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM agent_patch_sets WHERE task_id=?1",
                [&task.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(staged_patch_sets, 0);

        let latest_event = events(&db, &task.id, 0).unwrap().pop().unwrap();
        assert_eq!(latest_event.kind, "cancelled");
    }

    #[test]
    fn claim_payload_file_is_complete_and_can_be_reissued_without_a_new_attempt() {
        let (temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let first_output = temp.path().join("first-claim.json");
        fs::write(&first_output, b"previous payload").unwrap();

        let (_, first_task, first_file) = claim_to_file(
            &mut db,
            "external-agent",
            Some(&task.id),
            None,
            &first_output,
        )
        .unwrap()
        .unwrap();
        assert!(first_file.newly_claimed);
        assert_eq!(first_task.status, "claimed");
        assert_eq!(first_task.attempt_count, 1);
        let first_bytes = fs::read(&first_file.path).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&first_bytes)),
            first_file.sha256
        );
        let first_payload: Value = serde_json::from_slice(&first_bytes).unwrap();
        assert_eq!(first_payload["taskId"], task.id);
        assert_eq!(first_payload["leaseId"], lease_id(&first_task));
        assert_eq!(first_payload["attemptCount"], 1);
        assert!(first_payload["segments"].is_array());
        assert!(first_payload["baseVersionId"].is_string());
        assert!(first_payload["instructions"].is_string());
        assert!(first_payload["responseSchema"].is_object());

        db.execute(
            "UPDATE tasks SET claim_payload_json=NULL WHERE id=?1",
            [&task.id],
        )
        .unwrap();
        heartbeat(
            &mut db,
            &task.id,
            "external-agent",
            lease_id(&first_task),
            0.25,
            Some("处理中"),
        )
        .unwrap();
        let events_before_reissue = events(&db, &task.id, 0).unwrap();
        let second_output = first_output.clone();
        let (_, second_task, second_file) = claim_to_file(
            &mut db,
            "external-agent",
            Some(&task.id),
            Some(lease_id(&first_task)),
            &second_output,
        )
        .unwrap()
        .unwrap();

        assert!(!second_file.newly_claimed);
        assert_eq!(second_task.status, "running");
        assert_eq!(second_task.progress, 0.25);
        assert_eq!(second_task.attempt_count, 1);
        assert_eq!(fs::read(&second_file.path).unwrap(), first_bytes);
        assert_eq!(events(&db, &task.id, 0).unwrap(), events_before_reissue);
        let claimed_events = events_before_reissue
            .iter()
            .filter(|event| event.kind == "claimed")
            .count();
        assert_eq!(claimed_events, 1);
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.contains(".siaocut-claim-")
                && !name.ends_with(".partial")
                && !name.ends_with(".backup")
                && !name.ends_with(".rollback")
        }));

        let other_output = temp.path().join("other-agent.json");
        let other_error = claim_to_file(
            &mut db,
            "another-agent",
            Some(&task.id),
            Some(lease_id(&first_task)),
            &other_output,
        )
        .unwrap_err()
        .to_string();
        assert!(other_error.contains("task_lease_mismatch"));
        assert!(!other_output.exists());
    }

    #[test]
    fn claim_payload_write_failure_leaves_task_queued_without_a_claim_event() {
        let (temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let output_directory = temp.path().join("not-a-file");
        fs::create_dir(&output_directory).unwrap();

        let error = claim_to_file(
            &mut db,
            "external-agent",
            Some(&task.id),
            None,
            &output_directory,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("task_payload_output_failed"));

        let loaded = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(loaded.status, "queued");
        assert_eq!(loaded.attempt_count, 0);
        assert!(loaded.lease.is_none());
        assert_eq!(
            events(&db, &task.id, 0)
                .unwrap()
                .iter()
                .filter(|event| event.kind == "claimed")
                .count(),
            0
        );
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.contains(".siaocut-claim-")
                && !name.ends_with(".partial")
                && !name.ends_with(".backup")
                && !name.ends_with(".rollback")
        }));
    }

    #[test]
    fn claim_commit_failure_restores_the_previous_payload_and_database_state() {
        let (temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let output = temp.path().join("claim.json");
        let previous_payload = b"previous payload";
        fs::write(&output, previous_payload).unwrap();

        FAIL_NEXT_CLAIM_COMMIT.with(|flag| flag.set(true));
        let error = claim_to_file(&mut db, "external-agent", Some(&task.id), None, &output)
            .unwrap_err()
            .to_string();

        assert!(error.contains("注入的任务领取事务提交失败"));
        assert_eq!(fs::read(&output).unwrap(), previous_payload);
        let loaded = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(loaded.status, "queued");
        assert_eq!(loaded.attempt_count, 0);
        assert!(loaded.lease.is_none());
        assert_eq!(
            events(&db, &task.id, 0)
                .unwrap()
                .iter()
                .filter(|event| event.kind == "claimed")
                .count(),
            0
        );
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.contains(".siaocut-claim-")
                && !name.ends_with(".partial")
                && !name.ends_with(".rollback")
        }));
    }

    #[test]
    fn claim_succeeds_when_post_commit_rollback_cleanup_needs_a_drop_retry() {
        let (temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let output = temp.path().join("claim.json");
        fs::write(&output, b"previous payload").unwrap();

        CLAIM_ROLLBACK_CLEANUP_ATTEMPTS.with(|attempts| attempts.set(0));
        FAIL_NEXT_CLAIM_ROLLBACK_CLEANUP.with(|flag| flag.set(true));
        let (_, claimed_task, payload_file) =
            claim_to_file(&mut db, "external-agent", Some(&task.id), None, &output)
                .unwrap()
                .unwrap();

        assert_eq!(claimed_task.status, "claimed");
        assert_eq!(claimed_task.attempt_count, 1);
        assert_eq!(payload_file.path, fs::canonicalize(&output).unwrap());
        assert_eq!(
            CLAIM_ROLLBACK_CLEANUP_ATTEMPTS.with(|attempts| attempts.get()),
            2
        );
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.ends_with(".rollback")
        }));
        let loaded = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(loaded.status, "claimed");
        assert_eq!(loaded.attempt_count, 1);
        assert_eq!(
            events(&db, &task.id, 0)
                .unwrap()
                .iter()
                .filter(|event| event.kind == "claimed")
                .count(),
            1
        );
    }

    #[test]
    fn claim_commit_failure_removes_a_newly_published_payload() {
        let (temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let output = temp.path().join("new-claim.json");

        FAIL_NEXT_CLAIM_COMMIT.with(|flag| flag.set(true));
        claim_to_file(&mut db, "external-agent", Some(&task.id), None, &output).unwrap_err();

        assert!(!output.exists());
        let loaded = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(loaded.status, "queued");
        assert_eq!(loaded.attempt_count, 0);
        assert!(loaded.lease.is_none());
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.contains(".siaocut-claim-")
                && !name.ends_with(".partial")
                && !name.ends_with(".rollback")
        }));
    }

    #[test]
    fn stale_requeue_cas_cannot_clear_a_new_attempt_lease() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let (_, first_attempt, _) = claim(&mut db, "runner-worker", Some(&task.id))
            .unwrap()
            .unwrap();
        assert!(
            fail_runner(
                &mut db,
                &task.id,
                Some("runner-worker"),
                Some(lease_id(&first_attempt)),
                "第一次尝试失败",
            )
            .unwrap()
        );
        retry(&mut db, &task.id).unwrap();
        let (_, second_attempt, _) = claim(&mut db, "runner-worker", Some(&task.id))
            .unwrap()
            .unwrap();
        let second_lease_id = lease_id(&second_attempt).to_owned();

        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let changed = requeue_runner_task_cas(
            &tx,
            &task.id,
            "failed",
            None,
            project.history.current_version_id.as_deref(),
            None,
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(changed, 0);
        let loaded = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(loaded.status, "claimed");
        assert_eq!(lease_id(&loaded), second_lease_id);
        assert_eq!(loaded.attempt_count, 2);
    }

    #[test]
    fn stale_lease_from_an_earlier_attempt_cannot_mutate_a_retried_task() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let (_, first_task, _) = claim(&mut db, "same-worker", Some(&task.id))
            .unwrap()
            .unwrap();
        let first_lease = lease_id(&first_task).to_owned();
        fail(
            &mut db,
            &task.id,
            "same-worker",
            &first_lease,
            "第一次尝试失败",
        )
        .unwrap();
        retry(&mut db, &task.id).unwrap();
        let (_, second_task, second_payload) = claim(&mut db, "same-worker", Some(&task.id))
            .unwrap()
            .unwrap();
        let second_lease = lease_id(&second_task).to_owned();
        assert_ne!(first_lease, second_lease);
        assert_eq!(second_task.attempt_count, 2);

        let heartbeat_error = heartbeat(
            &mut db,
            &task.id,
            "same-worker",
            &first_lease,
            0.5,
            Some("旧进程心跳"),
        )
        .unwrap_err()
        .to_string();
        assert!(heartbeat_error.contains("task_lease_mismatch"));

        let fail_error = fail(
            &mut db,
            &task.id,
            "same-worker",
            &first_lease,
            "旧进程失败回写",
        )
        .unwrap_err()
        .to_string();
        assert!(fail_error.contains("task_lease_mismatch"));

        let submit_error = submit(
            &mut db,
            &task.id,
            "same-worker",
            &first_lease,
            json!({
                "baseVersionId": second_payload["baseVersionId"],
                "summary": "旧进程不应提交成功"
            }),
        )
        .unwrap_err()
        .to_string();
        assert!(submit_error.contains("task_lease_mismatch"));
        assert_eq!(
            find_task(&project::load(&db, &project.id).unwrap(), &task.id)
                .unwrap()
                .status,
            "claimed"
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM agent_patch_sets WHERE task_id=?1",
                [&task.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );

        heartbeat(
            &mut db,
            &task.id,
            "same-worker",
            &second_lease,
            0.5,
            Some("新进程继续"),
        )
        .unwrap();
        submit(
            &mut db,
            &task.id,
            "same-worker",
            &second_lease,
            json!({
                "baseVersionId": second_payload["baseVersionId"],
                "summary": "新进程提交"
            }),
        )
        .unwrap();
        assert_eq!(
            find_task(&project::load(&db, &project.id).unwrap(), &task.id)
                .unwrap()
                .status,
            "review"
        );
    }

    #[test]
    fn stale_runner_finalizers_cannot_overwrite_an_atomic_submission() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let task = create(&mut db, &project.id, "summary", None).unwrap();
        let (_, claimed_task, payload) = claim(&mut db, "runner-worker", Some(&task.id))
            .unwrap()
            .unwrap();
        submit(
            &mut db,
            &task.id,
            "runner-worker",
            lease_id(&claimed_task),
            json!({
                "baseVersionId": payload["baseVersionId"],
                "summary": "已经原子提交的结果"
            }),
        )
        .unwrap();

        let stale_lease_id = lease_id(&claimed_task).to_owned();
        assert!(!finish_runner_cancel(&mut db, &task.id, Some(&stale_lease_id)).unwrap());
        assert!(!interrupt_runner(&mut db, &task.id, Some(&stale_lease_id)).unwrap());
        assert!(
            !fail_runner(
                &mut db,
                &task.id,
                Some("runner-worker"),
                Some(&stale_lease_id),
                "迟到的 Worker 失败回写",
            )
            .unwrap()
        );

        let loaded = find_task(&project::load(&db, &project.id).unwrap(), &task.id).unwrap();
        assert_eq!(loaded.status, "review");
        assert_eq!(
            patches::load_by_task(&db, &task.id).unwrap().status,
            "pending_review"
        );
        let terminal_events = events(&db, &task.id, 0)
            .unwrap()
            .into_iter()
            .filter(|event| matches!(event.kind.as_str(), "cancelled" | "failed" | "interrupted"))
            .count();
        assert_eq!(terminal_events, 0);
    }

    #[test]
    fn stale_runner_finalizers_cannot_overwrite_a_retried_same_worker_lease() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let workflow = crate::workflows::create(&mut db, &project.id, "summary", None).unwrap();
        let (_, first_attempt, _) = claim(&mut db, "codex-run", Some(&workflow.task_id))
            .unwrap()
            .unwrap();
        let first_lease_id = lease_id(&first_attempt).to_owned();
        assert!(
            fail_runner(
                &mut db,
                &workflow.task_id,
                Some("codex-run"),
                Some(&first_lease_id),
                "第一次尝试失败",
            )
            .unwrap()
        );
        requeue_for_runner(&mut db, &workflow.task_id).unwrap();
        let (_, second_attempt, _) = claim(&mut db, "codex-run", Some(&workflow.task_id))
            .unwrap()
            .unwrap();
        let second_lease_id = lease_id(&second_attempt).to_owned();
        assert_ne!(first_lease_id, second_lease_id);
        let terminal_events_before = events(&db, &workflow.task_id, 0)
            .unwrap()
            .into_iter()
            .filter(|event| matches!(event.kind.as_str(), "cancelled" | "failed" | "interrupted"))
            .count();

        assert!(!finish_runner_cancel(&mut db, &workflow.task_id, Some(&first_lease_id),).unwrap());
        assert!(!interrupt_runner(&mut db, &workflow.task_id, Some(&first_lease_id),).unwrap());
        assert!(
            !fail_runner(
                &mut db,
                &workflow.task_id,
                Some("codex-run"),
                Some(&first_lease_id),
                "旧 Worker 迟到的失败回写",
            )
            .unwrap()
        );

        let loaded =
            find_task(&project::load(&db, &project.id).unwrap(), &workflow.task_id).unwrap();
        assert_eq!(loaded.status, "claimed");
        assert_eq!(lease_id(&loaded), second_lease_id);
        assert_eq!(loaded.attempt_count, 2);
        assert_eq!(
            crate::workflows::load(&db, &workflow.id).unwrap().status,
            "running"
        );
        let terminal_events_after = events(&db, &workflow.task_id, 0)
            .unwrap()
            .into_iter()
            .filter(|event| matches!(event.kind.as_str(), "cancelled" | "failed" | "interrupted"))
            .count();
        assert_eq!(terminal_events_after, terminal_events_before);
    }

    #[test]
    fn task_transitions_keep_the_review_workflow_status_in_sync() {
        let (_temp, mut db, project, _segment_id) = fixture();
        let workflow = crate::workflows::create(&mut db, &project.id, "summary", None).unwrap();
        assert_eq!(workflow.status, "waiting_agent");

        let (_, claimed_task, _) = claim(&mut db, "external-agent", Some(&workflow.task_id))
            .unwrap()
            .unwrap();
        assert_eq!(
            crate::workflows::load(&db, &workflow.id).unwrap().status,
            "running"
        );
        fail(
            &mut db,
            &workflow.task_id,
            "external-agent",
            lease_id(&claimed_task),
            "无法完成",
        )
        .unwrap();
        assert_eq!(
            crate::workflows::load(&db, &workflow.id).unwrap().status,
            "failed"
        );

        retry(&mut db, &workflow.task_id).unwrap();
        assert_eq!(
            crate::workflows::load(&db, &workflow.id).unwrap().status,
            "waiting_agent"
        );
        claim(&mut db, "external-agent", Some(&workflow.task_id))
            .unwrap()
            .unwrap();
        db.execute(
            "UPDATE tasks SET lease_expires_at='2000-01-01T00:00:00+00:00' WHERE id=?1",
            [&workflow.task_id],
        )
        .unwrap();
        reconcile_expired(&mut db).unwrap();
        assert_eq!(
            crate::workflows::load(&db, &workflow.id).unwrap().status,
            "interrupted"
        );

        retry(&mut db, &workflow.task_id).unwrap();
        cancel(&mut db, &workflow.task_id).unwrap();
        assert_eq!(
            crate::workflows::load(&db, &workflow.id).unwrap().status,
            "cancelled"
        );
    }
}
