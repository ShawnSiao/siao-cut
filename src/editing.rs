//! Desktop editing: the journal is private working data, never part of a Project snapshot.
use crate::{project, translation, util::now};
use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub use crate::editing_contract::{
    Draft, EditReceipt, EditingRequest, ProjectMutation, ProjectOperation, SaveEdit,
};

pub fn mutate(db: &mut Connection, request: &ProjectMutation) -> Result<serde_json::Value> {
    use crate::{
        speaker, subtitle_style, subtitle_workbench as subtitles,
        write_transaction::WriteTransaction,
    };
    use serde_json::json;
    if request.project_id.is_empty()
        || request.mutation_id.is_empty()
        || request.mutation_id.len() > 256
    {
        bail!("invalid_request: 项目修改标识无效")
    }
    let raw = serde_json::to_string(request)?;
    let mut tx = WriteTransaction::begin(db)?;
    let previous: Option<(String,String)> = tx.query_row("SELECT request_json,response_json FROM editing_receipts WHERE project_id=?1 AND mutation_id=?2", params![request.project_id,request.mutation_id], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
    if let Some((old, response)) = previous {
        if old != raw {
            bail!("editing_mutation_reused: 保存标识已经用于其他内容")
        }
        return Ok(serde_json::from_str(&response)?);
    }
    let id = &request.project_id;
    if project::current_version_id(&tx, id)? != request.expected_version_id {
        bail!("editing_version_conflict: 项目版本已变化，请重新核对操作")
    }
    let mut result = match &request.operation {
        ProjectOperation::RelinkMedia { path } => {
            json!({"project":project::relink_media(&mut tx,id,std::path::Path::new(path))?})
        }
        ProjectOperation::ReviewPatch {
            patch_item_id,
            action,
        } => {
            let owner: String = tx.query_row("SELECT project_id FROM agent_patch_sets WHERE id=(SELECT patch_set_id FROM agent_patch_items WHERE id=?1)",[patch_item_id],|row| row.get(0))?;
            if &owner != id {
                bail!("invalid_request: 补丁不属于当前项目")
            }
            let (_, set) = crate::patches::review_item(&mut tx, patch_item_id, action)?;
            json!({"patchSet":set})
        }
        ProjectOperation::ReviewAll { task_id, action } => {
            let owner: String = tx.query_row(
                "SELECT project_id FROM tasks WHERE id=?1",
                [task_id],
                |row| row.get(0),
            )?;
            if &owner != id {
                bail!("invalid_request: 任务不属于当前项目")
            }
            let (_, set) = crate::patches::review_all(&mut tx, task_id, action)?;
            json!({"patchSet":set})
        }
        ProjectOperation::ReplaceGlossary {
            language,
            expected_glossary_version,
            entries,
        } => {
            translation::replace_language(
                &mut tx,
                id,
                language,
                *expected_glossary_version,
                entries.clone(),
            )?;
            project::snapshot_in_transaction(&tx, id, "更新翻译术语表")?;
            json!({"project":project::load(&tx,id)?})
        }
        ProjectOperation::CreateWorkflow {
            workflow_kind,
            language,
            locale,
        } => {
            let workflow = crate::workflows::create_with_locale(
                &mut tx,
                id,
                workflow_kind,
                language.clone(),
                locale,
            )?;
            json!({"taskId":workflow.task_id})
        }
        ProjectOperation::ImportSubtitle {
            path,
            sha256,
            preview_version_id,
        } => {
            let imported = crate::subtitle_import::import_file_at_version(
                &mut tx,
                id,
                std::path::Path::new(path),
                true,
                sha256,
                preview_version_id,
            )?;
            json!({"project":imported.project,"insertedSegments":imported.inserted_segments,"impact":imported.impact})
        }
        ProjectOperation::DetectCuts => json!({"suggestions":crate::cuts::detect(&mut tx,id)?}),
        ProjectOperation::SetCutStatus { edit_id, action } => {
            let status = match action.as_str() {
                "apply" => "applied",
                "restore" => "restored",
                "dismiss" => "dismissed",
                _ => bail!("invalid_request: 剪辑操作无效"),
            };
            json!({"cut":crate::cuts::set_status(&mut tx,id,edit_id,status)?})
        }
        ProjectOperation::CreateWordCut {
            segment_id,
            from_word_id,
            to_word_id,
            padding_ms,
        } => {
            json!({"cut":crate::cuts::create_word_range(&mut tx,id,segment_id,from_word_id,to_word_id,*padding_ms)?})
        }
        ProjectOperation::Split {
            segment_id,
            text_offset,
            at,
        } => {
            json!({"structureEdit": subtitles::split(&mut tx,id,segment_id,*text_offset as usize,*at)?})
        }
        ProjectOperation::Merge {
            first_id,
            second_id,
        } => json!({"structureEdit": subtitles::merge(&mut tx,id,first_id,second_id," ")?}),
        ProjectOperation::Timing {
            segment_id,
            start,
            end,
        } => json!({"structureEdit": subtitles::adjust_timing(&mut tx,id,segment_id,*start,*end)?}),
        ProjectOperation::Offset { segment_ids, delta } => {
            json!({"structureEdit": subtitles::offset(&mut tx,id,segment_ids,*delta)?})
        }
        ProjectOperation::Replace {
            search,
            replacement,
        } => {
            let (project, count) = project::replace_all(&mut tx, id, search, replacement)?;
            json!({"project":project,"changedSegments":count})
        }
        ProjectOperation::Undo => json!({"project":project::undo(&mut tx,id)?}),
        ProjectOperation::Redo => json!({"project":project::redo(&mut tx,id)?}),
        ProjectOperation::Restore { version_id } => {
            project::restore_version(&mut tx, id, version_id)?;
            json!({"project":project::load(&tx,id)?})
        }
        ProjectOperation::Canvas {
            aspect_ratio,
            framing,
        } => json!({"project":project::set_canvas(&mut tx,id,aspect_ratio,framing)?}),
        ProjectOperation::Style {
            preset,
            position,
            source_font_size,
            translation_font_size,
            box_width_percent,
            box_height_lines,
        } => {
            json!({"project":subtitle_style::set(&mut tx,id,preset,position,subtitle_style::SubtitleStyleOverrides {source_font_size:*source_font_size,translation_font_size:*translation_font_size,box_width_percent:*box_width_percent,box_height_lines:*box_height_lines})?})
        }
        ProjectOperation::RenameSpeaker { speaker_id, name } => {
            json!({"speakerTrack":speaker::rename(&mut tx,id,speaker_id,name)?})
        }
        ProjectOperation::MergeSpeaker { from_id, into_id } => {
            json!({"speakerTrack":speaker::merge(&mut tx,id,from_id,into_id)?})
        }
        ProjectOperation::AssignSpeaker {
            segment_id,
            speaker_id,
        } => json!({"speakerTrack":speaker::assign(&mut tx,id,segment_id,speaker_id)?}),
    };
    result["projectId"] = json!(id);
    result["mutationId"] = json!(request.mutation_id);
    result["versionId"] = json!(project::current_version_id(&tx, id)?);
    tx.execute("INSERT INTO editing_receipts(project_id,mutation_id,request_json,response_json) VALUES(?1,?2,?3,?4)", params![id,request.mutation_id,raw,serde_json::to_string(&result)?])?;
    tx.commit()?;
    Ok(result)
}

fn validate(draft: &Draft) -> Result<()> {
    for value in [
        &draft.project_id,
        &draft.session_id,
        &draft.segment_id,
        &draft.field,
    ] {
        if value.is_empty() || value.len() > 256 || value.contains('\0') {
            bail!("invalid_request: 编辑标识无效")
        }
    }
    if draft.text.len() > 32_000 || draft.base_text.len() > 32_000 {
        bail!("invalid_request: 草稿过长")
    }
    if draft.field != "source"
        && !draft
            .field
            .strip_prefix("translation:")
            .is_some_and(|lang| {
                !lang.is_empty()
                    && lang.len() <= 16
                    && lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
    {
        bail!("invalid_request: 编辑字段无效")
    }
    Ok(())
}

pub fn journal(db: &Connection, draft: &Draft) -> Result<()> {
    validate(draft)?;
    db.execute("INSERT INTO editing_drafts(project_id,session_id,segment_id,field,base_version_id,base_text,text,revision,updated_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
        ON CONFLICT(project_id,session_id,segment_id,field) DO UPDATE SET
        base_version_id=excluded.base_version_id,base_text=excluded.base_text,text=excluded.text,revision=excluded.revision,updated_at=excluded.updated_at,discarded=0
        WHERE excluded.revision > editing_drafts.revision",
        params![draft.project_id,draft.session_id,draft.segment_id,draft.field,draft.base_version_id,draft.base_text,draft.text,draft.revision,now()])?;
    Ok(())
}

pub fn list(db: &Connection, project_id: &str) -> Result<Vec<Draft>> {
    Ok(db.prepare("SELECT session_id,segment_id,field,base_version_id,base_text,text,revision FROM editing_drafts WHERE project_id=?1 AND discarded=0 ORDER BY updated_at,session_id")?
        .query_map([project_id], |row| Ok(Draft { project_id: project_id.into(), session_id: row.get(0)?,segment_id: row.get(1)?,field: row.get(2)?,base_version_id: row.get(3)?,base_text: row.get(4)?,text: row.get(5)?,revision: row.get(6)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn discard(db: &Connection, draft: &Draft) -> Result<()> {
    validate(draft)?;
    // Never delete a more recent journal entry because an old save completed late.
    db.execute("INSERT INTO editing_drafts(project_id,session_id,segment_id,field,base_text,text,revision,updated_at,discarded)
        VALUES(?1,?2,?3,?4,'','',?5,?6,1)
        ON CONFLICT(project_id,session_id,segment_id,field) DO UPDATE SET
        base_text='',text='',revision=excluded.revision,discarded=1,updated_at=excluded.updated_at
        WHERE editing_drafts.revision<=excluded.revision",
        params![draft.project_id,draft.session_id,draft.segment_id,draft.field,draft.revision,now()])?;
    Ok(())
}

pub fn save(db: &mut Connection, edit: &SaveEdit) -> Result<EditReceipt> {
    validate(&edit.draft)?;
    for id in [&edit.mutation_id, &edit.group_id] {
        if id.is_empty() || id.len() > 256 {
            bail!("invalid_request: 保存标识无效")
        }
    }
    if edit.draft.text.trim().is_empty() {
        bail!("editing_text_empty: 字幕文本不能为空，草稿已保留")
    }
    let raw = serde_json::to_string(edit)?;
    let d = &edit.draft;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous: Option<(String, String)> = tx.query_row("SELECT request_json,response_json FROM editing_receipts WHERE project_id=?1 AND mutation_id=?2", params![d.project_id,edit.mutation_id], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
    if let Some((request, response)) = previous {
        if request != raw {
            bail!("editing_mutation_reused: 保存标识已经用于其他内容")
        }
        return Ok(serde_json::from_str(&response)?);
    }
    if project::current_version_id(&tx, &d.project_id)? != edit.expected_version_id {
        bail!("editing_version_conflict: 项目已变化，本地草稿已保留，请核对当前内容")
    }
    let current: String = if d.field == "source" {
        tx.query_row(
            "SELECT text FROM segments WHERE project_id=?1 AND id=?2",
            params![d.project_id, d.segment_id],
            |row| row.get(0),
        )?
    } else {
        tx.query_row("SELECT text FROM translation_segments WHERE project_id=?1 AND segment_id=?2 AND language=?3", params![d.project_id,d.segment_id,&d.field[12..]], |row| row.get(0))?
    };
    if current != d.base_text {
        bail!("editing_content_conflict: 字幕已变化，本地草稿已保留")
    }
    if d.field == "source" {
        project::edit_segment_in_transaction(&tx, &d.project_id, &d.segment_id, &d.text)?;
    } else {
        translation::edit_segment_in_transaction(
            &tx,
            &d.project_id,
            &d.segment_id,
            &d.field[12..],
            &d.text,
        )?;
    }
    let group = format!(
        "{}:{}:{}:{}",
        d.session_id, d.segment_id, d.field, edit.group_id
    );
    let version = project::snapshot_with_group(&tx, &d.project_id, "编辑字幕", Some(&group))?;
    let receipt = EditReceipt {
        mutation_id: edit.mutation_id.clone(),
        project_id: d.project_id.clone(),
        version_id: version.id.clone(),
        history: Some(project::history_status(&tx, &d.project_id)?),
        version: Some(version),
        segment_id: d.segment_id.clone(),
        field: d.field.clone(),
        text: d.text.clone(),
        changed_domains: vec![
            "transcript".into(),
            "translations".into(),
            "history".into(),
            "quality".into(),
            "edits".into(),
        ],
    };
    discard(&tx, d)?;
    tx.execute("INSERT INTO editing_receipts(project_id,mutation_id,request_json,response_json) VALUES(?1,?2,?3,?4)", params![d.project_id,edit.mutation_id,raw,serde_json::to_string(&receipt)?])?;
    tx.commit()?;
    Ok(receipt)
}

pub fn execute(db: &mut Connection, request: EditingRequest) -> Result<serde_json::Value> {
    use serde_json::json;
    Ok(match request {
        EditingRequest::Mutate { mutation } => mutate(db, &mutation)?,
        EditingRequest::Journal { draft } => {
            journal(db, &draft)?;
            json!({"journaled": true})
        }
        EditingRequest::List { project_id } => json!({"drafts": list(db, &project_id)?}),
        EditingRequest::Discard { draft } => {
            discard(db, &draft)?;
            json!({"discarded": true})
        }
        EditingRequest::Save { edit } => json!({"editReceipt": save(db, &edit)?}),
    })
}

#[cfg(test)]
#[path = "editing_tests.rs"]
mod tests;
