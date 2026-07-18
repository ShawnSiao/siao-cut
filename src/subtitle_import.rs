use crate::{
    media,
    model::{Project, Segment, SubtitleQualityReport},
    project, subtitle_quality,
    util::{new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleFileFormat {
    Srt,
    Vtt,
    Ass,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleImportPreview {
    pub format: SubtitleFileFormat,
    pub source_path: String,
    pub sha256: String,
    pub segment_count: usize,
    pub segments: Vec<Segment>,
    pub quality: SubtitleQualityReport,
    pub can_import: bool,
    pub requires_confirmation: bool,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleImportImpact {
    pub replaced_segments: usize,
    pub removed_words: usize,
    pub removed_edits: usize,
    pub translations_marked_stale: usize,
    pub translation_segments_removed: usize,
    pub agent_patch_items_detached: usize,
    pub speaker_associations_removed: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleImportResult {
    pub format: SubtitleFileFormat,
    pub sha256: String,
    pub inserted_segments: usize,
    pub impact: SubtitleImportImpact,
    pub quality: SubtitleQualityReport,
    pub project: Project,
}

fn format_from_path(path: &Path) -> Result<SubtitleFileFormat> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("srt") => Ok(SubtitleFileFormat::Srt),
        Some("vtt") => Ok(SubtitleFileFormat::Vtt),
        Some("ass" | "ssa") => Ok(SubtitleFileFormat::Ass),
        _ => bail!("subtitle_import_format_unsupported: 只支持 SRT、VTT、ASS 和 SSA 字幕文件"),
    }
}

fn read_utf8(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| {
        format!(
            "subtitle_import_file_unreadable: 无法读取字幕文件：{}",
            path.display()
        )
    })?;
    let text = String::from_utf8(bytes).map_err(|_| {
        anyhow!("subtitle_import_encoding_unsupported: 字幕文件必须使用 UTF-8 编码")
    })?;
    Ok(text
        .strip_prefix('\u{feff}')
        .unwrap_or(&text)
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

fn parse_timestamp(value: &str) -> Result<f64> {
    let normalized = value.trim().replace(',', ".");
    let parts = normalized.split(':').collect::<Vec<_>>();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (0.0, minutes.parse::<f64>()?, seconds.parse::<f64>()?),
        [hours, minutes, seconds] => (
            hours.parse::<f64>()?,
            minutes.parse::<f64>()?,
            seconds.parse::<f64>()?,
        ),
        _ => bail!("时间码格式无效：{value}"),
    };
    let result = hours * 3600.0 + minutes * 60.0 + seconds;
    if !result.is_finite()
        || result < 0.0
        || !(0.0..60.0).contains(&minutes)
        || !(0.0..60.0).contains(&seconds)
    {
        bail!("时间码范围无效：{value}")
    }
    Ok(result)
}

fn parse_timing_line(line: &str) -> Result<(f64, f64)> {
    let (start, raw_end) = line
        .split_once("-->")
        .ok_or_else(|| anyhow!("缺少 --> 时间分隔符"))?;
    let end = raw_end
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("缺少结束时间"))?;
    Ok((parse_timestamp(start)?, parse_timestamp(end)?))
}

fn segment(ordinal: usize, start: f64, end: f64, text: String) -> Segment {
    Segment {
        id: format!("preview-{}", ordinal + 1),
        start,
        end,
        text,
        confidence: None,
    }
}

fn nonempty_blocks(text: &str) -> impl Iterator<Item = &str> {
    text.split("\n\n").filter(|block| !block.trim().is_empty())
}

fn parse_srt(text: &str) -> Result<Vec<Segment>> {
    let mut segments = Vec::new();
    for (block_index, block) in nonempty_blocks(text).enumerate() {
        let lines = block.lines().collect::<Vec<_>>();
        let timing_index = lines
            .iter()
            .position(|line| line.contains("-->"))
            .ok_or_else(|| anyhow!("第 {} 个字幕块缺少时间码", block_index + 1))?;
        let (start, end) = parse_timing_line(lines[timing_index])?;
        let text = lines[timing_index + 1..].join("\n");
        segments.push(segment(segments.len(), start, end, text));
    }
    if segments.is_empty() {
        bail!("字幕文件没有可识别的事件")
    }
    Ok(segments)
}

fn parse_vtt(text: &str) -> Result<Vec<Segment>> {
    let header = text.lines().next().unwrap_or_default().trim();
    if !header.starts_with("WEBVTT") {
        bail!("WebVTT 文件缺少 WEBVTT 文件头")
    }
    let body = text
        .split_once('\n')
        .map(|(_, body)| body)
        .unwrap_or_default();
    let mut segments = Vec::new();
    for block in nonempty_blocks(body) {
        let lines = block.lines().collect::<Vec<_>>();
        let first = lines.first().map(|line| line.trim()).unwrap_or_default();
        if first.starts_with("NOTE") || first == "STYLE" || first == "REGION" {
            continue;
        }
        let Some(timing_index) = lines.iter().position(|line| line.contains("-->")) else {
            continue;
        };
        let (start, end) = parse_timing_line(lines[timing_index])?;
        let text = lines[timing_index + 1..].join("\n");
        segments.push(segment(segments.len(), start, end, text));
    }
    if segments.is_empty() {
        bail!("WebVTT 文件没有可识别的事件")
    }
    Ok(segments)
}

fn strip_ass_overrides(value: &str) -> String {
    let mut result = String::new();
    let mut in_override = false;
    for character in value.chars() {
        match character {
            '{' => in_override = true,
            '}' if in_override => in_override = false,
            _ if !in_override => result.push(character),
            _ => {}
        }
    }
    result
        .replace("\\N", "\n")
        .replace("\\n", "\n")
        .replace("\\h", " ")
}

fn parse_ass(text: &str) -> Result<Vec<Segment>> {
    let mut in_events = false;
    let mut fields = Vec::<String>::new();
    let mut segments = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_events = line.eq_ignore_ascii_case("[Events]");
            continue;
        }
        if !in_events {
            continue;
        }
        if let Some((prefix, value)) = line.split_once(':') {
            if prefix.eq_ignore_ascii_case("Format") {
                fields = value
                    .split(',')
                    .map(|field| field.trim().to_ascii_lowercase())
                    .collect();
                continue;
            }
            if !prefix.eq_ignore_ascii_case("Dialogue") {
                continue;
            }
            if fields.is_empty() {
                bail!("ASS Events 区域缺少 Format 行")
            }
            let values = value.splitn(fields.len(), ',').collect::<Vec<_>>();
            if values.len() != fields.len() {
                bail!("ASS Dialogue 字段数量与 Format 不一致")
            }
            let start_index = fields
                .iter()
                .position(|field| field == "start")
                .ok_or_else(|| anyhow!("ASS Format 缺少 Start 字段"))?;
            let end_index = fields
                .iter()
                .position(|field| field == "end")
                .ok_or_else(|| anyhow!("ASS Format 缺少 End 字段"))?;
            let text_index = fields
                .iter()
                .position(|field| field == "text")
                .ok_or_else(|| anyhow!("ASS Format 缺少 Text 字段"))?;
            segments.push(segment(
                segments.len(),
                parse_timestamp(values[start_index])?,
                parse_timestamp(values[end_index])?,
                strip_ass_overrides(values[text_index].trim()),
            ));
        }
    }
    if segments.is_empty() {
        bail!("ASS 文件没有可识别的 Dialogue 事件")
    }
    Ok(segments)
}

fn parse(path: &Path) -> Result<(SubtitleFileFormat, Vec<Segment>)> {
    let format = format_from_path(path)?;
    let text = read_utf8(path)?;
    let segments = match format {
        SubtitleFileFormat::Srt => parse_srt(&text),
        SubtitleFileFormat::Vtt => parse_vtt(&text),
        SubtitleFileFormat::Ass => parse_ass(&text),
    }
    .map_err(|error| anyhow!("subtitle_import_parse_failed: {error}"))?;
    Ok((format, segments))
}

pub fn inspect_file(
    db: &Connection,
    project_id: &str,
    path: &Path,
) -> Result<SubtitleImportPreview> {
    let project = project::load(db, project_id)?;
    let (format, segments) = parse(path)?;
    let quality = subtitle_quality::inspect_with_language(
        &segments,
        project.media.duration_seconds,
        &project.transcript.source_language,
    );
    Ok(SubtitleImportPreview {
        format,
        source_path: path.to_string_lossy().into_owned(),
        sha256: media::hash_file(path)?,
        segment_count: segments.len(),
        segments,
        can_import: quality.error_count == 0,
        requires_confirmation: true,
        quality,
    })
}

pub fn import_file(
    db: &mut Connection,
    project_id: &str,
    path: &Path,
    confirm_replace: bool,
    expected_sha256: &str,
) -> Result<SubtitleImportResult> {
    if !confirm_replace {
        bail!("subtitle_import_confirmation_required: 替换项目字幕需要显式确认")
    }
    if expected_sha256.len() != 64 || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("subtitle_import_hash_invalid: 预检 SHA-256 格式无效")
    }
    let preview = inspect_file(db, project_id, path)?;
    if !preview.sha256.eq_ignore_ascii_case(expected_sha256) {
        bail!("subtitle_import_file_changed: 字幕文件在预检后发生变化，请重新预检")
    }
    if !preview.can_import {
        bail!("subtitle_import_quality_blocked: 字幕文件包含错误，未写入项目")
    }

    let inserted_segments = preview.segments.len();
    let mut impact = SubtitleImportImpact::default();
    let tx = db.transaction()?;
    impact.replaced_segments = tx.query_row(
        "SELECT COUNT(*) FROM segments WHERE project_id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    impact.removed_words = tx.query_row(
        "SELECT COUNT(*) FROM words WHERE project_id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    impact.removed_edits = tx.query_row(
        "SELECT COUNT(*) FROM edits WHERE project_id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    impact.speaker_associations_removed = tx.query_row(
        "SELECT COUNT(*) FROM segment_speakers WHERE project_id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    impact.translation_segments_removed = tx.execute(
        "DELETE FROM translation_segments WHERE project_id=?1",
        [project_id],
    )?;
    impact.translations_marked_stale = tx.execute(
        "UPDATE translations SET status='stale',updated_at=?2 WHERE project_id=?1 AND status!='stale'",
        params![project_id, now()],
    )?;
    tx.execute("DELETE FROM edits WHERE project_id=?1", [project_id])?;
    impact.agent_patch_items_detached = tx.execute(
        "UPDATE agent_patch_items SET segment_id=NULL WHERE patch_set_id IN (
             SELECT id FROM agent_patch_sets WHERE project_id=?1
         ) AND segment_id IS NOT NULL",
        [project_id],
    )?;
    tx.execute("DELETE FROM segments WHERE project_id=?1", [project_id])?;
    for item in &preview.segments {
        tx.execute(
            "INSERT INTO segments(id,project_id,start_seconds,end_seconds,text,confidence) VALUES(?1,?2,?3,?4,?5,NULL)",
            params![new_id("s"), project_id, item.start, item.end, &item.text],
        )?;
    }
    tx.commit()?;
    project::snapshot(db, project_id, &format!("导入 {:?} 字幕", preview.format))?;
    let project = project::load(db, project_id)?;
    Ok(SubtitleImportResult {
        format: preview.format,
        sha256: preview.sha256,
        inserted_segments,
        impact,
        quality: project.subtitle_quality.clone(),
        project,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn subtitle_import_parses_srt_vtt_and_ass_events() {
        let srt = parse_srt(
            "1\n00:00:00,000 --> 00:00:01,500\n第一句\n\n2\n00:00:01,700 --> 00:00:03,000\n第二句",
        )
        .unwrap();
        assert_eq!(srt.len(), 2);
        assert_eq!(srt[0].end, 1.5);

        let vtt = parse_vtt(
            "WEBVTT\n\nNOTE ignored\nmetadata\n\ncue-1\n00:00.000 --> 00:02.000 align:start\nHello",
        )
        .unwrap();
        assert_eq!(vtt.len(), 1);
        assert_eq!(vtt[0].text, "Hello");

        let ass = parse_ass("[Script Info]\nTitle: demo\n[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,{\\i1}第一行{\\i0}\\N第二行").unwrap();
        assert_eq!(ass.len(), 1);
        assert_eq!(ass[0].start, 1.0);
        assert_eq!(ass[0].text, "第一行\n第二行");
    }

    #[test]
    fn subtitle_import_rejects_invalid_or_changed_files_without_writes() {
        let temp = tempdir().unwrap();
        let media_path = temp.path().join("source.wav");
        let subtitle_path = temp.path().join("captions.srt");
        fs::write(&media_path, b"source").unwrap();
        fs::write(&subtitle_path, "1\n00:00:00,000 --> 00:00:01,000\n").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("db.sqlite")).unwrap();
        let project = project::create(&mut db, &media_path, None).unwrap();
        project::add_segment(&mut db, &project.id, 0.0, 1.0, "original".into(), None).unwrap();
        let before = project::load(&db, &project.id).unwrap();
        let preview = inspect_file(&db, &project.id, &subtitle_path).unwrap();
        assert!(!preview.can_import);
        assert!(import_file(&mut db, &project.id, &subtitle_path, true, &preview.sha256).is_err());
        assert_eq!(
            project::load(&db, &project.id).unwrap().transcript,
            before.transcript
        );

        fs::write(&subtitle_path, "1\n00:00:00,000 --> 00:00:01,000\nvalid").unwrap();
        let preview = inspect_file(&db, &project.id, &subtitle_path).unwrap();
        fs::write(&subtitle_path, "1\n00:00:00,000 --> 00:00:01,000\nchanged").unwrap();
        let error =
            import_file(&mut db, &project.id, &subtitle_path, true, &preview.sha256).unwrap_err();
        assert!(error.to_string().contains("subtitle_import_file_changed"));
        assert_eq!(
            project::load(&db, &project.id).unwrap().transcript,
            before.transcript
        );
    }

    #[test]
    fn subtitle_import_requires_confirmation_and_round_trips_history() {
        let temp = tempdir().unwrap();
        let media_path = temp.path().join("source.wav");
        let subtitle_path = temp.path().join("captions.vtt");
        fs::write(&media_path, b"source").unwrap();
        fs::write(
            &subtitle_path,
            "WEBVTT\n\n00:00.000 --> 00:01.500\nImported\n\n00:01.700 --> 00:03.000\nSecond",
        )
        .unwrap();
        let mut db = crate::db::open_at(&temp.path().join("db.sqlite")).unwrap();
        let project = project::create(&mut db, &media_path, None).unwrap();
        let original =
            project::add_segment(&mut db, &project.id, 0.0, 1.0, "original".into(), None).unwrap();
        let base_version = project::current_version_id(&db, &project.id)
            .unwrap()
            .unwrap();
        db.execute(
            "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,confidence,ordinal) VALUES('w1',?1,?2,0,1,'original',0.9,0)",
            params![&project.id, &original.id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO translations(project_id,language,status,updated_at) VALUES(?1,'en','current','fixture')",
            [&project.id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO translation_segments(project_id,language,segment_id,text) VALUES(?1,'en',?2,'translated')",
            params![&project.id, &original.id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO edits(id,project_id,kind,status,segment_id,start_seconds,end_seconds,reason,created_at) VALUES('e1',?1,'cut','applied',?2,0,0.5,'fixture','fixture')",
            params![&project.id, &original.id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO speaker_tracks(project_id,status,runtime_version,segmentation_model,embedding_model,generated_at) VALUES(?1,'ready','test','test','test','fixture')",
            [&project.id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO speakers(id,project_id,source_label,label,color_index,created_at) VALUES('sp1',?1,'A','A',0,'fixture')",
            [&project.id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO segment_speakers(project_id,segment_id,speaker_id,source,confidence,updated_at) VALUES(?1,?2,'sp1','manual',NULL,'fixture')",
            params![&project.id, &original.id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO tasks(id,project_id,kind,status,created_at,base_version_id,progress,attempt_count) VALUES('t1',?1,'polish','review','fixture',?2,1,1)",
            params![&project.id, &base_version],
        )
        .unwrap();
        db.execute(
            "INSERT INTO agent_patch_sets(id,task_id,project_id,kind,status,base_version_id,created_at) VALUES('p1','t1',?1,'polish','pending_review',?2,'fixture')",
            params![&project.id, &base_version],
        )
        .unwrap();
        db.execute(
            "INSERT INTO agent_patch_items(id,patch_set_id,segment_id,target,before_text,after_text,current_text_at_submit,reason,status,ordinal) VALUES('pi1','p1',?1,'source','original','revised','original','fixture','pending',0)",
            [&original.id],
        )
        .unwrap();
        project::snapshot(&db, &project.id, "fixture dependencies").unwrap();
        let preview = inspect_file(&db, &project.id, &subtitle_path).unwrap();
        let error =
            import_file(&mut db, &project.id, &subtitle_path, false, &preview.sha256).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("subtitle_import_confirmation_required")
        );

        let imported =
            import_file(&mut db, &project.id, &subtitle_path, true, &preview.sha256).unwrap();
        assert_eq!(imported.inserted_segments, 2);
        assert_eq!(imported.impact.removed_words, 1);
        assert_eq!(imported.impact.removed_edits, 1);
        assert_eq!(imported.impact.translations_marked_stale, 1);
        assert_eq!(imported.impact.translation_segments_removed, 1);
        assert_eq!(imported.impact.agent_patch_items_detached, 1);
        assert_eq!(imported.impact.speaker_associations_removed, 1);
        assert_eq!(imported.project.transcript.segments[0].text, "Imported");
        assert!(imported.project.transcript.words.is_empty());
        assert!(imported.project.edits.is_empty());
        assert_eq!(imported.project.translations["en"].status, "stale");
        assert!(imported.project.translations["en"].segments.is_empty());
        assert!(imported.project.patch_sets[0].items[0].segment_id.is_none());
        assert!(
            crate::speaker::load_track(&db, &project.id)
                .unwrap()
                .associations
                .is_empty()
        );
        assert_eq!(fs::read(&media_path).unwrap(), b"source");
        let undone = project::undo(&mut db, &project.id).unwrap();
        assert_eq!(undone.transcript.segments[0].text, "original");
        assert_eq!(undone.transcript.words.len(), 1);
        assert_eq!(undone.edits.len(), 1);
        assert_eq!(undone.translations["en"].status, "current");
        assert_eq!(
            undone.patch_sets[0].items[0].segment_id.as_deref(),
            Some(original.id.as_str())
        );
        assert_eq!(
            crate::speaker::load_track(&db, &project.id)
                .unwrap()
                .associations
                .len(),
            1
        );
        let redone = project::redo(&mut db, &project.id).unwrap();
        assert_eq!(redone.transcript.segments[0].text, "Imported");
    }
}
