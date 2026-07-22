use crate::{
    media::{ffprobe_duration, hash_file},
    model::{
        CanvasAspectRatio, CanvasFraming, CanvasSettings, CutRange, CutSuggestion, Edit,
        HistoryState, Lease, Media, MediaArtifacts, Project, Segment, SpeechInsights, Task,
        TaskActivity, TimelineMap, Transcript, Translation, TranslationSegment, Version, Word,
    },
    patches, speaker, speech, subtitle_quality, subtitle_style, timeline, translation,
    util::{new_id, now},
    workflows,
};
use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub fn load(db: &Connection, id: &str) -> Result<Project> {
    let base: (String, String, String, String, String, String, String, String) = db
        .query_row(
            "SELECT id,title,created_at,updated_at,source_language,canvas_aspect_ratio,canvas_framing,subtitle_style_json FROM projects WHERE id=?1",
            [id],
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
                ))
            },
        )
        .optional()?
        .ok_or_else(|| anyhow!("项目不存在：{id}"))?;
    let media = db.query_row(
        "SELECT source_path,sha256,extension,duration_seconds FROM media WHERE project_id=?1",
        [id],
        |row| {
            Ok(Media {
                source_path: row.get(0)?,
                sha256: row.get(1)?,
                extension: row.get(2)?,
                duration_seconds: row.get(3)?,
            })
        },
    )?;
    let media_artifacts = db
        .query_row(
            "SELECT status,proxy_path,waveform_path,thumbnails_json,source_sha256,updated_at,error_message FROM media_artifacts WHERE project_id=?1",
            [id],
            |row| {
                let thumbnails: String = row.get(3)?;
                Ok(MediaArtifacts {
                    status: row.get(0)?,
                    proxy_path: row.get(1)?,
                    waveform_path: row.get(2)?,
                    thumbnails: serde_json::from_str(&thumbnails).unwrap_or_default(),
                    source_sha256: row.get(4)?,
                    updated_at: row.get(5)?,
                    error_message: row.get(6)?,
                })
            },
        )
        .optional()?;
    let segments = select_segments(db, id)?;
    let words = select_words(db, id)?;
    let mut translations = BTreeMap::new();
    let mut statement = db.prepare(
        "SELECT language,status,updated_at,glossary_version FROM translations WHERE project_id=?1 ORDER BY language",
    )?;
    let mut rows = statement.query([id])?;
    while let Some(row) = rows.next()? {
        let language: String = row.get(0)?;
        let mut segment_statement = db.prepare(
            "SELECT segment_id,text,source_hash,status,updated_at FROM translation_segments WHERE project_id=?1 AND language=?2 ORDER BY rowid",
        )?;
        let mut translated_segments = segment_statement
            .query_map(params![id, &language], |segment_row| {
                Ok(TranslationSegment {
                    segment_id: segment_row.get(0)?,
                    text: segment_row.get(1)?,
                    source_hash: segment_row.get(2)?,
                    status: segment_row.get(3)?,
                    updated_at: segment_row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for translated in &mut translated_segments {
            if let Some(source) = segments
                .iter()
                .find(|segment| segment.id == translated.segment_id)
            {
                translated.status =
                    translation::effective_segment_status(source, translated, &language);
            } else {
                translated.status = "stale".to_owned();
            }
        }
        let complete = !segments.is_empty()
            && segments.iter().all(|source| {
                translated_segments.iter().any(|translated| {
                    translated.segment_id == source.id && translated.status == "current"
                })
            });
        translations.insert(
            language,
            Translation {
                status: if complete {
                    "current".to_owned()
                } else {
                    "stale".to_owned()
                },
                updated_at: row.get(2)?,
                glossary_version: row.get::<_, i64>(3)?.max(0) as u32,
                segments: translated_segments,
            },
        );
    }
    let edits = db.prepare(
        "SELECT e.id,e.kind,e.status,e.segment_id,e.start_seconds,e.end_seconds,e.reason,e.created_at,
                w.from_word_id,w.to_word_id,w.selected_start_seconds,w.selected_end_seconds,
                w.padding_ms,w.transcript_hash,w.stale,
                s.suggestion_type,s.confidence,s.detector_version
         FROM edits e LEFT JOIN word_range_cuts w ON w.edit_id=e.id
         LEFT JOIN cut_suggestions s ON s.edit_id=e.id
         WHERE e.project_id=?1 ORDER BY e.start_seconds",
    )?.query_map([id],|row| {
        let from_word_id: Option<String> = row.get(8)?;
        Ok(Edit {
            id: row.get(0)?, kind: row.get(1)?, status: row.get(2)?, segment_id: row.get(3)?,
            start: row.get(4)?, end: row.get(5)?, reason: row.get(6)?, created_at: row.get(7)?,
            cut_range: from_word_id.map(|from_word_id| CutRange {
                from_word_id,
                to_word_id: row.get(9).unwrap_or_default(),
                selected_start: row.get(10).unwrap_or_default(),
                selected_end: row.get(11).unwrap_or_default(),
                padding_ms: row.get::<_, i64>(12).unwrap_or_default() as u32,
                transcript_hash: row.get(13).unwrap_or_default(),
                stale: row.get(14).unwrap_or(false),
            }),
            suggestion: row.get::<_, Option<String>>(15)?.map(|suggestion_type| CutSuggestion {
                suggestion_type,
                confidence: row.get(16).unwrap_or_default(),
                detector_version: row.get(17).unwrap_or_default(),
            }),
        })
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    let tasks = db.prepare("SELECT id,kind,language,status,created_at,completed_at,lease_worker,lease_id,lease_expires_at,base_version_id,progress,error_message,attempt_count,cancel_requested_at,workflow_id,instruction_locale,(SELECT kind FROM task_events WHERE task_id=tasks.id ORDER BY id DESC LIMIT 1),(SELECT progress FROM task_events WHERE task_id=tasks.id ORDER BY id DESC LIMIT 1),(SELECT message FROM task_events WHERE task_id=tasks.id ORDER BY id DESC LIMIT 1),(SELECT created_at FROM task_events WHERE task_id=tasks.id ORDER BY id DESC LIMIT 1) FROM tasks WHERE project_id=?1 ORDER BY created_at")?.query_map([id],|row| { let worker:Option<String>=row.get(6)?; let status:String=row.get(3)?; let error_message:Option<String>=row.get(11)?; let activity_kind:Option<String>=row.get(16)?; Ok(Task{id:row.get(0)?,kind:row.get(1)?,language:row.get(2)?,error_code:crate::model::background_error_code(&status,error_message.as_deref()),status,created_at:row.get(4)?,completed_at:row.get(5)?,lease:worker.map(|worker| Lease { worker, id:row.get(7).unwrap_or_default(), expires_at:row.get(8).unwrap_or_default()}),last_activity:activity_kind.map(|kind| TaskActivity { kind, progress:row.get(17).unwrap_or(None), message:row.get(18).unwrap_or_default(), created_at:row.get(19).unwrap_or_default() }),base_version_id:row.get(9)?,progress:row.get(10)?,error_message,attempt_count:row.get(12)?,cancel_requested_at:row.get(13)?,workflow_id:row.get(14)?,instruction_locale:row.get(15)?})})?.collect::<rusqlite::Result<Vec<_>>>()?;
    let versions = db
        .prepare(
            "SELECT id,reason,created_at FROM versions WHERE project_id=?1 ORDER BY history_index",
        )?
        .query_map([id], |row| {
            Ok(Version {
                id: row.get(0)?,
                reason: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut project = Project {
        id: base.0,
        title: base.1,
        created_at: base.2,
        updated_at: base.3,
        canvas_settings: CanvasSettings {
            aspect_ratio: CanvasAspectRatio::parse(&base.5)
                .ok_or_else(|| anyhow!("项目画布比例无效：{}", base.5))?,
            framing: CanvasFraming::parse(&base.6)
                .ok_or_else(|| anyhow!("项目画布构图无效：{}", base.6))?,
        },
        subtitle_style: subtitle_style::from_storage(&base.7)?,
        media,
        media_artifacts,
        timeline: TimelineMap::default(),
        transcript: Transcript {
            source_language: crate::model::reconcile_source_language(
                &base.4,
                segments.iter().map(|segment| segment.text.as_str()),
            ),
            segments,
            words,
        },
        subtitle_quality: Default::default(),
        speech_insights: SpeechInsights::default(),
        translations,
        glossary: translation::load_glossary(db, id)?,
        edits,
        tasks,
        versions,
        history: history_status(db, id)?,
        patch_sets: patches::for_project(db, id)?,
        workflows: workflows::for_project(db, id)?,
    };
    project.speech_insights = speech::analyze(&project.transcript);
    project.subtitle_quality = subtitle_quality::inspect_with_language(
        &project.transcript.segments,
        project.media.duration_seconds,
        &project.transcript.source_language,
    );
    project.timeline = timeline::build(&project);
    Ok(project)
}

pub fn list(db: &Connection) -> Result<Vec<Project>> {
    let mut statement = db.prepare("SELECT id FROM projects ORDER BY updated_at DESC")?;
    statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| load(db, &id))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDeletionBlocker {
    pub kind: String,
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDeletionPreflight {
    pub project_id: String,
    pub deletable: bool,
    pub blockers: Vec<ProjectDeletionBlocker>,
}

pub fn deletion_preflight(db: &Connection, project_id: &str) -> Result<ProjectDeletionPreflight> {
    db.query_row("SELECT 1 FROM projects WHERE id=?1", [project_id], |row| {
        row.get::<_, i64>(0)
    })
    .optional()?
    .ok_or_else(|| anyhow!("project_not_found: 项目不存在：{project_id}"))?;

    let blockers = db
        .prepare(
            "SELECT kind,id,status FROM (
                SELECT 'agent_task' AS kind,id,status FROM tasks
                    WHERE project_id=?1 AND status IN ('queued','claimed','running')
                UNION ALL
                SELECT 'export',id,status FROM export_jobs
                    WHERE project_id=?1 AND status IN ('queued','running')
                UNION ALL
                SELECT 'audio_analysis',id,status FROM audio_analysis_jobs
                    WHERE project_id=?1 AND status IN ('queued','running')
                UNION ALL
                SELECT 'speaker_analysis',id,status FROM speaker_jobs
                    WHERE project_id=?1 AND status IN ('queued','running')
                UNION ALL
                SELECT 'auto_workflow',id,status FROM auto_workflows
                    WHERE project_id=?1 AND status IN ('queued','running','needs_agent','needs_review','failed','interrupted')
                UNION ALL
                SELECT 'transcription',id,status FROM transcription_jobs
                    WHERE project_id=?1 AND status IN ('queued','running','finalizing','awaiting_apply')
             ) ORDER BY kind,id",
        )?
        .query_map([project_id], |row| {
            Ok(ProjectDeletionBlocker {
                kind: row.get(0)?,
                id: row.get(1)?,
                status: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(ProjectDeletionPreflight {
        project_id: project_id.to_owned(),
        deletable: blockers.is_empty(),
        blockers,
    })
}

pub fn delete(db: &mut Connection, project_id: &str) -> Result<()> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let preflight = deletion_preflight(&tx, project_id)?;
    if !preflight.deletable {
        bail!("project_busy: 项目仍有正在运行或等待处理的任务，请先取消后再删除")
    }

    let changed = tx.execute("DELETE FROM projects WHERE id=?1", [project_id])?;
    if changed != 1 {
        bail!("project_delete_failed: 无法删除项目：{project_id}")
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn select_segments(db: &Connection, project_id: &str) -> Result<Vec<Segment>> {
    Ok(db.prepare("SELECT id,start_seconds,end_seconds,text,confidence FROM segments WHERE project_id=?1 ORDER BY start_seconds,id")?.query_map([project_id],|row| Ok(Segment{id:row.get(0)?,start:row.get(1)?,end:row.get(2)?,text:row.get(3)?,confidence:row.get(4)?}))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub(crate) fn select_words(db: &Connection, project_id: &str) -> Result<Vec<Word>> {
    Ok(db.prepare("SELECT id,segment_id,start_seconds,end_seconds,text,confidence FROM words WHERE project_id=?1 ORDER BY start_seconds,ordinal")?.query_map([project_id],|row| Ok(Word{id:row.get(0)?,segment_id:row.get(1)?,start:row.get(2)?,end:row.get(3)?,text:row.get(4)?,confidence:row.get(5)?}))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn history_status(db: &Connection, project_id: &str) -> Result<HistoryState> {
    let cursor = db
        .query_row(
            "SELECT cursor_index FROM project_history WHERE project_id=?1",
            [project_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let Some(cursor) = cursor else {
        return Ok(HistoryState::default());
    };
    let current_version_id = db
        .query_row(
            "SELECT id FROM versions WHERE project_id=?1 AND history_index=?2",
            params![project_id, cursor],
            |row| row.get(0),
        )
        .optional()?;
    let can_undo = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM versions WHERE project_id=?1 AND history_index < ?2)",
        params![project_id, cursor],
        |row| row.get(0),
    )?;
    let can_redo = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM versions WHERE project_id=?1 AND history_index > ?2)",
        params![project_id, cursor],
        |row| row.get(0),
    )?;
    Ok(HistoryState {
        can_undo,
        can_redo,
        current_version_id,
    })
}

pub fn current_version_id(db: &Connection, project_id: &str) -> Result<Option<String>> {
    Ok(history_status(db, project_id)?.current_version_id)
}

pub(crate) fn snapshot_in_transaction(
    tx: &Transaction<'_>,
    project_id: &str,
    reason: &str,
) -> Result<Version> {
    let version = Version {
        id: new_id("v"),
        reason: reason.to_owned(),
        created_at: now(),
    };
    let mut snapshot = serde_json::to_value(load(tx, project_id)?)?;
    if let Value::Object(object) = &mut snapshot {
        object.insert(
            "speakerTrack".into(),
            serde_json::to_value(speaker::load_track(tx, project_id)?)?,
        );
    }
    let raw = serde_json::to_string(&snapshot)?;
    let cursor = tx
        .query_row(
            "SELECT cursor_index FROM project_history WHERE project_id=?1",
            [project_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    let next = cursor + 1;
    tx.execute(
        "DELETE FROM versions WHERE project_id=?1 AND history_index > ?2",
        params![project_id, cursor],
    )?;
    tx.execute("INSERT INTO versions(id,project_id,reason,created_at,snapshot_json,history_index) VALUES(?1,?2,?3,?4,?5,?6)",params![&version.id,project_id,&version.reason,&version.created_at,raw,next])?;
    tx.execute(
        "INSERT INTO project_history(project_id,cursor_index,updated_at) VALUES(?1,?2,?3)
         ON CONFLICT(project_id) DO UPDATE SET cursor_index=excluded.cursor_index,updated_at=excluded.updated_at",
        params![project_id, next, &version.created_at],
    )?;
    tx.execute("DELETE FROM versions WHERE id IN (SELECT id FROM versions WHERE project_id=?1 ORDER BY history_index DESC LIMIT -1 OFFSET 40)",[project_id])?;
    tx.execute(
        "UPDATE projects SET updated_at=?2 WHERE id=?1",
        params![project_id, &version.created_at],
    )?;
    tx.execute(
        "INSERT INTO operations(project_id,kind,created_at,payload_json) VALUES(?1,?2,?3,?4)",
        params![
            project_id,
            reason,
            &version.created_at,
            json!({"versionId":version.id}).to_string()
        ],
    )?;
    Ok(version)
}

#[cfg(test)]
pub(crate) fn snapshot(db: &Connection, project_id: &str, reason: &str) -> Result<Version> {
    let tx = db.unchecked_transaction()?;
    let version = snapshot_in_transaction(&tx, project_id, reason)?;
    tx.commit()?;
    Ok(version)
}

pub(crate) fn mutate_with_snapshot<T>(
    db: &mut Connection,
    project_id: &str,
    reason: &str,
    mutate: impl FnOnce(&Transaction<'_>) -> Result<T>,
) -> Result<T> {
    let tx = db.transaction()?;
    let result = mutate(&tx)?;
    snapshot_in_transaction(&tx, project_id, reason)?;
    tx.commit()?;
    Ok(result)
}

pub fn assert_segment(start: f64, end: f64, text: &str) -> Result<()> {
    if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
        bail!("无效时间范围")
    }
    if text.trim().is_empty() {
        bail!("字幕文本不能为空")
    }
    Ok(())
}

pub fn create(db: &mut Connection, media_path: &Path, title: Option<String>) -> Result<Project> {
    create_with_id(db, media_path, title, &new_id("p"))
}

pub(crate) fn create_with_id(
    db: &mut Connection,
    media_path: &Path,
    title: Option<String>,
    id: &str,
) -> Result<Project> {
    if let Ok(project) = load(db, id) {
        return Ok(project);
    }
    if !media_path.is_file() {
        bail!("媒体文件不存在：{}", media_path.display())
    }
    let extension = media_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_lowercase()))
        .unwrap_or_default();
    if ![".mp4", ".mov", ".mkv", ".mp3", ".m4a", ".wav"].contains(&extension.as_str()) {
        bail!("仅支持 mp4/mov/mkv/mp3/m4a/wav")
    }
    let created_at = now();
    let title = title.unwrap_or_else(|| {
        media_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("未命名项目")
            .to_owned()
    });
    let source_path = media_path.canonicalize()?.to_string_lossy().to_string();
    let sha256 = hash_file(media_path)?;
    let duration = ffprobe_duration(media_path);
    mutate_with_snapshot(db, id, "项目创建", |tx| {
        tx.execute(
            "INSERT INTO projects(id,title,created_at,updated_at) VALUES(?1,?2,?3,?3)",
            params![id, title, created_at],
        )?;
        tx.execute(
            "INSERT INTO media(project_id,source_path,sha256,extension,duration_seconds) VALUES(?1,?2,?3,?4,?5)",
            params![id, source_path, sha256, extension, duration],
        )?;
        tx.execute(
            "INSERT INTO project_glossaries(project_id,current_version,updated_at) VALUES(?1,0,?2)",
            params![id, &created_at],
        )?;
        tx.execute(
            "INSERT INTO glossary_versions(project_id,version,created_at) VALUES(?1,0,?2)",
            params![id, &created_at],
        )?;
        Ok(())
    })?;
    load(db, id)
}

pub fn relink_media(db: &mut Connection, project_id: &str, media_path: &Path) -> Result<Project> {
    if !media_path.is_file() {
        bail!("媒体文件不存在：{}", media_path.display())
    }
    let project = load(db, project_id)?;
    let actual_hash = hash_file(media_path)?;
    if actual_hash != project.media.sha256 {
        bail!("media_hash_changed: 所选文件与项目记录的原片不一致，已拒绝重连")
    }
    let source_path = media_path.canonicalize()?.to_string_lossy().to_string();
    let duration = ffprobe_duration(media_path);
    mutate_with_snapshot(db, project_id, "重新定位原片", |tx| {
        tx.execute(
            "UPDATE media SET source_path=?2,duration_seconds=?3 WHERE project_id=?1",
            params![project_id, source_path, duration],
        )?;
        Ok(())
    })?;
    load(db, project_id)
}

pub fn set_canvas(
    db: &mut Connection,
    project_id: &str,
    aspect_ratio: &str,
    framing: &str,
) -> Result<Project> {
    let aspect_ratio = CanvasAspectRatio::parse(aspect_ratio)
        .ok_or_else(|| anyhow!("画布比例只支持 source 或 9:16"))?;
    let framing = CanvasFraming::parse(framing)
        .ok_or_else(|| anyhow!("竖屏构图只支持 contain-blur 或 cover-center"))?;
    mutate_with_snapshot(db, project_id, "更新画布设置", |tx| {
        let changed = tx.execute(
            "UPDATE projects SET canvas_aspect_ratio=?2,canvas_framing=?3 WHERE id=?1",
            params![project_id, aspect_ratio.as_str(), framing.as_str()],
        )?;
        if changed == 0 {
            bail!("项目不存在：{project_id}")
        }
        tx.execute(
            "UPDATE media_artifacts SET status='stale',updated_at=?2 WHERE project_id=?1",
            params![project_id, now()],
        )?;
        Ok(())
    })?;
    load(db, project_id)
}

pub fn add_segment(
    db: &mut Connection,
    project_id: &str,
    start: f64,
    end: f64,
    text: String,
    confidence: Option<f64>,
) -> Result<Segment> {
    assert_segment(start, end, &text)?;
    let segment = Segment {
        id: new_id("s"),
        start,
        end,
        text,
        confidence,
    };
    mutate_with_snapshot(db, project_id, "新增字幕段", |tx| {
        tx.execute("INSERT INTO segments(id,project_id,start_seconds,end_seconds,text,confidence) VALUES(?1,?2,?3,?4,?5,?6)",params![&segment.id,project_id,segment.start,segment.end,&segment.text,segment.confidence])?;
        Ok(())
    })?;
    Ok(segment)
}

pub fn edit_segment(
    db: &mut Connection,
    project_id: &str,
    segment_id: &str,
    text: String,
) -> Result<Segment> {
    if text.trim().is_empty() {
        bail!("字幕文本不能为空")
    }
    mutate_with_snapshot(db, project_id, "编辑原文", |tx| {
        let count = tx.execute(
            "UPDATE segments SET text=?3 WHERE id=?1 AND project_id=?2",
            params![segment_id, project_id, &text],
        )?;
        if count == 0 {
            bail!("字幕段不存在：{segment_id}")
        }
        translation::invalidate_segments(tx, project_id, &[segment_id])?;
        tx.execute(
            "UPDATE word_range_cuts SET stale=1 WHERE edit_id IN (SELECT id FROM edits WHERE project_id=?1 AND segment_id=?2)",
            params![project_id, segment_id],
        )?;
        tx.execute(
            "UPDATE edits SET status='restored' WHERE project_id=?1 AND segment_id=?2 AND kind='word_cut' AND status='applied'",
            params![project_id, segment_id],
        )?;
        Ok(())
    })?;
    select_segments(db, project_id)?
        .into_iter()
        .find(|segment| segment.id == segment_id)
        .ok_or_else(|| anyhow!("字幕段不存在：{segment_id}"))
}

pub fn replace_all(
    db: &mut Connection,
    project_id: &str,
    find: &str,
    replacement: &str,
) -> Result<(Project, usize)> {
    if find.is_empty() {
        bail!("查找文字不能为空")
    }
    let segments = select_segments(db, project_id)?;
    let changes = segments
        .into_iter()
        .filter_map(|segment| {
            let text = segment.text.replace(find, replacement);
            (text != segment.text).then_some((segment.id, text))
        })
        .collect::<Vec<_>>();
    if changes.is_empty() {
        return Ok((load(db, project_id)?, 0));
    }
    mutate_with_snapshot(db, project_id, &format!("批量替换「{find}」"), |tx| {
        for (segment_id, text) in &changes {
            if text.trim().is_empty() {
                bail!("批量替换会生成空字幕，已取消操作")
            }
            tx.execute(
                "UPDATE segments SET text=?3 WHERE id=?1 AND project_id=?2",
                params![segment_id, project_id, text],
            )?;
            tx.execute(
                "UPDATE word_range_cuts SET stale=1 WHERE edit_id IN (SELECT id FROM edits WHERE project_id=?1 AND segment_id=?2)",
                params![project_id, segment_id],
            )?;
            tx.execute(
                "UPDATE edits SET status='restored' WHERE project_id=?1 AND segment_id=?2 AND kind='word_cut' AND status='applied'",
                params![project_id, segment_id],
            )?;
        }
        let changed_ids = changes
            .iter()
            .map(|(segment_id, _)| segment_id.as_str())
            .collect::<Vec<_>>();
        translation::invalidate_segments(tx, project_id, &changed_ids)?;
        Ok(())
    })?;
    Ok((load(db, project_id)?, changes.len()))
}

pub fn restore_version(db: &mut Connection, project_id: &str, version_id: &str) -> Result<Version> {
    let snapshot_json: String = db
        .query_row(
            "SELECT snapshot_json FROM versions WHERE id=?1 AND project_id=?2",
            params![version_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("版本不存在：{version_id}"))?;
    let reason = format!("恢复 {version_id}");
    let tx = db.transaction()?;
    apply_snapshot_in_transaction(&tx, project_id, &snapshot_json, None)?;
    let version = snapshot_in_transaction(&tx, project_id, &reason)?;
    tx.commit()?;
    Ok(version)
}

fn apply_snapshot_in_transaction(
    tx: &Transaction<'_>,
    project_id: &str,
    snapshot_json: &str,
    history_move: Option<(i64, &str)>,
) -> Result<()> {
    let snapshot: Value = serde_json::from_str(snapshot_json)?;
    let project: Project = serde_json::from_value(snapshot.clone())?;
    let speaker_track = snapshot
        .get("speakerTrack")
        .map(|value| serde_json::from_value::<speaker::SpeakerTrack>(value.clone()))
        .transpose()?;
    speaker::clear_track_tx(tx, project_id)?;
    tx.execute(
        "UPDATE projects SET canvas_aspect_ratio=?2,canvas_framing=?3,subtitle_style_json=?4 WHERE id=?1",
        params![
            project_id,
            project.canvas_settings.aspect_ratio.as_str(),
            project.canvas_settings.framing.as_str(),
            subtitle_style::storage_json(&project.subtitle_style)?
        ],
    )?;
    tx.execute(
        "UPDATE media_artifacts SET status='stale',updated_at=?2 WHERE project_id=?1",
        params![project_id, now()],
    )?;
    tx.execute(
        "UPDATE media SET source_path=?2,sha256=?3,extension=?4,duration_seconds=?5 WHERE project_id=?1",
        params![
            project_id,
            &project.media.source_path,
            &project.media.sha256,
            &project.media.extension,
            project.media.duration_seconds
        ],
    )?;
    tx.execute(
        "DELETE FROM translation_segments WHERE project_id=?1",
        [project_id],
    )?;
    tx.execute("DELETE FROM translations WHERE project_id=?1", [project_id])?;
    tx.execute("DELETE FROM edits WHERE project_id=?1", [project_id])?;
    tx.execute(
        "UPDATE agent_patch_items SET segment_id=NULL WHERE patch_set_id IN (
             SELECT id FROM agent_patch_sets WHERE project_id=?1
         )",
        [project_id],
    )?;
    tx.execute("DELETE FROM segments WHERE project_id=?1", [project_id])?;
    for segment in &project.transcript.segments {
        tx.execute("INSERT INTO segments(id,project_id,start_seconds,end_seconds,text,confidence) VALUES(?1,?2,?3,?4,?5,?6)",params![&segment.id,project_id,segment.start,segment.end,&segment.text,segment.confidence])?;
    }
    for (ordinal, word) in project.transcript.words.iter().enumerate() {
        tx.execute("INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,confidence,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![&word.id,project_id,&word.segment_id,word.start,word.end,&word.text,word.confidence,ordinal as i64])?;
    }
    for (language, translation) in &project.translations {
        tx.execute(
            "INSERT INTO translations(project_id,language,status,updated_at,glossary_version) VALUES(?1,?2,?3,?4,?5)",
            params![
                project_id,
                language,
                &translation.status,
                &translation.updated_at,
                i64::from(translation.glossary_version)
            ],
        )?;
        for segment in &translation.segments {
            tx.execute("INSERT INTO translation_segments(project_id,language,segment_id,text,source_hash,status,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![project_id,language,&segment.segment_id,&segment.text,&segment.source_hash,&segment.status,&segment.updated_at])?;
        }
    }
    for edit in &project.edits {
        tx.execute("INSERT INTO edits(id,project_id,kind,status,segment_id,start_seconds,end_seconds,reason,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![&edit.id,project_id,&edit.kind,&edit.status,&edit.segment_id,edit.start,edit.end,&edit.reason,&edit.created_at])?;
        if let Some(range) = &edit.cut_range {
            tx.execute(
                "INSERT INTO word_range_cuts(edit_id,from_word_id,to_word_id,selected_start_seconds,selected_end_seconds,padding_ms,transcript_hash,stale) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![&edit.id,&range.from_word_id,&range.to_word_id,range.selected_start,range.selected_end,range.padding_ms,&range.transcript_hash,range.stale],
            )?;
        }
        if let Some(suggestion) = &edit.suggestion {
            tx.execute(
                "INSERT INTO cut_suggestions(edit_id,suggestion_type,confidence,detector_version) VALUES(?1,?2,?3,?4)",
                params![&edit.id,&suggestion.suggestion_type,suggestion.confidence,&suggestion.detector_version],
            )?;
        }
    }
    for patch_set in &project.patch_sets {
        for item in &patch_set.items {
            tx.execute(
                "UPDATE agent_patch_items SET segment_id=?2 WHERE id=?1 AND patch_set_id=?3",
                params![&item.id, &item.segment_id, &patch_set.id],
            )?;
        }
    }
    speaker::replace_track_tx(tx, project_id, speaker_track.as_ref())?;
    if let Some((cursor, action)) = history_move {
        let changed_at = now();
        tx.execute(
            "UPDATE project_history SET cursor_index=?2,updated_at=?3 WHERE project_id=?1",
            params![project_id, cursor, &changed_at],
        )?;
        tx.execute(
            "UPDATE projects SET updated_at=?2 WHERE id=?1",
            params![project_id, &changed_at],
        )?;
        tx.execute(
            "INSERT INTO operations(project_id,kind,created_at,payload_json) VALUES(?1,?2,?3,?4)",
            params![
                project_id,
                action,
                &changed_at,
                json!({"cursorIndex":cursor}).to_string()
            ],
        )?;
    }
    Ok(())
}

fn move_history(db: &mut Connection, project_id: &str, undo: bool) -> Result<Project> {
    let tx = db.transaction()?;
    let cursor = tx
        .query_row(
            "SELECT cursor_index FROM project_history WHERE project_id=?1",
            [project_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("项目没有可用历史：{project_id}"))?;
    let comparison = if undo { "<" } else { ">" };
    let direction = if undo { "DESC" } else { "ASC" };
    let query = format!(
        "SELECT history_index,snapshot_json FROM versions
         WHERE project_id=?1 AND history_index {comparison} ?2
         ORDER BY history_index {direction} LIMIT 1"
    );
    let target = tx
        .query_row(&query, params![project_id, cursor], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .optional()?
        .ok_or_else(|| {
            anyhow!(if undo {
                "history_undo_empty: 没有更早的可撤销版本"
            } else {
                "history_redo_empty: 没有可重做版本"
            })
        })?;
    apply_snapshot_in_transaction(
        &tx,
        project_id,
        &target.1,
        Some((target.0, if undo { "撤销" } else { "重做" })),
    )?;
    tx.commit()?;
    load(db, project_id)
}

pub fn undo(db: &mut Connection, project_id: &str) -> Result<Project> {
    move_history(db, project_id, true)
}

pub fn redo(db: &mut Connection, project_id: &str) -> Result<Project> {
    move_history(db, project_id, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_invalid_segments() {
        assert!(assert_segment(1.0, 1.0, "x").is_err());
        assert!(assert_segment(0.0, 1.0, " ").is_err());
    }

    #[test]
    fn rolls_back_business_change_when_snapshot_write_fails() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("atomic.wav");
        std::fs::write(&media, b"audio").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("atomic.db")).unwrap();
        let project = create(&mut db, &media, Some("Atomic".into())).unwrap();
        let version_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM versions WHERE project_id=?1",
                [&project.id],
                |row| row.get(0),
            )
            .unwrap();
        let cursor: i64 = db
            .query_row(
                "SELECT cursor_index FROM project_history WHERE project_id=?1",
                [&project.id],
                |row| row.get(0),
            )
            .unwrap();
        db.execute_batch(
            "CREATE TRIGGER reject_version_snapshot
             BEFORE INSERT ON versions
             BEGIN
               SELECT RAISE(ABORT, 'forced snapshot failure');
             END;",
        )
        .unwrap();

        let error =
            add_segment(&mut db, &project.id, 0.0, 1.0, "transient".into(), None).unwrap_err();

        assert!(error.to_string().contains("forced snapshot failure"));
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM segments WHERE project_id=?1",
                [&project.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM versions WHERE project_id=?1",
                [&project.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            version_count
        );
        assert_eq!(
            db.query_row(
                "SELECT cursor_index FROM project_history WHERE project_id=?1",
                [&project.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            cursor
        );
    }

    #[test]
    fn project_history_undoes_redoes_and_discards_a_replaced_branch() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("history.wav");
        std::fs::write(&media, b"audio").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("history.db")).unwrap();
        let created = create(&mut db, &media, Some("History".into())).unwrap();
        assert!(!created.history.can_undo);
        assert!(!created.history.can_redo);

        let segment = add_segment(&mut db, &created.id, 0.0, 1.0, "first".into(), None).unwrap();
        edit_segment(&mut db, &created.id, &segment.id, "second".into()).unwrap();
        let replaced_version_id = current_version_id(&db, &created.id).unwrap().unwrap();

        let undone = undo(&mut db, &created.id).unwrap();
        assert_eq!(undone.transcript.segments[0].text, "first");
        assert!(undone.history.can_undo);
        assert!(undone.history.can_redo);
        let redone = redo(&mut db, &created.id).unwrap();
        assert_eq!(redone.transcript.segments[0].text, "second");
        assert!(!redone.history.can_redo);

        undo(&mut db, &created.id).unwrap();
        let branched = edit_segment(&mut db, &created.id, &segment.id, "branch".into()).unwrap();
        assert_eq!(branched.text, "branch");
        let project = load(&db, &created.id).unwrap();
        assert!(!project.history.can_redo);
        assert_eq!(project.versions.len(), 3);
        assert!(
            !project
                .versions
                .iter()
                .any(|version| version.id == replaced_version_id)
        );
        assert!(
            redo(&mut db, &created.id)
                .unwrap_err()
                .to_string()
                .contains("history_redo_empty")
        );
    }

    #[test]
    fn restores_and_moves_through_legacy_snapshots_without_max_lines() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("legacy-history.wav");
        std::fs::write(&media, b"audio").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("legacy-history.db")).unwrap();
        let created = create(&mut db, &media, Some("Legacy history".into())).unwrap();
        let segment = add_segment(&mut db, &created.id, 0.0, 1.0, "first".into(), None).unwrap();
        let legacy_version_id = current_version_id(&db, &created.id).unwrap().unwrap();
        edit_segment(&mut db, &created.id, &segment.id, "second".into()).unwrap();

        let snapshot_json: String = db
            .query_row(
                "SELECT snapshot_json FROM versions WHERE id=?1",
                [&legacy_version_id],
                |row| row.get(0),
            )
            .unwrap();
        let mut snapshot: Value = serde_json::from_str(&snapshot_json).unwrap();
        snapshot["subtitleQuality"]["thresholds"]
            .as_object_mut()
            .unwrap()
            .remove("maxLines");
        db.execute(
            "UPDATE versions SET snapshot_json=?2 WHERE id=?1",
            params![
                &legacy_version_id,
                serde_json::to_string(&snapshot).unwrap()
            ],
        )
        .unwrap();

        let undone = undo(&mut db, &created.id).unwrap();
        assert_eq!(undone.transcript.segments[0].text, "first");
        assert_eq!(undone.subtitle_quality.thresholds.max_lines, 2);

        let redone = redo(&mut db, &created.id).unwrap();
        assert_eq!(redone.transcript.segments[0].text, "second");
        assert_eq!(redone.subtitle_quality.thresholds.max_lines, 2);

        restore_version(&mut db, &created.id, &legacy_version_id).unwrap();
        let restored = load(&db, &created.id).unwrap();
        assert_eq!(restored.transcript.segments[0].text, "first");
        assert_eq!(restored.subtitle_quality.thresholds.max_lines, 2);
    }

    #[test]
    fn relinks_only_an_identical_media_file() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("relink.db");
        let original = temp.path().join("original.mp4");
        let moved = temp.path().join("moved.mp4");
        let wrong = temp.path().join("wrong.mp4");
        std::fs::write(&original, b"same media bytes").unwrap();
        std::fs::write(&moved, b"same media bytes").unwrap();
        std::fs::write(&wrong, b"different bytes").unwrap();
        let mut db = crate::db::open_at(&database).unwrap();
        let project = create(&mut db, &original, None).unwrap();
        let relinked = relink_media(&mut db, &project.id, &moved).unwrap();
        assert_eq!(
            Path::new(&relinked.media.source_path),
            moved.canonicalize().unwrap()
        );
        let error = relink_media(&mut db, &project.id, &wrong)
            .unwrap_err()
            .to_string();
        assert!(error.contains("media_hash_changed"));
    }

    #[test]
    fn persists_valid_canvas_settings_and_rejects_unknown_values() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("canvas.db");
        let media = temp.path().join("sample.mp4");
        std::fs::write(&media, b"media bytes").unwrap();
        let mut db = crate::db::open_at(&database).unwrap();
        let project = create(&mut db, &media, None).unwrap();
        assert_eq!(
            project.canvas_settings.aspect_ratio,
            CanvasAspectRatio::Source
        );
        assert_eq!(project.canvas_settings.framing, CanvasFraming::ContainBlur);

        db.execute(
            "INSERT INTO media_artifacts(project_id,status,proxy_path,thumbnails_json,source_sha256,updated_at) VALUES(?1,'ready','proxy.mp4','[]',?2,'now')",
            params![&project.id, &project.media.sha256],
        )
        .unwrap();

        let updated = set_canvas(&mut db, &project.id, "9:16", "cover-center").unwrap();
        assert_eq!(
            updated.canvas_settings.aspect_ratio,
            CanvasAspectRatio::Vertical
        );
        assert_eq!(updated.canvas_settings.framing, CanvasFraming::CoverCenter);
        assert_eq!(updated.versions.last().unwrap().reason, "更新画布设置");
        let artifact_status: String = db
            .query_row(
                "SELECT status FROM media_artifacts WHERE project_id=?1",
                [&project.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(artifact_status, "stale");
        assert!(set_canvas(&mut db, &project.id, "16:9", "cover-center").is_err());
        assert!(set_canvas(&mut db, &project.id, "9:16", "stretch").is_err());
    }

    #[test]
    fn replaces_matching_segments_as_one_recoverable_operation() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("replace.db");
        let media = temp.path().join("sample.mp4");
        std::fs::write(&media, b"media bytes").unwrap();
        let mut db = crate::db::open_at(&database).unwrap();
        let project = create(&mut db, &media, None).unwrap();
        add_segment(
            &mut db,
            &project.id,
            0.0,
            1.0,
            "今天介绍 SiaoCut".to_owned(),
            Some(0.9),
        )
        .unwrap();
        add_segment(
            &mut db,
            &project.id,
            1.0,
            2.0,
            "SiaoCut 保留原片".to_owned(),
            Some(0.8),
        )
        .unwrap();
        db.execute(
            "INSERT INTO translations(project_id,language,status,updated_at) VALUES(?1,'en','ready','now')",
            [&project.id],
        )
        .unwrap();

        let (updated, changed) = replace_all(&mut db, &project.id, "SiaoCut", "Siao Cut").unwrap();

        assert_eq!(changed, 2);
        assert!(
            updated
                .transcript
                .segments
                .iter()
                .all(|segment| segment.text.contains("Siao Cut"))
        );
        assert_eq!(updated.translations["en"].status, "stale");
        assert_eq!(
            updated.versions.last().unwrap().reason,
            "批量替换「SiaoCut」"
        );

        let error = replace_all(&mut db, &project.id, "今天介绍 Siao Cut", "")
            .unwrap_err()
            .to_string();
        assert!(error.contains("生成空字幕"));
        assert!(
            load(&db, &project.id).unwrap().transcript.segments[0]
                .text
                .contains("今天介绍 Siao Cut")
        );
    }

    #[test]
    fn deletes_project_records_without_deleting_source_media() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("delete.db");
        let media = temp.path().join("keep-me.mp4");
        std::fs::write(&media, b"original media bytes").unwrap();
        let mut db = crate::db::open_at(&database).unwrap();
        let project = create(&mut db, &media, Some("Delete me".into())).unwrap();

        delete(&mut db, &project.id).unwrap();

        assert!(media.exists());
        assert!(load(&db, &project.id).is_err());
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn refuses_to_delete_a_project_with_active_work() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("busy-delete.db");
        let media = temp.path().join("keep-me.mp4");
        std::fs::write(&media, b"original media bytes").unwrap();
        let mut db = crate::db::open_at(&database).unwrap();
        let project = create(&mut db, &media, Some("Busy".into())).unwrap();
        db.execute(
            "INSERT INTO tasks(id,project_id,kind,status,created_at,progress,attempt_count) VALUES('t-busy',?1,'polish','queued','now',0,0)",
            [&project.id],
        )
        .unwrap();

        let error = delete(&mut db, &project.id).unwrap_err().to_string();

        assert!(error.contains("project_busy"));
        assert!(load(&db, &project.id).is_ok());
        assert!(media.exists());
    }

    #[test]
    fn refuses_to_orphan_a_resumable_auto_workflow() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("busy-auto-delete.db");
        let media = temp.path().join("keep-me.mp4");
        std::fs::write(&media, b"original media bytes").unwrap();
        let mut db = crate::db::open_at(&database).unwrap();
        let project = create(&mut db, &media, Some("Busy auto".into())).unwrap();
        db.execute(
            "INSERT INTO auto_workflows(
                id,input_kind,input_value,project_id,model_path,output_path,burn_subtitles,
                subtitle_mode,status,current_stage,progress,created_at,updated_at
             ) VALUES('auto-busy','local',?2,?1,'model.bin','output.mp4',0,
                'source','needs_agent','translate',0.62,'now','now')",
            rusqlite::params![&project.id, media.to_string_lossy()],
        )
        .unwrap();

        let error = delete(&mut db, &project.id).unwrap_err().to_string();

        assert!(error.contains("project_busy"));
        assert!(load(&db, &project.id).is_ok());
        assert!(media.exists());
    }

    #[test]
    fn refuses_to_delete_a_project_with_active_or_pending_transcription() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("busy-transcription-delete.db");
        let media = temp.path().join("keep-me.mp4");
        std::fs::write(&media, b"original media bytes").unwrap();
        let mut db = crate::db::open_at(&database).unwrap();
        let project = create(&mut db, &media, Some("Busy transcription".into())).unwrap();
        db.execute(
            "INSERT INTO transcription_jobs(
                id,project_id,provider_id,endpoint,model_id,status,stage,created_at,updated_at
             ) VALUES('transcription-busy',?1,'moss_openai','http://127.0.0.1:8000','moss','awaiting_apply','awaiting_apply','now','now')",
            [&project.id],
        )
        .unwrap();

        let preflight = deletion_preflight(&db, &project.id).unwrap();
        assert!(!preflight.deletable);
        assert_eq!(preflight.blockers[0].kind, "transcription");
        let error = delete(&mut db, &project.id).unwrap_err().to_string();

        assert!(error.contains("project_busy"));
        assert!(load(&db, &project.id).is_ok());
        assert!(media.exists());
    }
}
