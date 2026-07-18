use crate::{
    model::{Project, Segment},
    project,
    util::{new_id, now},
};
use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

const TIME_EPSILON: f64 = 0.000_001;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureImpact {
    pub translations_marked_stale: usize,
    pub translation_segments_removed: usize,
    pub words_reassigned: usize,
    pub words_removed: usize,
    pub words_shifted: usize,
    pub edits_restored: usize,
    pub word_cuts_invalidated: usize,
    pub agent_patch_items_rebased: usize,
    pub speaker_associations_copied: usize,
    pub speaker_associations_removed: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureEditResult {
    pub operation: String,
    pub affected_segment_ids: Vec<String>,
    pub created_segment_id: Option<String>,
    pub removed_segment_ids: Vec<String>,
    pub impact: StructureImpact,
    pub project: Project,
}

struct StructureEditChange {
    operation: &'static str,
    affected_segment_ids: Vec<String>,
    created_segment_id: Option<String>,
    removed_segment_ids: Vec<String>,
    impact: StructureImpact,
}

fn ordered_segments(db: &Connection, project_id: &str) -> Result<Vec<Segment>> {
    project::select_segments(db, project_id)
}

fn segment_by_id(segments: &[Segment], segment_id: &str) -> Result<Segment> {
    segments
        .iter()
        .find(|segment| segment.id == segment_id)
        .cloned()
        .ok_or_else(|| anyhow!("subtitle_segment_not_found: 字幕段不存在：{segment_id}"))
}

fn assert_media_bound(project: &Project, end: f64) -> Result<()> {
    if let Some(duration) = project.media.duration_seconds
        && duration.is_finite()
        && end > duration + TIME_EPSILON
    {
        bail!("subtitle_time_out_of_bounds: 字幕结束时间超过原片时长")
    }
    Ok(())
}

fn invalidate_translations(
    tx: &Transaction<'_>,
    project_id: &str,
    segment_ids: &[&str],
    impact: &mut StructureImpact,
) -> Result<()> {
    for segment_id in segment_ids {
        impact.translation_segments_removed += tx.execute(
            "DELETE FROM translation_segments WHERE project_id=?1 AND segment_id=?2",
            params![project_id, segment_id],
        )?;
    }
    impact.translations_marked_stale += tx.execute(
        "UPDATE translations SET status='stale',updated_at=?2 WHERE project_id=?1 AND status!='stale'",
        params![project_id, now()],
    )?;
    Ok(())
}

fn invalidate_edits(
    tx: &Transaction<'_>,
    project_id: &str,
    segment_ids: &[&str],
    impact: &mut StructureImpact,
) -> Result<()> {
    for segment_id in segment_ids {
        impact.word_cuts_invalidated += tx.execute(
            "DELETE FROM word_range_cuts WHERE edit_id IN (
                 SELECT id FROM edits WHERE project_id=?1 AND segment_id=?2
             )",
            params![project_id, segment_id],
        )?;
        impact.edits_restored += tx.execute(
            "UPDATE edits SET status='restored' WHERE project_id=?1 AND segment_id=?2 AND status IN ('proposed','applied')",
            params![project_id, segment_id],
        )?;
    }
    Ok(())
}

fn remove_automatic_speaker_association(
    tx: &Transaction<'_>,
    project_id: &str,
    segment_id: &str,
    impact: &mut StructureImpact,
) -> Result<()> {
    impact.speaker_associations_removed += tx.execute(
        "DELETE FROM segment_speakers WHERE project_id=?1 AND segment_id=?2 AND source!='manual'",
        params![project_id, segment_id],
    )?;
    Ok(())
}

fn copy_speaker_association(
    tx: &Transaction<'_>,
    project_id: &str,
    from_segment_id: &str,
    to_segment_id: &str,
    impact: &mut StructureImpact,
) -> Result<()> {
    impact.speaker_associations_copied += tx.execute(
        "INSERT INTO segment_speakers(project_id,segment_id,speaker_id,source,confidence,updated_at)
         SELECT project_id,?3,speaker_id,source,confidence,?4
         FROM segment_speakers WHERE project_id=?1 AND segment_id=?2",
        params![project_id, from_segment_id, to_segment_id, now()],
    )?;
    Ok(())
}

fn merge_speaker_associations(
    tx: &Transaction<'_>,
    project_id: &str,
    left_id: &str,
    right_id: &str,
    impact: &mut StructureImpact,
) -> Result<()> {
    type Association = (String, String, Option<f64>);
    let read = |segment_id: &str| -> Result<Option<Association>> {
        Ok(tx
            .query_row(
                "SELECT speaker_id,source,confidence FROM segment_speakers WHERE project_id=?1 AND segment_id=?2",
                params![project_id, segment_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?)
    };
    let left = read(left_id)?;
    let right = read(right_id)?;
    match (left, right) {
        (_, None) => {}
        (None, Some(_)) => {
            tx.execute(
                "UPDATE segment_speakers SET segment_id=?3,updated_at=?4 WHERE project_id=?1 AND segment_id=?2",
                params![project_id, right_id, left_id, now()],
            )?;
        }
        (Some((left_speaker, _, _)), Some((right_speaker, _, _)))
            if left_speaker == right_speaker =>
        {
            impact.speaker_associations_removed += tx.execute(
                "DELETE FROM segment_speakers WHERE project_id=?1 AND segment_id=?2",
                params![project_id, right_id],
            )?;
        }
        (Some((_, left_source, _)), Some(_)) if left_source == "manual" => {
            impact.speaker_associations_removed += tx.execute(
                "DELETE FROM segment_speakers WHERE project_id=?1 AND segment_id=?2",
                params![project_id, right_id],
            )?;
        }
        (Some(_), Some((_, right_source, _))) if right_source == "manual" => {
            impact.speaker_associations_removed += tx.execute(
                "DELETE FROM segment_speakers WHERE project_id=?1 AND segment_id=?2",
                params![project_id, left_id],
            )?;
            tx.execute(
                "UPDATE segment_speakers SET segment_id=?3,updated_at=?4 WHERE project_id=?1 AND segment_id=?2",
                params![project_id, right_id, left_id, now()],
            )?;
        }
        (Some(_), Some(_)) => {
            impact.speaker_associations_removed += tx.execute(
                "DELETE FROM segment_speakers WHERE project_id=?1 AND segment_id IN (?2,?3)",
                params![project_id, left_id, right_id],
            )?;
        }
    }
    Ok(())
}

fn finish(
    db: &mut Connection,
    project_id: &str,
    reason: &str,
    change: StructureEditChange,
) -> Result<StructureEditResult> {
    project::snapshot(db, project_id, reason)?;
    Ok(StructureEditResult {
        operation: change.operation.to_owned(),
        affected_segment_ids: change.affected_segment_ids,
        created_segment_id: change.created_segment_id,
        removed_segment_ids: change.removed_segment_ids,
        impact: change.impact,
        project: project::load(db, project_id)?,
    })
}

pub fn split(
    db: &mut Connection,
    project_id: &str,
    segment_id: &str,
    text_offset: usize,
    at_seconds: f64,
) -> Result<StructureEditResult> {
    let loaded = project::load(db, project_id)?;
    let segment = segment_by_id(&loaded.transcript.segments, segment_id)?;
    if !at_seconds.is_finite()
        || at_seconds <= segment.start + TIME_EPSILON
        || at_seconds >= segment.end - TIME_EPSILON
    {
        bail!("subtitle_split_time_invalid: 拆分时间必须位于字幕段内部")
    }
    let char_count = segment.text.chars().count();
    if text_offset == 0 || text_offset >= char_count {
        bail!("subtitle_split_text_invalid: 文字拆分位置必须位于非空文本内部")
    }
    let byte_offset = segment
        .text
        .char_indices()
        .nth(text_offset)
        .map(|(offset, _)| offset)
        .ok_or_else(|| anyhow!("subtitle_split_text_invalid: 无法解析文字拆分位置"))?;
    let left_text = segment.text[..byte_offset].trim().to_owned();
    let right_text = segment.text[byte_offset..].trim().to_owned();
    project::assert_segment(segment.start, at_seconds, &left_text)?;
    project::assert_segment(at_seconds, segment.end, &right_text)?;
    let words = loaded
        .transcript
        .words
        .iter()
        .filter(|word| word.segment_id == segment_id)
        .cloned()
        .collect::<Vec<_>>();
    if words
        .iter()
        .any(|word| word.start < at_seconds - TIME_EPSILON && word.end > at_seconds + TIME_EPSILON)
    {
        bail!("subtitle_split_crosses_word: 拆分时间不能穿过词级证据")
    }
    let right_word_ids = words
        .iter()
        .filter(|word| word.start >= at_seconds - TIME_EPSILON)
        .map(|word| word.id.clone())
        .collect::<Vec<_>>();
    let right_id = new_id("s");
    let mut impact = StructureImpact::default();
    let tx = db.transaction()?;
    tx.execute(
        "UPDATE segments SET end_seconds=?3,text=?4 WHERE project_id=?1 AND id=?2",
        params![project_id, segment_id, at_seconds, left_text],
    )?;
    tx.execute(
        "INSERT INTO segments(id,project_id,start_seconds,end_seconds,text,confidence) VALUES(?1,?2,?3,?4,?5,?6)",
        params![&right_id, project_id, at_seconds, segment.end, right_text, segment.confidence],
    )?;
    for word_id in &right_word_ids {
        impact.words_reassigned += tx.execute(
            "UPDATE words SET segment_id=?3 WHERE project_id=?1 AND id=?2",
            params![project_id, word_id, &right_id],
        )?;
    }
    invalidate_translations(&tx, project_id, &[segment_id], &mut impact)?;
    invalidate_edits(&tx, project_id, &[segment_id], &mut impact)?;
    copy_speaker_association(&tx, project_id, segment_id, &right_id, &mut impact)?;
    tx.commit()?;
    finish(
        db,
        project_id,
        "拆分字幕段",
        StructureEditChange {
            operation: "split",
            affected_segment_ids: vec![segment_id.to_owned(), right_id.clone()],
            created_segment_id: Some(right_id),
            removed_segment_ids: Vec::new(),
            impact,
        },
    )
}

pub fn merge(
    db: &mut Connection,
    project_id: &str,
    first_segment_id: &str,
    second_segment_id: &str,
    separator: &str,
) -> Result<StructureEditResult> {
    if first_segment_id == second_segment_id {
        bail!("subtitle_merge_same: 不能合并同一个字幕段")
    }
    if separator.chars().count() > 8 {
        bail!("subtitle_merge_separator_invalid: 合并分隔符不能超过 8 个字符")
    }
    let segments = ordered_segments(db, project_id)?;
    let first_index = segments
        .iter()
        .position(|segment| segment.id == first_segment_id)
        .ok_or_else(|| anyhow!("subtitle_segment_not_found: 字幕段不存在：{first_segment_id}"))?;
    let second_index = segments
        .iter()
        .position(|segment| segment.id == second_segment_id)
        .ok_or_else(|| anyhow!("subtitle_segment_not_found: 字幕段不存在：{second_segment_id}"))?;
    if first_index.abs_diff(second_index) != 1 {
        bail!("subtitle_merge_not_adjacent: 只允许合并时间顺序相邻的字幕段")
    }
    let (left, right) = if first_index < second_index {
        (&segments[first_index], &segments[second_index])
    } else {
        (&segments[second_index], &segments[first_index])
    };
    let text = format!(
        "{}{}{}",
        left.text.trim_end(),
        separator,
        right.text.trim_start()
    );
    project::assert_segment(left.start, right.end, &text)?;
    let confidence = match (left.confidence, right.confidence) {
        (Some(left_value), Some(right_value)) => {
            let left_duration = left.end - left.start;
            let right_duration = right.end - right.start;
            Some(
                (left_value * left_duration + right_value * right_duration)
                    / (left_duration + right_duration),
            )
        }
        _ => None,
    };
    let mut impact = StructureImpact::default();
    let tx = db.transaction()?;
    invalidate_translations(&tx, project_id, &[&left.id, &right.id], &mut impact)?;
    invalidate_edits(&tx, project_id, &[&left.id, &right.id], &mut impact)?;
    impact.words_reassigned += tx.execute(
        "UPDATE words SET segment_id=?3 WHERE project_id=?1 AND segment_id=?2",
        params![project_id, &right.id, &left.id],
    )?;
    impact.agent_patch_items_rebased += tx.execute(
        "UPDATE agent_patch_items SET segment_id=?3 WHERE segment_id=?2 AND patch_set_id IN (
             SELECT id FROM agent_patch_sets WHERE project_id=?1
         )",
        params![project_id, &right.id, &left.id],
    )?;
    tx.execute(
        "UPDATE edits SET segment_id=?3 WHERE project_id=?1 AND segment_id=?2",
        params![project_id, &right.id, &left.id],
    )?;
    merge_speaker_associations(&tx, project_id, &left.id, &right.id, &mut impact)?;
    tx.execute(
        "UPDATE segments SET end_seconds=?3,text=?4,confidence=?5 WHERE project_id=?1 AND id=?2",
        params![project_id, &left.id, right.end, text, confidence],
    )?;
    tx.execute(
        "DELETE FROM segments WHERE project_id=?1 AND id=?2",
        params![project_id, &right.id],
    )?;
    tx.commit()?;
    finish(
        db,
        project_id,
        "合并相邻字幕段",
        StructureEditChange {
            operation: "merge",
            affected_segment_ids: vec![left.id.clone()],
            created_segment_id: None,
            removed_segment_ids: vec![right.id.clone()],
            impact,
        },
    )
}

pub fn adjust_timing(
    db: &mut Connection,
    project_id: &str,
    segment_id: &str,
    start: f64,
    end: f64,
) -> Result<StructureEditResult> {
    let loaded = project::load(db, project_id)?;
    let segment = segment_by_id(&loaded.transcript.segments, segment_id)?;
    project::assert_segment(start, end, &segment.text)
        .map_err(|_| anyhow!("subtitle_time_invalid: 字幕时间必须有限、非负且结束晚于开始"))?;
    assert_media_bound(&loaded, end)?;
    if (segment.start - start).abs() <= TIME_EPSILON && (segment.end - end).abs() <= TIME_EPSILON {
        bail!("subtitle_timing_unchanged: 字幕时间没有变化")
    }
    let remove_words = loaded.transcript.words.iter().any(|word| {
        word.segment_id == segment_id
            && (word.start < start - TIME_EPSILON || word.end > end + TIME_EPSILON)
    });
    let mut impact = StructureImpact::default();
    let tx = db.transaction()?;
    invalidate_edits(&tx, project_id, &[segment_id], &mut impact)?;
    if remove_words {
        impact.words_removed += tx.execute(
            "DELETE FROM words WHERE project_id=?1 AND segment_id=?2",
            params![project_id, segment_id],
        )?;
    }
    remove_automatic_speaker_association(&tx, project_id, segment_id, &mut impact)?;
    tx.execute(
        "UPDATE segments SET start_seconds=?3,end_seconds=?4 WHERE project_id=?1 AND id=?2",
        params![project_id, segment_id, start, end],
    )?;
    tx.commit()?;
    finish(
        db,
        project_id,
        "调整字幕时间",
        StructureEditChange {
            operation: "timing",
            affected_segment_ids: vec![segment_id.to_owned()],
            created_segment_id: None,
            removed_segment_ids: Vec::new(),
            impact,
        },
    )
}

pub fn offset(
    db: &mut Connection,
    project_id: &str,
    segment_ids: &[String],
    delta_seconds: f64,
) -> Result<StructureEditResult> {
    if !delta_seconds.is_finite() || delta_seconds.abs() <= TIME_EPSILON {
        bail!("subtitle_offset_invalid: 批量偏移必须是非零有限秒数")
    }
    let requested = segment_ids.iter().cloned().collect::<BTreeSet<_>>();
    if requested.is_empty() {
        bail!("subtitle_offset_empty: 批量偏移至少需要一个字幕段")
    }
    let loaded = project::load(db, project_id)?;
    let by_id = loaded
        .transcript
        .segments
        .iter()
        .map(|segment| (segment.id.clone(), segment))
        .collect::<HashMap<_, _>>();
    for segment_id in &requested {
        let segment = by_id
            .get(segment_id)
            .ok_or_else(|| anyhow!("subtitle_segment_not_found: 字幕段不存在：{segment_id}"))?;
        let start = segment.start + delta_seconds;
        let end = segment.end + delta_seconds;
        project::assert_segment(start, end, &segment.text)
            .map_err(|_| anyhow!("subtitle_offset_out_of_bounds: 偏移后字幕时间无效"))?;
        assert_media_bound(&loaded, end)
            .map_err(|_| anyhow!("subtitle_offset_out_of_bounds: 偏移后字幕超过原片范围"))?;
    }
    for word in &loaded.transcript.words {
        let shifted_start = word.start + delta_seconds;
        let shifted_end = word.end + delta_seconds;
        if requested.contains(&word.segment_id)
            && (!shifted_start.is_finite()
                || !shifted_end.is_finite()
                || shifted_start < 0.0
                || shifted_end <= shifted_start)
        {
            bail!("subtitle_offset_word_out_of_bounds: 偏移后词级时间无效")
        }
    }
    let ordered_ids = loaded
        .transcript
        .segments
        .iter()
        .filter(|segment| requested.contains(&segment.id))
        .map(|segment| segment.id.clone())
        .collect::<Vec<_>>();
    let mut impact = StructureImpact::default();
    let tx = db.transaction()?;
    for segment_id in &ordered_ids {
        invalidate_edits(&tx, project_id, &[segment_id], &mut impact)?;
        remove_automatic_speaker_association(&tx, project_id, segment_id, &mut impact)?;
        tx.execute(
            "UPDATE segments SET start_seconds=start_seconds+?3,end_seconds=end_seconds+?3 WHERE project_id=?1 AND id=?2",
            params![project_id, segment_id, delta_seconds],
        )?;
        impact.words_shifted += tx.execute(
            "UPDATE words SET start_seconds=start_seconds+?3,end_seconds=end_seconds+?3 WHERE project_id=?1 AND segment_id=?2",
            params![project_id, segment_id, delta_seconds],
        )?;
    }
    tx.commit()?;
    finish(
        db,
        project_id,
        &format!("批量偏移字幕 {delta_seconds:+.3} 秒"),
        StructureEditChange {
            operation: "offset",
            affected_segment_ids: ordered_ids,
            created_segment_id: None,
            removed_segment_ids: Vec::new(),
            impact,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::{fs, path::PathBuf};
    use tempfile::{TempDir, tempdir};

    struct Fixture {
        _temp: TempDir,
        db: Connection,
        project_id: String,
        media: PathBuf,
        exported: PathBuf,
    }

    fn fixture() -> Fixture {
        let temp = tempdir().unwrap();
        let media = temp.path().join("source.wav");
        let exported = temp.path().join("existing-export.mp4");
        fs::write(&media, b"immutable-source").unwrap();
        fs::write(&exported, b"immutable-export").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("workbench.db")).unwrap();
        let created = project::create(&mut db, &media, Some("Workbench".into())).unwrap();
        let project_id = created.id;
        let base_version = project::current_version_id(&db, &project_id)
            .unwrap()
            .unwrap();
        db.execute(
            "UPDATE media SET duration_seconds=10 WHERE project_id=?1",
            [&project_id],
        )
        .unwrap();
        for (id, start, end, text) in [
            ("s1", 0.0, 2.0, "hello world"),
            ("s2", 2.0, 4.0, "second"),
            ("s3", 4.0, 6.0, "third"),
        ] {
            db.execute(
                "INSERT INTO segments(id,project_id,start_seconds,end_seconds,text,confidence) VALUES(?1,?2,?3,?4,?5,0.8)",
                params![id, &project_id, start, end, text],
            )
            .unwrap();
        }
        for (ordinal, (id, segment_id, start, end, text)) in [
            ("w1", "s1", 0.2, 0.8, "hello"),
            ("w2", "s1", 1.2, 1.8, "world"),
            ("w3", "s2", 2.2, 2.8, "second"),
        ]
        .into_iter()
        .enumerate()
        {
            db.execute(
                "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,confidence,ordinal) VALUES(?1,?2,?3,?4,?5,?6,0.9,?7)",
                params![id, &project_id, segment_id, start, end, text, ordinal as i64],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO translations(project_id,language,status,updated_at) VALUES(?1,'en','current','fixture')",
            [&project_id],
        )
        .unwrap();
        for (segment_id, text) in [("s1", "one"), ("s2", "two"), ("s3", "three")] {
            db.execute(
                "INSERT INTO translation_segments(project_id,language,segment_id,text) VALUES(?1,'en',?2,?3)",
                params![&project_id, segment_id, text],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO edits(id,project_id,kind,status,segment_id,start_seconds,end_seconds,reason,created_at) VALUES('e1',?1,'word_cut','applied','s1',0.1,0.9,'fixture','fixture')",
            [&project_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO word_range_cuts(edit_id,from_word_id,to_word_id,selected_start_seconds,selected_end_seconds,padding_ms,transcript_hash,stale) VALUES('e1','w1','w1',0.2,0.8,100,'fixture',0)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO speaker_tracks(project_id,status,runtime_version,segmentation_model,embedding_model,generated_at) VALUES(?1,'ready','test','test','test','fixture')",
            [&project_id],
        )
        .unwrap();
        for (id, label, color) in [("sp1", "A", 0), ("sp2", "B", 1)] {
            db.execute(
                "INSERT INTO speakers(id,project_id,source_label,label,color_index,created_at) VALUES(?1,?2,?3,?3,?4,'fixture')",
                params![id, &project_id, label, color],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO segment_speakers(project_id,segment_id,speaker_id,source,confidence,updated_at) VALUES(?1,'s1','sp1','overlap',0.8,'fixture'),(?1,'s2','sp2','manual',NULL,'fixture')",
            [&project_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO tasks(id,project_id,kind,status,created_at,base_version_id,progress,attempt_count) VALUES('t1',?1,'polish','review','fixture',?2,1,1)",
            params![&project_id, &base_version],
        )
        .unwrap();
        db.execute(
            "INSERT INTO agent_patch_sets(id,task_id,project_id,kind,status,base_version_id,created_at) VALUES('p1','t1',?1,'polish','pending_review',?2,'fixture')",
            params![&project_id, &base_version],
        )
        .unwrap();
        db.execute(
            "INSERT INTO agent_patch_items(id,patch_set_id,segment_id,target,before_text,after_text,current_text_at_submit,reason,status,ordinal) VALUES('pi1','p1','s2','source','second','second revised','second','fixture','pending',0)",
            [],
        )
        .unwrap();
        project::snapshot(&db, &project_id, "fixture baseline").unwrap();
        Fixture {
            _temp: temp,
            db,
            project_id,
            media,
            exported,
        }
    }

    fn association(
        db: &Connection,
        project_id: &str,
        segment_id: &str,
    ) -> Option<(String, String)> {
        db.query_row(
            "SELECT speaker_id,source FROM segment_speakers WHERE project_id=?1 AND segment_id=?2",
            params![project_id, segment_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .unwrap()
    }

    fn assert_files_unchanged(fixture: &Fixture) {
        assert_eq!(fs::read(&fixture.media).unwrap(), b"immutable-source");
        assert_eq!(fs::read(&fixture.exported).unwrap(), b"immutable-export");
    }

    #[test]
    fn split_invalidates_dependencies_and_round_trips_history() {
        let mut fixture = fixture();
        let result = split(&mut fixture.db, &fixture.project_id, "s1", 5, 1.0).unwrap();
        let right_id = result.created_segment_id.clone().unwrap();
        assert_eq!(result.operation, "split");
        assert_eq!(result.impact.words_reassigned, 1);
        assert_eq!(result.impact.translation_segments_removed, 1);
        assert_eq!(result.impact.edits_restored, 1);
        assert_eq!(result.impact.word_cuts_invalidated, 1);
        let split_project = project::load(&fixture.db, &fixture.project_id).unwrap();
        assert_eq!(split_project.transcript.segments[0].text, "hello");
        assert_eq!(split_project.transcript.segments[1].id, right_id);
        assert_eq!(split_project.transcript.words[1].segment_id, right_id);
        assert_eq!(split_project.translations["en"].status, "stale");
        assert_eq!(split_project.translations["en"].segments.len(), 2);
        assert_eq!(split_project.edits[0].status, "restored");
        assert!(split_project.edits[0].cut_range.is_none());
        assert_eq!(
            association(&fixture.db, &fixture.project_id, &right_id)
                .unwrap()
                .0,
            "sp1"
        );
        assert_files_unchanged(&fixture);

        let undone = project::undo(&mut fixture.db, &fixture.project_id).unwrap();
        assert_eq!(undone.transcript.segments[0].text, "hello world");
        assert_eq!(undone.transcript.segments.len(), 3);
        assert_eq!(undone.translations["en"].status, "current");
        assert_eq!(undone.translations["en"].segments.len(), 3);
        assert_eq!(undone.edits[0].status, "applied");
        assert!(undone.edits[0].cut_range.is_some());
        let redone = project::redo(&mut fixture.db, &fixture.project_id).unwrap();
        assert_eq!(redone.transcript.segments.len(), 4);
        assert_eq!(redone.transcript.segments[1].id, right_id);
        assert_files_unchanged(&fixture);
    }

    #[test]
    fn merge_rebases_references_and_round_trips_history() {
        let mut fixture = fixture();
        let result = merge(&mut fixture.db, &fixture.project_id, "s2", "s1", " ").unwrap();
        assert_eq!(result.removed_segment_ids, ["s2"]);
        assert_eq!(result.impact.words_reassigned, 1);
        assert_eq!(result.impact.agent_patch_items_rebased, 1);
        let merged = project::load(&fixture.db, &fixture.project_id).unwrap();
        assert_eq!(merged.transcript.segments.len(), 2);
        assert_eq!(merged.transcript.segments[0].id, "s1");
        assert_eq!(merged.transcript.segments[0].text, "hello world second");
        assert_eq!(
            merged.patch_sets[0].items[0].segment_id.as_deref(),
            Some("s1")
        );
        assert_eq!(
            association(&fixture.db, &fixture.project_id, "s1"),
            Some(("sp2".into(), "manual".into()))
        );

        let undone = project::undo(&mut fixture.db, &fixture.project_id).unwrap();
        assert_eq!(undone.transcript.segments.len(), 3);
        assert_eq!(
            undone.patch_sets[0].items[0].segment_id.as_deref(),
            Some("s2")
        );
        assert_eq!(
            association(&fixture.db, &fixture.project_id, "s1")
                .unwrap()
                .0,
            "sp1"
        );
        assert_eq!(
            association(&fixture.db, &fixture.project_id, "s2")
                .unwrap()
                .0,
            "sp2"
        );
        let redone = project::redo(&mut fixture.db, &fixture.project_id).unwrap();
        assert_eq!(redone.transcript.segments.len(), 2);
        assert_eq!(
            redone.patch_sets[0].items[0].segment_id.as_deref(),
            Some("s1")
        );
        assert_files_unchanged(&fixture);
    }

    #[test]
    fn timing_and_offset_invalidate_only_unsafe_evidence() {
        let mut timing_fixture = fixture();
        let timed = adjust_timing(
            &mut timing_fixture.db,
            &timing_fixture.project_id,
            "s1",
            0.3,
            1.5,
        )
        .unwrap();
        assert_eq!(timed.impact.words_removed, 2);
        assert_eq!(timed.project.translations["en"].status, "current");
        assert!(association(&timing_fixture.db, &timing_fixture.project_id, "s1").is_none());
        assert_eq!(timed.project.edits[0].status, "restored");
        let undone = project::undo(&mut timing_fixture.db, &timing_fixture.project_id).unwrap();
        assert_eq!(undone.transcript.words.len(), 3);
        let redone = project::redo(&mut timing_fixture.db, &timing_fixture.project_id).unwrap();
        assert_eq!(redone.transcript.words.len(), 1);

        let mut offset_fixture = fixture();
        let shifted = offset(
            &mut offset_fixture.db,
            &offset_fixture.project_id,
            &["s1".into(), "s2".into(), "s2".into()],
            1.0,
        )
        .unwrap();
        assert_eq!(shifted.affected_segment_ids, ["s1", "s2"]);
        assert_eq!(shifted.impact.words_shifted, 3);
        assert_eq!(shifted.project.transcript.segments[0].start, 1.0);
        assert_eq!(shifted.project.transcript.words[0].start, 1.2);
        assert_eq!(shifted.project.translations["en"].status, "current");
        assert!(association(&offset_fixture.db, &offset_fixture.project_id, "s1").is_none());
        assert_eq!(
            association(&offset_fixture.db, &offset_fixture.project_id, "s2")
                .unwrap()
                .1,
            "manual"
        );
        assert_eq!(shifted.project.edits[0].status, "restored");
        let undone = project::undo(&mut offset_fixture.db, &offset_fixture.project_id).unwrap();
        assert_eq!(undone.transcript.segments[0].start, 0.0);
        let redone = project::redo(&mut offset_fixture.db, &offset_fixture.project_id).unwrap();
        assert_eq!(redone.transcript.segments[0].start, 1.0);
        assert_files_unchanged(&offset_fixture);
    }

    #[test]
    fn invalid_structure_requests_are_atomic() {
        let mut fixture = fixture();
        let baseline = project::load(&fixture.db, &fixture.project_id).unwrap();
        let version = baseline.history.current_version_id.clone();
        let error = split(&mut fixture.db, &fixture.project_id, "s1", 5, 0.5).unwrap_err();
        assert!(error.to_string().contains("subtitle_split_crosses_word"));
        let error = merge(&mut fixture.db, &fixture.project_id, "s1", "s3", " ").unwrap_err();
        assert!(error.to_string().contains("subtitle_merge_not_adjacent"));
        let error = offset(&mut fixture.db, &fixture.project_id, &["s1".into()], -1.0).unwrap_err();
        assert!(error.to_string().contains("subtitle_offset_out_of_bounds"));
        assert!(adjust_timing(&mut fixture.db, &fixture.project_id, "s1", 2.0, 1.0).is_err());
        let current = project::load(&fixture.db, &fixture.project_id).unwrap();
        assert_eq!(current.transcript, baseline.transcript);
        assert_eq!(current.translations, baseline.translations);
        assert_eq!(current.edits, baseline.edits);
        assert_eq!(current.history.current_version_id, version);
        assert_files_unchanged(&fixture);
    }
}
