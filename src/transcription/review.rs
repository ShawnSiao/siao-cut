use super::{ImportedSegment, PROVIDER_ID, ReviewItem};
use crate::{
    project,
    speaker::{SegmentSpeaker, SpeakerIdentity, SpeakerTrack, SpeakerTurn},
    util::{new_id, now},
};
use anyhow::{Result, bail};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

pub(super) fn build_track(
    segments: &[ImportedSegment],
    model_id: &str,
    timestamp: &str,
) -> SpeakerTrack {
    let mut labels = Vec::<String>::new();
    for segment in segments {
        if !labels.contains(&segment.speaker) {
            labels.push(segment.speaker.clone());
        }
    }
    let speakers = labels
        .iter()
        .enumerate()
        .map(|(index, label)| SpeakerIdentity {
            id: new_id("speaker"),
            source_label: label.clone(),
            label: format!("说话人 {}", index + 1),
            color_index: index as u32,
            created_at: timestamp.into(),
        })
        .collect::<Vec<_>>();
    let ids = speakers
        .iter()
        .map(|speaker| (speaker.source_label.clone(), speaker.id.clone()))
        .collect::<BTreeMap<_, _>>();
    let turns = segments
        .iter()
        .map(|segment| SpeakerTurn {
            id: new_id("turn"),
            speaker_id: ids[&segment.speaker].clone(),
            start: segment.start,
            end: segment.end,
            confidence: None,
            source: "moss_end_to_end".into(),
            model_version: model_id.into(),
            created_at: timestamp.into(),
        })
        .collect::<Vec<_>>();
    let associations = segments
        .iter()
        .map(|segment| SegmentSpeaker {
            segment_id: segment.id.clone(),
            speaker_id: ids[&segment.speaker].clone(),
            source: "moss_end_to_end".into(),
            confidence: None,
            updated_at: timestamp.into(),
        })
        .collect::<Vec<_>>();
    SpeakerTrack {
        status: if segments.is_empty() {
            "no_speech"
        } else {
            "ready"
        }
        .into(),
        runtime_version: "openai-compatible-loopback-v1".into(),
        segmentation_model: "end-to-end".into(),
        embedding_model: "end-to-end".into(),
        provider_id: PROVIDER_ID.into(),
        model_id: model_id.into(),
        source_kind: "end_to_end".into(),
        generated_at: Some(timestamp.into()),
        speakers,
        turns,
        associations,
    }
}

pub(super) fn build_review_items(
    project_id: &str,
    run_id: &str,
    segments: &[ImportedSegment],
    timestamp: &str,
) -> Vec<ReviewItem> {
    let mut items = Vec::new();
    let punctuation = ['。', '！', '？', '.', '!', '?', '…'];
    for (index, segment) in segments.iter().enumerate() {
        if segment.end - segment.start < 0.35 {
            items.push(review_item(
                project_id,
                run_id,
                Some(&segment.id),
                "warning",
                "short_fragment",
                "该分段很短，请检查是否为插话或错误切分。",
                timestamp,
            ));
        }
        if !segment.text.trim_end().ends_with(punctuation) {
            items.push(review_item(
                project_id,
                run_id,
                Some(&segment.id),
                "info",
                "missing_punctuation",
                "模型未输出句末标点，可提交 Agent 标点建议。",
                timestamp,
            ));
        }
        if let Some(previous) = index.checked_sub(1).and_then(|value| segments.get(value))
            && previous.speaker != segment.speaker
            && segment.start - previous.end < 0.25
        {
            items.push(review_item(
                project_id,
                run_id,
                Some(&segment.id),
                "warning",
                "rapid_speaker_switch",
                "这里发生快速说话人切换，请人工确认人物归属。",
                timestamp,
            ));
        }
    }
    items
}

fn review_item(
    project_id: &str,
    run_id: &str,
    segment_id: Option<&str>,
    severity: &str,
    kind: &str,
    message: &str,
    timestamp: &str,
) -> ReviewItem {
    ReviewItem {
        id: new_id("review"),
        project_id: project_id.into(),
        run_id: run_id.into(),
        segment_id: segment_id.map(str::to_owned),
        severity: severity.into(),
        kind: kind.into(),
        message: message.into(),
        status: "open".into(),
        created_at: timestamp.into(),
        resolved_at: None,
    }
}

pub fn review_items(db: &Connection, project_id: &str, only_open: bool) -> Result<Vec<ReviewItem>> {
    let sql = if only_open {
        "SELECT id,project_id,run_id,segment_id,severity,kind,message,status,created_at,resolved_at FROM transcription_review_items WHERE project_id=?1 AND status='open' ORDER BY CASE severity WHEN 'error' THEN 0 WHEN 'warning' THEN 1 ELSE 2 END,created_at,id"
    } else {
        "SELECT id,project_id,run_id,segment_id,severity,kind,message,status,created_at,resolved_at FROM transcription_review_items WHERE project_id=?1 ORDER BY created_at,id"
    };
    Ok(db
        .prepare(sql)?
        .query_map([project_id], |row| {
            Ok(ReviewItem {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                segment_id: row.get(3)?,
                severity: row.get(4)?,
                kind: row.get(5)?,
                message: row.get(6)?,
                status: row.get(7)?,
                created_at: row.get(8)?,
                resolved_at: row.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn resolve_review(db: &Connection, item_id: &str, action: &str) -> Result<ReviewItem> {
    if !matches!(action, "resolved" | "ignored") {
        bail!("transcription_response_invalid: 复核操作只能是 resolved 或 ignored")
    }
    let timestamp = now();
    let changed = db.execute(
        "UPDATE transcription_review_items SET status=?2,resolved_at=?3 WHERE id=?1 AND status='open'",
        params![item_id, action, timestamp],
    )?;
    if changed == 0 {
        bail!("transcription_response_invalid: 复核项不存在或已经处理")
    }
    db.query_row(
        "SELECT id,project_id,run_id,segment_id,severity,kind,message,status,created_at,resolved_at FROM transcription_review_items WHERE id=?1",
        [item_id],
        |row| {
            Ok(ReviewItem {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                segment_id: row.get(3)?,
                severity: row.get(4)?,
                kind: row.get(5)?,
                message: row.get(6)?,
                status: row.get(7)?,
                created_at: row.get(8)?,
                resolved_at: row.get(9)?,
            })
        },
    )
    .map_err(Into::into)
}

pub fn render_structured_export(
    db: &Connection,
    project_id: &str,
    format: &str,
    include_speaker_labels: bool,
    confirm_warnings: bool,
) -> Result<(String, Value)> {
    if !matches!(format, "json" | "markdown") {
        bail!("transcription_export_format_invalid: 结构化导出只支持 json 或 markdown")
    }
    let open_items = review_items(db, project_id, true)?;
    let errors = open_items
        .iter()
        .filter(|item| item.severity == "error")
        .collect::<Vec<_>>();
    let warnings = open_items
        .iter()
        .filter(|item| item.severity == "warning")
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        bail!("transcription_export_blocked: 存在未处理的错误复核项，不能导出")
    }
    if !warnings.is_empty() && !confirm_warnings {
        bail!(
            "transcription_export_warning_confirmation_required: 存在未处理的警告，确认后才能导出"
        )
    }

    let project = project::load(db, project_id)?;
    let track = crate::speaker::load_track(db, project_id)?;
    let speakers = track
        .speakers
        .iter()
        .map(|speaker| (speaker.id.as_str(), speaker))
        .collect::<HashMap<_, _>>();
    let associations = track
        .associations
        .iter()
        .map(|association| (association.segment_id.as_str(), association))
        .collect::<HashMap<_, _>>();
    let segments = project
        .transcript
        .segments
        .iter()
        .map(|segment| {
            let association = associations.get(segment.id.as_str()).copied();
            let speaker =
                association.and_then(|item| speakers.get(item.speaker_id.as_str()).copied());
            let rendered_text = match (include_speaker_labels, speaker) {
                (true, Some(speaker)) => format!("[{}] {}", speaker.label, segment.text),
                _ => segment.text.clone(),
            };
            json!({
                "id": segment.id,
                "start": segment.start,
                "end": segment.end,
                "text": segment.text,
                "renderedText": rendered_text,
                "speakerId": speaker.map(|item| item.id.as_str()),
                "speakerLabel": speaker.map(|item| item.label.as_str()),
                "speakerSource": association.map(|item| item.source.as_str()),
                "speakerConfidence": association.and_then(|item| item.confidence),
            })
        })
        .collect::<Vec<_>>();
    let evidence = json!({
        "schemaVersion": 1,
        "project": {"id": project.id, "title": project.title},
        "transcript": {
            "sourceLanguage": project.transcript.source_language,
            "speakerLabelsIncluded": include_speaker_labels,
            "segments": segments,
        },
        "speakerTrack": track,
        "review": {
            "openWarningCount": warnings.len(),
            "warningsConfirmed": !warnings.is_empty() && confirm_warnings,
            "openItems": open_items,
        }
    });
    let content = if format == "json" {
        serde_json::to_string_pretty(&evidence)?
    } else {
        let mut lines = vec![
            format!("# {}", project.title),
            String::new(),
            format!("- Provider: `{}`", track.provider_id),
            format!("- Model: `{}`", track.model_id),
            format!("- Source kind: `{}`", track.source_kind),
            format!("- Open warnings: {}", warnings.len()),
            String::new(),
            "## Transcript".into(),
            String::new(),
            "| Start | End | Speaker | Text |".into(),
            "| ---: | ---: | --- | --- |".into(),
        ];
        for segment in evidence["transcript"]["segments"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let speaker = if include_speaker_labels {
                segment["speakerLabel"].as_str().unwrap_or("")
            } else {
                ""
            };
            lines.push(format!(
                "| {:.3} | {:.3} | {} | {} |",
                segment["start"].as_f64().unwrap_or_default(),
                segment["end"].as_f64().unwrap_or_default(),
                escape_markdown_cell(speaker),
                escape_markdown_cell(segment["text"].as_str().unwrap_or("")),
            ));
        }
        lines.push(String::new());
        lines.push("## Speaker evidence".into());
        lines.push(String::new());
        lines.push("```json".into());
        lines.push(serde_json::to_string_pretty(&evidence["speakerTrack"])?);
        lines.push("```".into());
        lines.join("\n")
    };
    let audit = json!({
        "ready": true,
        "openErrorCount": errors.len(),
        "openWarningCount": warnings.len(),
        "warningsConfirmed": warnings.is_empty() || confirm_warnings,
    });
    Ok((content, audit))
}

fn escape_markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}
