use crate::{
    model::{CutRange, CutSuggestion, Edit, Project, Word},
    project, speech,
    util::{new_id, now},
};
use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const DETECTOR_VERSION: &str = "heuristic-v1";

#[derive(Debug, Clone, PartialEq)]
struct SuggestedRange {
    from_index: usize,
    to_index: usize,
    suggestion_type: &'static str,
    confidence: f64,
}

pub fn detect(db: &mut Connection, project_id: &str) -> Result<Vec<Edit>> {
    let project = project::load(db, project_id)?;
    let existing_full: HashSet<String> = project
        .edits
        .iter()
        .filter(|edit| edit.kind == "cut")
        .map(|edit| edit.segment_id.clone())
        .collect();
    let mut existing_ranges = project
        .edits
        .iter()
        .filter_map(|edit| edit.cut_range.as_ref())
        .map(|range| (range.from_word_id.clone(), range.to_word_id.clone()))
        .collect::<HashSet<_>>();
    let mut suggestions = Vec::new();
    for segment in &project.transcript.segments {
        let words = project
            .transcript
            .words
            .iter()
            .filter(|word| word.segment_id == segment.id)
            .collect::<Vec<_>>();
        if !words.is_empty() {
            let evidence = words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<String>();
            if normalized_evidence(&evidence) != normalized_evidence(&segment.text) {
                continue;
            }
            for candidate in detect_word_ranges(&words) {
                let from = words[candidate.from_index];
                let to = words[candidate.to_index];
                if !existing_ranges.insert((from.id.clone(), to.id.clone())) {
                    continue;
                }
                let selected_text =
                    display_words(&words[candidate.from_index..=candidate.to_index]);
                let reason = match candidate.suggestion_type {
                    "standalone_filler" => format!("句内口头语：{selected_text}"),
                    "adjacent_repetition" => format!("3 秒内相邻重复：{selected_text}"),
                    "speech_restart" => format!("说话重启：{selected_text}"),
                    _ => unreachable!("known suggestion type"),
                };
                suggestions.push(build_word_range_edit(
                    &project,
                    &segment.id,
                    &from.id,
                    &to.id,
                    100,
                    Some(reason),
                    Some(CutSuggestion {
                        suggestion_type: candidate.suggestion_type.into(),
                        confidence: candidate.confidence,
                        detector_version: DETECTOR_VERSION.into(),
                    }),
                )?);
            }
            continue;
        }
        let normalized = segment
            .text
            .trim()
            .trim_matches(|character: char| "，。,.!?！？\"“” ".contains(character))
            .to_lowercase();
        if ["嗯", "呃", "额", "啊", "uh", "um", "erm"].contains(&normalized.as_str())
            && !existing_full.contains(&segment.id)
        {
            let edit = Edit {
                id: new_id("cut"),
                kind: "cut".into(),
                status: "proposed".into(),
                segment_id: segment.id.clone(),
                start: segment.start,
                end: segment.end,
                reason: "疑似口癖".into(),
                created_at: now(),
                cut_range: None,
                suggestion: None,
            };
            suggestions.push(edit);
        }
    }
    if suggestions.is_empty() {
        return Ok(suggestions);
    }
    let tx = db.transaction()?;
    for edit in &suggestions {
        if edit.kind == "word_cut" {
            insert_word_range_edit(&tx, project_id, edit)?;
        } else {
            tx.execute("INSERT INTO edits(id,project_id,kind,status,segment_id,start_seconds,end_seconds,reason,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![&edit.id,project_id,&edit.kind,&edit.status,&edit.segment_id,edit.start,edit.end,&edit.reason,&edit.created_at])?;
        }
    }
    tx.commit()?;
    project::snapshot(db, project_id, "检测粗剪建议")?;
    Ok(suggestions)
}

fn normalized_evidence(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn transcript_hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn display_words(words: &[&Word]) -> String {
    let mut text = String::new();
    for word in words {
        let token = word.text.as_str();
        if text
            .chars()
            .last()
            .is_some_and(|left| left.is_ascii_alphanumeric())
            && token
                .chars()
                .next()
                .is_some_and(|right| right.is_ascii_alphanumeric())
        {
            text.push(' ');
        }
        text.push_str(token);
    }
    text.trim().to_owned()
}

fn detect_word_ranges(words: &[&Word]) -> Vec<SuggestedRange> {
    let tokens = words
        .iter()
        .map(|word| normalized_evidence(&word.text))
        .collect::<Vec<_>>();
    let intentional_repeats = [
        "非常", "很", "再", "慢慢", "渐渐", "常常", "往往", "人人", "次次", "天天", "谢谢", "拜拜",
        "一点", "好", "very", "bye", "no", "yes",
    ];
    let mut suggestions = Vec::new();
    let mut occupied = HashSet::new();
    let mut index = 0;
    while index < words.len() {
        let max_length = 4.min((words.len() - index) / 2);
        let matched = (1..=max_length).rev().find(|length| {
            let second = index + length;
            let first_tokens = &tokens[index..second];
            let second_tokens = &tokens[second..second + length];
            !first_tokens.iter().any(String::is_empty)
                && first_tokens == second_tokens
                && words[second].start - words[second - 1].end <= 3.0
                && !words[index..second].iter().any(|word| {
                    word.text
                        .trim_end()
                        .ends_with(['。', '！', '？', '.', '!', '?'])
                })
                && (*length > 1 || !intentional_repeats.contains(&first_tokens[0].as_str()))
        });
        if let Some(length) = matched {
            let has_continuation = index + length * 2 < words.len();
            let suggestion_type = if has_continuation {
                "speech_restart"
            } else {
                "adjacent_repetition"
            };
            suggestions.push(SuggestedRange {
                from_index: index,
                to_index: index + length - 1,
                suggestion_type,
                confidence: if suggestion_type == "speech_restart" {
                    if length > 1 { 0.98 } else { 0.96 }
                } else if length > 1 {
                    0.99
                } else {
                    0.97
                },
            });
            occupied.extend(index..index + length);
            index += length * 2;
        } else {
            index += 1;
        }
    }

    for (index, token) in tokens.iter().enumerate() {
        if occupied.contains(&index) || !speech::is_filler_token(token) {
            continue;
        }
        if token == "uh" && tokens.get(index + 1).is_some_and(|next| next == "oh") {
            continue;
        }
        suggestions.push(SuggestedRange {
            from_index: index,
            to_index: index,
            suggestion_type: "standalone_filler",
            confidence: 0.99,
        });
    }
    suggestions.sort_by_key(|suggestion| (suggestion.from_index, suggestion.to_index));
    suggestions
}

fn build_word_range_edit(
    project: &Project,
    segment_id: &str,
    from_word_id: &str,
    to_word_id: &str,
    padding_ms: u32,
    reason: Option<String>,
    suggestion: Option<CutSuggestion>,
) -> Result<Edit> {
    if ![30, 100, 200].contains(&padding_ms) {
        bail!("word_range_invalid: 安全留白只支持 30、100 或 200 毫秒")
    }
    let segment = project
        .transcript
        .segments
        .iter()
        .find(|segment| segment.id == segment_id)
        .ok_or_else(|| anyhow!("word_range_invalid: 字幕段不存在：{segment_id}"))?;
    let segment_words = project
        .transcript
        .words
        .iter()
        .filter(|word| word.segment_id == segment_id)
        .collect::<Vec<_>>();
    if segment_words.is_empty() {
        bail!("word_range_invalid: 字幕段没有词级时间证据")
    }
    let from_index = segment_words
        .iter()
        .position(|word| word.id == from_word_id)
        .ok_or_else(|| anyhow!("word_range_invalid: 起始词不属于指定字幕段"))?;
    let to_index = segment_words
        .iter()
        .position(|word| word.id == to_word_id)
        .ok_or_else(|| anyhow!("word_range_invalid: 结束词不属于指定字幕段"))?;
    if from_index > to_index {
        bail!("word_range_invalid: 起始词必须位于结束词之前")
    }
    let evidence = segment_words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<String>();
    if normalized_evidence(&evidence) != normalized_evidence(&segment.text) {
        bail!("word_alignment_stale: 字幕文本与词级时间证据不一致，无法可靠创建范围剪辑")
    }
    let selected_start = segment_words[from_index].start;
    let selected_end = segment_words[to_index].end;
    let all_words = &project.transcript.words;
    let global_from = all_words
        .iter()
        .position(|word| word.id == from_word_id)
        .ok_or_else(|| anyhow!("word_range_invalid: 找不到起始词"))?;
    let global_to = all_words
        .iter()
        .position(|word| word.id == to_word_id)
        .ok_or_else(|| anyhow!("word_range_invalid: 找不到结束词"))?;
    let previous_end = global_from
        .checked_sub(1)
        .and_then(|index| all_words.get(index))
        .map(|word| word.end)
        .unwrap_or(0.0);
    let media_end = project.media.duration_seconds.unwrap_or_else(|| {
        project
            .transcript
            .segments
            .iter()
            .map(|item| item.end)
            .fold(0.0, f64::max)
    });
    let next_start = all_words
        .get(global_to + 1)
        .map(|word| word.start)
        .unwrap_or(media_end);
    let padding = padding_ms as f64 / 1000.0;
    let actual_start = (selected_start - padding).max(previous_end).max(0.0);
    let actual_end = (selected_end + padding).min(next_start).min(media_end);
    if actual_end <= actual_start {
        bail!("word_range_invalid: 计算后的剪辑范围为空")
    }
    let selected_text = display_words(&segment_words[from_index..=to_index]);
    Ok(Edit {
        id: new_id("cut"),
        kind: "word_cut".into(),
        status: "proposed".into(),
        segment_id: segment_id.to_owned(),
        start: actual_start,
        end: actual_end,
        reason: reason.unwrap_or_else(|| format!("词范围：{selected_text}")),
        created_at: now(),
        cut_range: Some(CutRange {
            from_word_id: from_word_id.to_owned(),
            to_word_id: to_word_id.to_owned(),
            selected_start,
            selected_end,
            padding_ms,
            transcript_hash: transcript_hash(&segment.text),
            stale: false,
        }),
        suggestion,
    })
}

fn insert_word_range_edit(db: &Connection, project_id: &str, edit: &Edit) -> Result<()> {
    db.execute(
        "INSERT INTO edits(id,project_id,kind,status,segment_id,start_seconds,end_seconds,reason,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![&edit.id,project_id,&edit.kind,&edit.status,&edit.segment_id,edit.start,edit.end,&edit.reason,&edit.created_at],
    )?;
    let range = edit.cut_range.as_ref().expect("word cut has range");
    db.execute(
        "INSERT INTO word_range_cuts(edit_id,from_word_id,to_word_id,selected_start_seconds,selected_end_seconds,padding_ms,transcript_hash,stale) VALUES(?1,?2,?3,?4,?5,?6,?7,0)",
        params![&edit.id,&range.from_word_id,&range.to_word_id,range.selected_start,range.selected_end,range.padding_ms,&range.transcript_hash],
    )?;
    if let Some(suggestion) = &edit.suggestion {
        db.execute(
            "INSERT INTO cut_suggestions(edit_id,suggestion_type,confidence,detector_version) VALUES(?1,?2,?3,?4)",
            params![&edit.id,&suggestion.suggestion_type,suggestion.confidence,&suggestion.detector_version],
        )?;
    }
    Ok(())
}

pub fn create_word_range(
    db: &mut Connection,
    project_id: &str,
    segment_id: &str,
    from_word_id: &str,
    to_word_id: &str,
    padding_ms: u32,
) -> Result<Edit> {
    let project = project::load(db, project_id)?;
    let duplicate: bool = db.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM word_range_cuts w JOIN edits e ON e.id=w.edit_id
            WHERE e.project_id=?1 AND w.from_word_id=?2 AND w.to_word_id=?3 AND e.status!='restored'
         )",
        params![project_id, from_word_id, to_word_id],
        |row| row.get(0),
    )?;
    if duplicate {
        bail!("word_range_invalid: 相同词范围已经存在待审或已应用剪辑")
    }
    let edit = build_word_range_edit(
        &project,
        segment_id,
        from_word_id,
        to_word_id,
        padding_ms,
        None,
        None,
    )?;
    let tx = db.transaction()?;
    insert_word_range_edit(&tx, project_id, &edit)?;
    tx.commit()?;
    project::snapshot(db, project_id, "创建词范围剪辑")?;
    Ok(edit)
}

pub fn preview(db: &Connection, project_id: &str, cut_id: &str) -> Result<Value> {
    let project = project::load(db, project_id)?;
    let edit = project
        .edits
        .iter()
        .find(|edit| edit.id == cut_id && edit.kind == "word_cut")
        .ok_or_else(|| anyhow!("软剪辑不存在：{cut_id}"))?;
    let duration = project.timeline.source_duration;
    Ok(json!({
        "cutId": edit.id,
        "previewStart": (edit.start - 1.0).max(0.0),
        "cutStart": edit.start,
        "cutEnd": edit.end,
        "previewEnd": (edit.end + 1.0).min(duration),
        "skipRange": true
    }))
}

pub fn set_status(
    db: &mut Connection,
    project_id: &str,
    cut_id: &str,
    status: &str,
) -> Result<Edit> {
    if !["applied", "restored"].contains(&status) {
        bail!("软剪辑状态无效：{status}")
    }
    let edit = project::load(db, project_id)?
        .edits
        .into_iter()
        .find(|edit| {
            edit.id == cut_id && matches!(edit.kind.as_str(), "cut" | "word_cut" | "semantic_cut")
        })
        .ok_or_else(|| anyhow!("软剪辑不存在：{cut_id}"))?;
    if status == "applied" && edit.cut_range.as_ref().is_some_and(|range| range.stale) {
        bail!("word_alignment_stale: 字幕文本已经变化，请重新创建词范围剪辑")
    }
    let changed = db.execute(
        "UPDATE edits SET status=?3 WHERE id=?1 AND project_id=?2 AND kind IN ('cut','word_cut','semantic_cut')",
        params![cut_id, project_id, status],
    )?;
    if changed == 0 {
        bail!("软剪辑不存在：{cut_id}")
    }
    if status == "applied" && edit.kind == "word_cut" {
        db.execute(
            "UPDATE translations SET status='stale' WHERE project_id=?1",
            [project_id],
        )?;
    }
    project::snapshot(
        db,
        project_id,
        if status == "applied" {
            "应用软剪辑"
        } else {
            "恢复软剪辑"
        },
    )?;
    project::load(db, project_id)?
        .edits
        .into_iter()
        .find(|edit| edit.id == cut_id)
        .ok_or_else(|| anyhow!("软剪辑不存在：{cut_id}"))
}

pub fn restore_all(db: &mut Connection, project_id: &str) -> Result<usize> {
    let count = db.execute(
        "UPDATE edits SET status='restored' WHERE project_id=?1 AND kind IN ('cut','word_cut','semantic_cut') AND status='applied'",
        [project_id],
    )?;
    project::snapshot(db, project_id, "恢复全部软剪辑")?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, project};
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn cut_suggestion_requires_explicit_apply_and_can_restore() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("core.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut db, &media, None).unwrap();
        project::add_segment(&mut db, &project.id, 0.0, 0.5, "嗯".into(), None).unwrap();
        let cuts = detect(&mut db, &project.id).unwrap();
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].status, "proposed");
        assert_eq!(
            set_status(&mut db, &project.id, &cuts[0].id, "applied")
                .unwrap()
                .status,
            "applied"
        );
        assert_eq!(restore_all(&mut db, &project.id).unwrap(), 1);
    }

    #[test]
    fn word_range_cut_uses_word_boundaries_padding_and_stale_protection() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("word-cut.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut db, &media, None).unwrap();
        let segment = project::add_segment(
            &mut db,
            &project.id,
            0.2,
            2.7,
            "hello brave world".into(),
            None,
        )
        .unwrap();
        for (id, start, end, text, ordinal) in [
            ("w1", 0.2, 0.7, "hello", 0),
            ("w2", 1.0, 1.5, "brave", 1),
            ("w3", 2.0, 2.7, "world", 2),
        ] {
            db.execute(
                "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![id,&project.id,&segment.id,start,end,text,ordinal],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO translations(project_id,language,status,updated_at) VALUES(?1,'zh','current','now')",
            [&project.id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO translation_segments(project_id,language,segment_id,text) VALUES(?1,'zh',?2,'你好勇敢世界')",
            params![&project.id, &segment.id],
        )
        .unwrap();

        let cut = create_word_range(&mut db, &project.id, &segment.id, "w2", "w2", 200).unwrap();
        let cut_version_id = project::load(&db, &project.id)
            .unwrap()
            .versions
            .last()
            .unwrap()
            .id
            .clone();
        assert_eq!(cut.status, "proposed");
        assert_eq!((cut.start, cut.end), (0.8, 1.7));
        assert_eq!(cut.cut_range.as_ref().unwrap().padding_ms, 200);
        let window = preview(&db, &project.id, &cut.id).unwrap();
        assert_eq!(window["previewStart"], 0.0);
        assert_eq!(window["previewEnd"], 2.7);
        assert_eq!(
            set_status(&mut db, &project.id, &cut.id, "applied")
                .unwrap()
                .status,
            "applied"
        );
        assert!(
            (project::load(&db, &project.id)
                .unwrap()
                .timeline
                .output_duration
                - 1.8)
                .abs()
                < 0.001
        );
        assert_eq!(
            project::load(&db, &project.id).unwrap().translations["zh"].status,
            "stale"
        );

        project::edit_segment(&mut db, &project.id, &segment.id, "hello bold world".into())
            .unwrap();
        let stale = project::load(&db, &project.id)
            .unwrap()
            .edits
            .into_iter()
            .find(|edit| edit.id == cut.id)
            .unwrap();
        assert_eq!(stale.status, "restored");
        assert!(stale.cut_range.unwrap().stale);
        assert!(set_status(&mut db, &project.id, &cut.id, "restored").is_ok());
        assert!(
            set_status(&mut db, &project.id, &cut.id, "applied")
                .unwrap_err()
                .to_string()
                .contains("word_alignment_stale")
        );
        project::restore_version(&mut db, &project.id, &cut_version_id).unwrap();
        let restored = project::load(&db, &project.id)
            .unwrap()
            .edits
            .into_iter()
            .find(|edit| edit.id == cut.id)
            .unwrap();
        assert_eq!(restored.status, "proposed");
        assert!(!restored.cut_range.unwrap().stale);
    }

    #[test]
    fn cut_suggestions_detect_fillers_repetitions_and_restarts_without_auto_apply() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("suggestions.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut db, &media, None).unwrap();
        let mut cursor = 0.0;
        let cases = [
            ("zh-filler", vec!["今天", "嗯", "我们", "开始"]),
            ("en-filler", vec!["we", "um", "can", "start"]),
            (
                "restart",
                vec!["我们", "需要", "我们", "需要", "现在", "开始"],
            ),
            ("repeat", vec!["重点", "是", "安全", "重点", "是", "安全"]),
            ("single-restart", vec!["我", "我", "觉得", "可以"]),
            ("intentional", vec!["非常", "非常", "重要"]),
            ("uh-oh", vec!["uh", "oh", "this", "is", "bad"]),
            ("demonstrative", vec!["这个", "方案", "可以"]),
        ];
        for (case_id, tokens) in cases {
            let start = cursor;
            let end = start + tokens.len() as f64 * 0.4;
            let segment =
                project::add_segment(&mut db, &project.id, start, end, tokens.join(" "), None)
                    .unwrap();
            for (ordinal, token) in tokens.iter().enumerate() {
                let word_start = start + ordinal as f64 * 0.4;
                db.execute(
                    "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![format!("{case_id}-{ordinal}"),&project.id,&segment.id,word_start,word_start+0.25,*token,ordinal as i64],
                )
                .unwrap();
            }
            cursor = end + 0.5;
        }

        let versions_before = project::load(&db, &project.id).unwrap().versions.len();
        let suggestions = detect(&mut db, &project.id).unwrap();
        assert_eq!(suggestions.len(), 5);
        assert!(suggestions.iter().all(|edit| edit.status == "proposed"));
        assert!(suggestions.iter().all(|edit| edit.kind == "word_cut"));
        let types = suggestions
            .iter()
            .map(|edit| edit.suggestion.as_ref().unwrap().suggestion_type.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            types,
            vec![
                "standalone_filler",
                "standalone_filler",
                "speech_restart",
                "adjacent_repetition",
                "speech_restart"
            ]
        );
        assert!(suggestions.iter().all(|edit| {
            let suggestion = edit.suggestion.as_ref().unwrap();
            suggestion.confidence >= 0.96 && suggestion.detector_version == DETECTOR_VERSION
        }));
        assert_eq!(
            project::load(&db, &project.id).unwrap().versions.len(),
            versions_before + 1
        );

        let first = &suggestions[0];
        assert!(preview(&db, &project.id, &first.id).is_ok());
        assert_eq!(
            set_status(&mut db, &project.id, &first.id, "applied")
                .unwrap()
                .status,
            "applied"
        );
        assert_eq!(
            project::undo(&mut db, &project.id)
                .unwrap()
                .edits
                .iter()
                .find(|edit| edit.id == first.id)
                .unwrap()
                .status,
            "proposed"
        );
        assert_eq!(
            project::redo(&mut db, &project.id)
                .unwrap()
                .edits
                .iter()
                .find(|edit| edit.id == first.id)
                .unwrap()
                .status,
            "applied"
        );
        set_status(&mut db, &project.id, &first.id, "restored").unwrap();
        assert!(detect(&mut db, &project.id).unwrap().is_empty());
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Benchmark {
        version: u32,
        annotation_policy: String,
        cases: Vec<BenchmarkCase>,
    }

    #[derive(Deserialize)]
    struct BenchmarkCase {
        id: String,
        language: String,
        tokens: Vec<String>,
        #[serde(default)]
        starts: Vec<f64>,
        expected: Vec<ExpectedSuggestion>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedSuggestion {
        suggestion_type: String,
        from: usize,
        to: usize,
    }

    #[test]
    fn cut_suggestions_benchmark_meets_quality_thresholds() {
        let benchmark: Benchmark = serde_json::from_str(include_str!(
            "../skills/siaocut/tests/fixtures/cut-suggestions-v1.json"
        ))
        .unwrap();
        assert_eq!(benchmark.version, 1);
        assert!(benchmark.cases.len() >= 30);
        let mut true_positive = 0usize;
        let mut false_positive = 0usize;
        let mut false_negative = 0usize;
        let mut cases_by_language = BTreeMap::<String, usize>::new();
        let mut labels_by_type = BTreeMap::<String, usize>::new();
        for case in &benchmark.cases {
            *cases_by_language.entry(case.language.clone()).or_default() += 1;
            let words = case
                .tokens
                .iter()
                .enumerate()
                .map(|(index, token)| {
                    let start = case
                        .starts
                        .get(index)
                        .copied()
                        .unwrap_or(index as f64 * 0.4);
                    Word {
                        id: format!("{}-{index}", case.id),
                        segment_id: "benchmark".into(),
                        start,
                        end: start + 0.25,
                        text: token.clone(),
                        confidence: None,
                    }
                })
                .collect::<Vec<_>>();
            let word_refs = words.iter().collect::<Vec<_>>();
            let predicted = detect_word_ranges(&word_refs)
                .into_iter()
                .map(|suggestion| {
                    (
                        suggestion.suggestion_type.to_owned(),
                        suggestion.from_index,
                        suggestion.to_index,
                    )
                })
                .collect::<Vec<_>>();
            let expected = case
                .expected
                .iter()
                .map(|suggestion| {
                    *labels_by_type
                        .entry(suggestion.suggestion_type.clone())
                        .or_default() += 1;
                    (
                        suggestion.suggestion_type.clone(),
                        suggestion.from,
                        suggestion.to,
                    )
                })
                .collect::<Vec<_>>();
            true_positive += predicted
                .iter()
                .filter(|suggestion| expected.contains(suggestion))
                .count();
            false_positive += predicted
                .iter()
                .filter(|suggestion| !expected.contains(suggestion))
                .count();
            false_negative += expected
                .iter()
                .filter(|suggestion| !predicted.contains(suggestion))
                .count();
        }
        let precision = true_positive as f64 / (true_positive + false_positive) as f64;
        let recall = true_positive as f64 / (true_positive + false_negative) as f64;
        assert!(precision >= 0.95, "precision {precision:.3} is below 0.95");
        assert!(recall >= 0.80, "recall {recall:.3} is below 0.80");

        if let Some(path) = std::env::var_os("SIAOCUT_PHASE3_EVIDENCE") {
            let evidence = json!({
                "date": "2026-07-17",
                "status": "passed",
                "detectorVersion": DETECTOR_VERSION,
                "benchmarkVersion": benchmark.version,
                "fixture": "skills/siaocut/tests/fixtures/cut-suggestions-v1.json",
                "annotationPolicy": benchmark.annotation_policy,
                "caseCount": benchmark.cases.len(),
                "casesByLanguage": cases_by_language,
                "labelsByType": labels_by_type,
                "truePositive": true_positive,
                "falsePositive": false_positive,
                "falseNegative": false_negative,
                "precision": precision,
                "recall": recall,
                "thresholds": {"precision": 0.95, "recall": 0.80}
            });
            fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        }
    }
}
