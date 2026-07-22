use crate::{
    media::{ffprobe_video_dimensions, hash_file},
    model::{Project, Segment, SubtitleMode},
    subtitle_quality, subtitle_style, timeline,
};

const BILINGUAL_SEPARATOR: char = '\u{001e}';
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::Path,
};

pub struct ExportOptions<'a> {
    pub format: &'a str,
    pub language: Option<&'a str>,
    pub subtitle_mode: SubtitleMode,
    pub include_cuts: bool,
    pub allow_stale_translation: bool,
}

pub fn resolve_subtitle_mode(
    explicit: Option<&str>,
    language: Option<&str>,
    bilingual: bool,
) -> Result<SubtitleMode> {
    let mode = if let Some(value) = explicit {
        let mode = SubtitleMode::parse(value)
            .ok_or_else(|| anyhow::anyhow!("字幕模式只支持 source、translated 或 bilingual"))?;
        if bilingual && mode != SubtitleMode::Bilingual {
            bail!("--bilingual 与 --subtitle-mode {value} 冲突")
        }
        if mode == SubtitleMode::Source && language.is_some() {
            bail!("source 字幕模式不能同时指定 --lang")
        }
        mode
    } else if bilingual {
        SubtitleMode::Bilingual
    } else if language.is_some() {
        SubtitleMode::Translated
    } else {
        SubtitleMode::Source
    };
    if mode != SubtitleMode::Source && language.is_none() {
        bail!("translated 和 bilingual 字幕模式必须指定 --lang")
    }
    Ok(mode)
}

pub fn validate_subtitle_mode(project: &Project, options: &ExportOptions<'_>) -> Result<()> {
    if options.subtitle_mode == SubtitleMode::Source {
        return Ok(());
    }
    let language = options
        .language
        .ok_or_else(|| anyhow::anyhow!("translation_missing: 译文字幕模式缺少目标语言"))?;
    let translation = project
        .translations
        .get(language)
        .ok_or_else(|| anyhow::anyhow!("translation_missing: 项目中没有 {language} 译文"))?;
    if translation.status == "stale" && !options.allow_stale_translation {
        bail!("translation_stale: {language} 译文已过期，请先更新译文")
    }
    let translated = translation
        .segments
        .iter()
        .map(|segment| segment.segment_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    if let Some(segment) = project
        .transcript
        .segments
        .iter()
        .find(|segment| !translated.contains(segment.id.as_str()))
    {
        bail!(
            "translation_incomplete: {language} 译文缺少字幕段 {}",
            segment.id
        )
    }
    Ok(())
}

pub fn quality_reports(project: &Project, options: &ExportOptions<'_>) -> Value {
    let source_report = || {
        subtitle_quality::inspect_with_language(
            &project.transcript.segments,
            project.media.duration_seconds,
            &project.transcript.source_language,
        )
    };
    let translated_report = || {
        let language = options.language.unwrap_or("");
        let translated = project
            .translations
            .get(language)
            .map(|translation| {
                translation
                    .segments
                    .iter()
                    .map(|segment| (segment.segment_id.as_str(), segment.text.as_str()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let segments = project
            .transcript
            .segments
            .iter()
            .map(|segment| Segment {
                id: segment.id.clone(),
                start: segment.start,
                end: segment.end,
                text: translated
                    .get(segment.id.as_str())
                    .copied()
                    .unwrap_or("")
                    .to_owned(),
                confidence: segment.confidence,
            })
            .collect::<Vec<_>>();
        subtitle_quality::inspect_with_language(&segments, project.media.duration_seconds, language)
    };
    match options.subtitle_mode {
        SubtitleMode::Source => json!([{
            "track": "source",
            "language": project.transcript.source_language,
            "report": source_report(),
        }]),
        SubtitleMode::Translated => json!([{
            "track": "translated",
            "language": options.language,
            "report": translated_report(),
        }]),
        SubtitleMode::Bilingual => json!([
            {
                "track": "source",
                "language": project.transcript.source_language,
                "report": source_report(),
            },
            {
                "track": "translated",
                "language": options.language,
                "report": translated_report(),
            }
        ]),
    }
}

pub fn audit_for_options(project: &Project, options: &ExportOptions<'_>) -> Value {
    let mut report = audit(project);
    let subtitle_quality = quality_reports(project, options);
    report["subtitleQuality"] = subtitle_quality.clone();
    let mut blockers = report["blockers"].as_array().cloned().unwrap_or_default();
    for track in subtitle_quality.as_array().into_iter().flatten() {
        for issue in track["report"]["issues"].as_array().into_iter().flatten() {
            if issue["severity"] == "error" {
                blockers.push(json!({
                    "code": "subtitle-quality-error",
                    "track": track["track"],
                    "language": track["language"],
                    "segmentId": issue["segmentId"],
                    "kind": issue["kind"]
                }));
            }
        }
    }
    if options.subtitle_mode != SubtitleMode::Source {
        let language = options.language.unwrap_or("");
        match project.translations.get(language) {
            None => blockers.push(json!({"code":"translation-missing","language":language})),
            Some(translation) => {
                let translated = translation
                    .segments
                    .iter()
                    .map(|segment| segment.segment_id.as_str())
                    .collect::<HashSet<_>>();
                for source in &project.transcript.segments {
                    if !translated.contains(source.id.as_str()) {
                        blockers.push(json!({"code":"translation-incomplete","language":language,"segmentId":source.id}));
                    }
                }
            }
        }
    }
    report["blockers"] = Value::Array(blockers.clone());
    report["ready"] = Value::Bool(blockers.is_empty());
    report["readiness"] = Value::String(
        if blockers.is_empty() {
            if report["warnings"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
            {
                "needs_confirmation"
            } else {
                "ready"
            }
        } else {
            "blocked"
        }
        .to_owned(),
    );
    let warning_slice = report["warnings"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    report["nextActions"] = Value::Array(next_actions(&blockers, warning_slice));
    report
}

pub fn audit(project: &Project) -> Value {
    let mut issues = Vec::new();
    if project.transcript.segments.is_empty() {
        issues.push(json!({"code":"no-speech-detected"}));
    }
    for (index, segment) in project.transcript.segments.iter().enumerate() {
        if segment.text.trim().is_empty() {
            issues.push(json!({"code":"empty-caption","segmentId":segment.id}))
        }
        if segment.end <= segment.start {
            issues.push(json!({"code":"invalid-time-range","segmentId":segment.id}))
        }
        if index > 0 && segment.start < project.transcript.segments[index - 1].end {
            issues.push(json!({"code":"overlapping-caption","segmentId":segment.id}))
        }
        if segment.end - segment.start > 8.0 {
            issues.push(json!({"code":"caption-too-long","segmentId":segment.id,"duration":segment.end-segment.start}))
        }
    }
    for word in &project.transcript.words {
        if word.end <= word.start {
            issues.push(json!({"code":"invalid-word-time-range","wordId":word.id}));
            continue;
        }
        if let Some(segment) = project
            .transcript
            .segments
            .iter()
            .find(|segment| segment.id == word.segment_id)
            && (word.start < segment.start - 0.001 || word.end > segment.end + 0.001)
        {
            issues.push(
                json!({"code":"word-outside-segment","wordId":word.id,"segmentId":segment.id}),
            );
        }
    }
    for (language, translation) in &project.translations {
        let stale_segments = translation
            .segments
            .iter()
            .filter(|segment| segment.status == "stale")
            .map(|segment| segment.segment_id.as_str())
            .collect::<Vec<_>>();
        let quality_failed_segments = translation
            .segments
            .iter()
            .filter(|segment| segment.status == "quality_failed")
            .map(|segment| segment.segment_id.as_str())
            .collect::<Vec<_>>();
        if translation.status == "stale" || !stale_segments.is_empty() {
            issues.push(
                json!({"code":"stale-translation","language":language,"segmentIds":stale_segments}),
            )
        }
        if !quality_failed_segments.is_empty() {
            issues.push(json!({"code":"translation-quality-failed","language":language,"segmentIds":quality_failed_segments}))
        }
    }
    for edit in project.edits.iter().filter(|edit| edit.status == "applied") {
        if edit.kind == "word_cut" {
            let Some(range) = &edit.cut_range else {
                issues.push(json!({"code":"word-cut-range-missing","editId":edit.id}));
                continue;
            };
            if range.stale {
                issues.push(json!({"code":"word-cut-stale","editId":edit.id}));
                continue;
            }
            let words = project
                .transcript
                .words
                .iter()
                .filter(|word| word.segment_id == edit.segment_id)
                .collect::<Vec<_>>();
            let from = words.iter().position(|word| word.id == range.from_word_id);
            let to = words.iter().position(|word| word.id == range.to_word_id);
            if from.is_none() || to.is_none() || from > to {
                issues.push(json!({"code":"word-cut-boundary-mismatch","editId":edit.id}));
            }
            continue;
        }
        if !matches!(edit.kind.as_str(), "cut" | "semantic_cut") {
            continue;
        }
        let matches_segment = project.transcript.segments.iter().any(|segment| {
            segment.id == edit.segment_id
                && (segment.start - edit.start).abs() < 0.001
                && (segment.end - edit.end).abs() < 0.001
        });
        if !matches_segment {
            issues.push(json!({"code":"cut-boundary-mismatch","editId":edit.id}));
        }
    }
    if !project.edits.is_empty() && timeline::build(project).output_duration <= 0.001 {
        issues.push(json!({"code":"empty-output-timeline"}));
    }
    let media_path = Path::new(&project.media.source_path);
    if !media_path.is_file() {
        issues.push(json!({"code":"media-missing","sourcePath":project.media.source_path}));
    } else if let Ok(actual_hash) = hash_file(media_path) {
        if actual_hash != project.media.sha256 {
            issues
                .push(json!({"code":"media-hash-changed","sourcePath":project.media.source_path}));
        }
    } else {
        issues.push(json!({"code":"media-unreadable","sourcePath":project.media.source_path}));
    }
    let warnings = issues
        .iter()
        .filter(|issue| !blocks_export(issue))
        .cloned()
        .collect::<Vec<_>>();
    let blockers = issues
        .iter()
        .filter(|issue| blocks_export(issue))
        .cloned()
        .collect::<Vec<_>>();
    let readiness = if !blockers.is_empty() {
        "blocked"
    } else if !warnings.is_empty() {
        "needs_confirmation"
    } else {
        "ready"
    };
    json!({
        "ready": blockers.is_empty(),
        "readiness": readiness,
        "issues": issues,
        "blockers": blockers,
        "warnings": warnings,
        "nextActions": next_actions(&blockers, &warnings)
    })
}

fn next_actions(blockers: &[Value], warnings: &[Value]) -> Vec<Value> {
    let mut actions = BTreeSet::new();
    for issue in blockers.iter().chain(warnings) {
        let action = match issue["code"].as_str().unwrap_or_default() {
            "media-missing" | "media-hash-changed" | "media-unreadable" => "relink-media",
            "stale-translation" | "translation-quality-failed" => "refresh-translation",
            "word-cut-stale" | "word-cut-boundary-mismatch" | "cut-boundary-mismatch" => {
                "review-cuts"
            }
            "translation-missing" | "translation-incomplete" => "complete-translation",
            _ => "fix-subtitles",
        };
        actions.insert(action);
    }
    actions
        .into_iter()
        .map(|code| json!({"code":code}))
        .collect()
}

fn blocks_export(issue: &Value) -> bool {
    !matches!(
        issue["code"].as_str(),
        Some(
            "stale-translation"
                | "translation-quality-failed"
                | "caption-too-long"
                | "overlapping-caption"
        )
    )
}

fn timestamp(seconds: f64, separator: char) -> String {
    let milliseconds = (seconds * 1000.0).round() as i64;
    format!(
        "{:02}:{:02}:{:02}{}{:03}",
        milliseconds / 3_600_000,
        (milliseconds % 3_600_000) / 60_000,
        (milliseconds % 60_000) / 1000,
        separator,
        milliseconds % 1000
    )
}

fn ass_timestamp(seconds: f64) -> String {
    let centiseconds = (seconds * 100.0).round().max(0.0) as i64;
    format!(
        "{}:{:02}:{:02}.{:02}",
        centiseconds / 360_000,
        (centiseconds % 360_000) / 6_000,
        (centiseconds % 6_000) / 100,
        centiseconds % 100
    )
}

fn join_words(words: &[&crate::model::Word]) -> String {
    let mut text = String::new();
    for word in words {
        let token = word.text.as_str();
        let needs_space = text
            .chars()
            .last()
            .is_some_and(|left| left.is_ascii_alphanumeric())
            && token
                .chars()
                .next()
                .is_some_and(|right| right.is_ascii_alphanumeric());
        if needs_space {
            text.push(' ');
        }
        text.push_str(token);
    }
    text.trim().to_owned()
}

fn escape_ass_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('\n', "\\N")
}

fn normalized_caption_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn karaoke_duration(start: f64, end: f64) -> i64 {
    ((end - start).max(0.01) * 100.0).round().max(1.0) as i64
}

fn karaoke_span(text: &str, start: f64, end: f64) -> String {
    format!(
        "{{\\kf{}}}{}",
        karaoke_duration(start, end),
        escape_ass_text(text)
    )
}

fn source_karaoke_text(
    project: &Project,
    segment_id: &str,
    map: &crate::model::TimelineMap,
    caption_start: f64,
    caption_end: f64,
    expected_text: &str,
) -> Option<String> {
    let mut words = project
        .transcript
        .words
        .iter()
        .filter(|word| word.segment_id == segment_id)
        .filter_map(|word| {
            timeline::retime_interval(map, word.start, word.end)
                .filter(|(start, end)| *end > caption_start && *start < caption_end)
                .map(|(start, end)| (start.max(caption_start), end.min(caption_end), word))
        })
        .collect::<Vec<_>>();
    words.sort_by(|left, right| left.0.total_cmp(&right.0));
    if words.is_empty() {
        return None;
    }
    let word_refs = words.iter().map(|(_, _, word)| *word).collect::<Vec<_>>();
    if normalized_caption_text(&join_words(&word_refs)) != normalized_caption_text(expected_text) {
        return None;
    }

    let mut rendered = String::new();
    let mut cursor = caption_start;
    let mut previous_last = None;
    for (index, (_, _, word)) in words.iter().enumerate() {
        let boundary = words
            .get(index + 1)
            .map(|(start, _, _)| *start)
            .unwrap_or(caption_end)
            .clamp(cursor + 0.01, caption_end.max(cursor + 0.01));
        let leading_space = previous_last.is_some_and(|left: char| left.is_ascii_alphanumeric())
            && word
                .text
                .chars()
                .next()
                .is_some_and(|right| right.is_ascii_alphanumeric());
        let token = if leading_space {
            format!(" {}", word.text)
        } else {
            word.text.clone()
        };
        rendered.push_str(&format!(
            "{{\\kf{}}}{}",
            karaoke_duration(cursor, boundary),
            escape_ass_text(&token)
        ));
        previous_last = word.text.chars().last();
        cursor = boundary;
    }
    Some(rendered)
}

pub fn wrap_subtitle_text(text: &str, language: &str) -> String {
    if !language.to_ascii_lowercase().starts_with("en") {
        return text.to_owned();
    }
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let visible = |value: &str| {
        value
            .chars()
            .filter(|character| !character.is_whitespace())
            .count()
    };
    if normalized.lines().count() <= 2 && visible(&normalized) <= 42 {
        return normalized;
    }
    let mut candidates = Vec::new();
    for (index, character) in normalized.char_indices() {
        if character.is_whitespace() {
            candidates.push(index);
        } else if matches!(character, '.' | ',' | '!' | '?' | ';' | ':' | '—' | '-') {
            candidates.push(index + character.len_utf8());
        }
    }
    candidates
        .into_iter()
        .filter_map(|index| {
            let (left, right) = normalized.split_at(index);
            let left = left.trim();
            let right = right.trim();
            if left.is_empty() || right.is_empty() {
                return None;
            }
            let left_visible = visible(left);
            let right_visible = visible(right);
            let overflow = left_visible.max(right_visible).saturating_sub(42);
            let imbalance = left_visible.abs_diff(right_visible);
            Some(((overflow, imbalance), format!("{left}\n{right}")))
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, wrapped)| wrapped)
        .unwrap_or(normalized)
}

fn display_subtitle_text(text: &str) -> String {
    text.replace(BILINGUAL_SEPARATOR, "\n")
}

fn ass_dialogue_text(
    text: &str,
    subtitle_mode: SubtitleMode,
    start: f64,
    end: f64,
    source_karaoke: Option<&str>,
) -> String {
    if subtitle_mode == SubtitleMode::Bilingual
        && let Some((primary, secondary)) = text.split_once(BILINGUAL_SEPARATOR)
    {
        return format!(
            "{}\\N{{\\rSecondary}}{}",
            source_karaoke
                .map(str::to_owned)
                .unwrap_or_else(|| karaoke_span(primary, start, end)),
            escape_ass_text(secondary)
        );
    }
    if subtitle_mode == SubtitleMode::Source {
        source_karaoke
            .map(str::to_owned)
            .unwrap_or_else(|| karaoke_span(text, start, end))
    } else {
        karaoke_span(text, start, end)
    }
}

fn source_parts(project: &Project, segment_id: &str) -> Option<Vec<(f64, f64, String)>> {
    let words = project
        .transcript
        .words
        .iter()
        .filter(|word| word.segment_id == segment_id)
        .collect::<Vec<_>>();
    let active_ranges = project
        .edits
        .iter()
        .filter(|edit| {
            edit.status == "applied" && edit.kind == "word_cut" && edit.segment_id == segment_id
        })
        .filter_map(|edit| edit.cut_range.as_ref())
        .filter(|range| !range.stale)
        .collect::<Vec<_>>();
    if active_ranges.is_empty() {
        return None;
    }
    let mut removed = HashSet::new();
    for range in active_ranges {
        let Some(from) = words.iter().position(|word| word.id == range.from_word_id) else {
            continue;
        };
        let Some(to) = words.iter().position(|word| word.id == range.to_word_id) else {
            continue;
        };
        if from <= to {
            removed.extend(from..=to);
        }
    }
    let mut parts = Vec::new();
    let mut cursor = 0;
    while cursor < words.len() {
        while cursor < words.len() && removed.contains(&cursor) {
            cursor += 1;
        }
        let start = cursor;
        while cursor < words.len() && !removed.contains(&cursor) {
            cursor += 1;
        }
        if start < cursor {
            let group = &words[start..cursor];
            parts.push((
                group[0].start,
                group[group.len() - 1].end,
                join_words(group),
            ));
        }
    }
    Some(parts)
}

pub fn render(project: &Project, options: &ExportOptions<'_>) -> Result<String> {
    validate_subtitle_mode(project, options)?;
    if !options.include_cuts {
        for edit in project
            .edits
            .iter()
            .filter(|edit| edit.status == "applied" && edit.kind == "word_cut")
        {
            let range = edit
                .cut_range
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("word_cut_range_missing: 词范围剪辑缺少边界证据"))?;
            if range.stale {
                bail!("word_cut_stale: 字幕文本已经变化，请重新创建词范围剪辑")
            }
            let words = project
                .transcript
                .words
                .iter()
                .filter(|word| word.segment_id == edit.segment_id)
                .collect::<Vec<_>>();
            let from = words.iter().position(|word| word.id == range.from_word_id);
            let to = words.iter().position(|word| word.id == range.to_word_id);
            if !matches!((from, to), (Some(from), Some(to)) if from <= to) {
                bail!("word_cut_boundary_mismatch: 词范围剪辑边界与当前词级证据不一致")
            }
        }
    }
    let translation = options
        .language
        .and_then(|language| project.translations.get(language));
    let translated = translation
        .map(|translation| {
            translation
                .segments
                .iter()
                .map(|segment| (segment.segment_id.clone(), segment.text.clone()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let timeline = timeline::build(project);
    let mut segments = Vec::new();
    for segment in &project.transcript.segments {
        if !options.include_cuts
            && options.subtitle_mode == SubtitleMode::Source
            && let Some(parts) = source_parts(project, &segment.id)
        {
            for (source_start, source_end, text) in parts {
                if let Some((start, end)) =
                    timeline::retime_interval(&timeline, source_start, source_end)
                {
                    let text = wrap_subtitle_text(&text, &project.transcript.source_language);
                    let karaoke =
                        source_karaoke_text(project, &segment.id, &timeline, start, end, &text);
                    segments.push((start, end, text, karaoke));
                }
            }
            continue;
        }
        let (start, end) = if options.include_cuts {
            (segment.start, segment.end)
        } else if let Some(interval) =
            timeline::retime_interval(&timeline, segment.start, segment.end)
        {
            interval
        } else {
            continue;
        };
        let target = translated.get(&segment.id);
        let text = match options.subtitle_mode {
            SubtitleMode::Source => {
                wrap_subtitle_text(&segment.text, &project.transcript.source_language)
            }
            SubtitleMode::Translated => wrap_subtitle_text(
                target.cloned().unwrap_or_default().as_str(),
                options.language.unwrap_or(""),
            ),
            SubtitleMode::Bilingual => {
                format!(
                    "{}{}{}",
                    wrap_subtitle_text(&segment.text, &project.transcript.source_language),
                    BILINGUAL_SEPARATOR,
                    wrap_subtitle_text(
                        target.cloned().unwrap_or_default().as_str(),
                        options.language.unwrap_or("")
                    )
                )
            }
        };
        let karaoke = (options.subtitle_mode != SubtitleMode::Translated)
            .then(|| {
                let expected = text
                    .split_once(BILINGUAL_SEPARATOR)
                    .map(|(source, _)| source)
                    .unwrap_or(text.as_str());
                source_karaoke_text(project, &segment.id, &timeline, start, end, expected)
            })
            .flatten();
        segments.push((start, end, text, karaoke));
    }
    Ok(match options.format {
        "markdown" => format!(
            "# {}\n\n{}\n",
            project.title,
            segments
                .iter()
                .map(|(start, _, text, _)| format!(
                    "- **{}** {}",
                    &timestamp(*start, '.')[..8],
                    display_subtitle_text(text)
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        "vtt" => format!(
            "WEBVTT\n\n{}",
            segments
                .iter()
                .map(|(start, end, text, _)| format!(
                    "{} --> {}\n{}\n",
                    timestamp(*start, '.'),
                    timestamp(*end, '.'),
                    display_subtitle_text(text)
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        "ass" => {
            let header = subtitle_style::ass_header_for_media(
                &project.subtitle_style,
                project.canvas_settings,
                ffprobe_video_dimensions(Path::new(&project.media.source_path)),
            )?;
            format!(
                "{}\n{}\n",
                header,
                segments
                    .iter()
                    .map(|(start, end, text, karaoke)| format!(
                        "Dialogue: 0,{},{},Primary,{}",
                        ass_timestamp(*start),
                        ass_timestamp(*end),
                        ass_dialogue_text(
                            text,
                            options.subtitle_mode,
                            *start,
                            *end,
                            karaoke.as_deref()
                        )
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
        _ => segments
            .iter()
            .enumerate()
            .map(|(index, (start, end, text, _))| {
                format!(
                    "{}\n{} --> {}\n{}\n",
                    index + 1,
                    timestamp(*start, ','),
                    timestamp(*end, ','),
                    display_subtitle_text(text)
                )
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CanvasSettings, CutRange, Edit, Media, Project, Segment, Transcript, Translation,
        TranslationSegment, Word,
    };
    use std::collections::BTreeMap;

    #[test]
    fn english_wrapping_uses_only_word_or_punctuation_boundaries() {
        let text = "This is a practical English subtitle that should wrap cleanly without splitting any word.";
        let wrapped = wrap_subtitle_text(text, "en-US");
        let lines = wrapped.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines.join(" "), text);
        assert!(
            lines
                .iter()
                .all(|line| !line.starts_with(' ') && !line.ends_with(' '))
        );
        assert!(lines.iter().all(|line| {
            line.chars()
                .filter(|character| !character.is_whitespace())
                .count()
                <= 42
        }));

        let unbreakable = "SupercalifragilisticexpialidociousSupercalifragilisticexpialidocious";
        assert_eq!(wrap_subtitle_text(unbreakable, "en"), unbreakable);
        assert_eq!(
            wrap_subtitle_text("中文不会按英文空格断行", "zh"),
            "中文不会按英文空格断行"
        );
    }

    #[test]
    fn ass_timestamps_use_centiseconds_and_karaoke_durations() {
        assert_eq!(ass_timestamp(0.07), "0:00:00.07");
        assert_eq!(ass_timestamp(6.4), "0:00:06.40");
        assert_eq!(ass_timestamp(86.6), "0:01:26.60");
        assert_eq!(karaoke_span("同步", 0.07, 0.57), "{\\kf50}同步");
    }

    #[test]
    fn applied_cut_retimes_following_caption() {
        let project = Project {
            id: "p".into(),
            title: "test".into(),
            created_at: String::new(),
            updated_at: String::new(),
            canvas_settings: CanvasSettings::default(),
            subtitle_style: Default::default(),
            media: Media {
                source_path: String::new(),
                sha256: String::new(),
                extension: ".wav".into(),
                duration_seconds: None,
            },
            media_artifacts: None,
            timeline: Default::default(),
            transcript: Transcript {
                source_language: "auto".into(),
                segments: vec![
                    Segment {
                        id: "a".into(),
                        start: 0.0,
                        end: 1.0,
                        text: "嗯".into(),
                        confidence: None,
                    },
                    Segment {
                        id: "b".into(),
                        start: 1.0,
                        end: 3.0,
                        text: "开始".into(),
                        confidence: None,
                    },
                ],
                words: Vec::new(),
            },
            subtitle_quality: Default::default(),
            speech_insights: Default::default(),
            translations: BTreeMap::new(),
            glossary: Default::default(),
            edits: vec![Edit {
                id: "cut".into(),
                kind: "cut".into(),
                status: "applied".into(),
                segment_id: "a".into(),
                start: 0.0,
                end: 1.0,
                reason: "filler".into(),
                created_at: String::new(),
                cut_range: None,
                suggestion: None,
            }],
            tasks: Vec::new(),
            versions: Vec::new(),
            history: Default::default(),
            patch_sets: Vec::new(),
            workflows: Vec::new(),
        };
        let srt = render(
            &project,
            &ExportOptions {
                format: "srt",
                language: None,
                subtitle_mode: SubtitleMode::Source,
                include_cuts: false,
                allow_stale_translation: false,
            },
        )
        .unwrap();
        assert!(srt.contains("00:00:00,000 --> 00:00:02,000"));
        assert!(!srt.contains("嗯"));

        let mut empty = project.clone();
        empty.transcript.segments.clear();
        empty.transcript.words.clear();
        assert_eq!(audit(&empty)["issues"][0]["code"], "no-speech-detected");

        let mut long = project;
        long.transcript.segments[1].end = 12.0;
        assert!(
            audit(&long)["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["code"] == "caption-too-long")
        );
        assert_eq!(audit(&long)["warnings"][0]["code"], "caption-too-long");
        assert!(!blocks_export(&json!({"code":"caption-too-long"})));

        let mut overlapping = long.clone();
        overlapping.transcript.segments[1].start = 0.5;
        assert_eq!(
            audit(&overlapping)["warnings"][0]["code"],
            "overlapping-caption"
        );
        assert!(!blocks_export(&json!({"code":"overlapping-caption"})));

        let mut misaligned = long;
        misaligned.transcript.words.push(Word {
            id: "w".into(),
            segment_id: "a".into(),
            start: 0.5,
            end: 1.5,
            text: "越界".into(),
            confidence: Some(0.9),
        });
        assert!(
            audit(&misaligned)["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["code"] == "word-outside-segment")
        );
    }

    #[test]
    fn subtitle_modes_never_fall_back_to_source_text() {
        let mut project = Project {
            id: "p".into(),
            title: "test".into(),
            created_at: String::new(),
            updated_at: String::new(),
            canvas_settings: CanvasSettings::default(),
            subtitle_style: Default::default(),
            media: Media {
                source_path: String::new(),
                sha256: String::new(),
                extension: ".wav".into(),
                duration_seconds: None,
            },
            media_artifacts: None,
            timeline: Default::default(),
            transcript: Transcript {
                source_language: "zh".into(),
                segments: vec![Segment {
                    id: "a".into(),
                    start: 0.0,
                    end: 1.0,
                    text: "原文".into(),
                    confidence: None,
                }],
                words: Vec::new(),
            },
            subtitle_quality: Default::default(),
            speech_insights: Default::default(),
            translations: BTreeMap::new(),
            glossary: Default::default(),
            edits: Vec::new(),
            tasks: Vec::new(),
            versions: Vec::new(),
            history: Default::default(),
            patch_sets: Vec::new(),
            workflows: Vec::new(),
        };
        project.translations.insert(
            "en".into(),
            Translation {
                status: "current".into(),
                updated_at: String::new(),
                glossary_version: 0,
                segments: vec![TranslationSegment {
                    segment_id: "a".into(),
                    text: "Translation".into(),
                    source_hash: crate::translation::source_hash("原文"),
                    status: "current".into(),
                    updated_at: String::new(),
                }],
            },
        );

        let translated = render(
            &project,
            &ExportOptions {
                format: "srt",
                language: Some("en"),
                subtitle_mode: SubtitleMode::Translated,
                include_cuts: false,
                allow_stale_translation: false,
            },
        )
        .unwrap();
        assert!(translated.contains("Translation"));
        assert!(!translated.contains("原文"));

        let bilingual = render(
            &project,
            &ExportOptions {
                format: "srt",
                language: Some("en"),
                subtitle_mode: SubtitleMode::Bilingual,
                include_cuts: false,
                allow_stale_translation: false,
            },
        )
        .unwrap();
        assert!(bilingual.contains("原文\nTranslation"));

        project.translations.get_mut("en").unwrap().segments[0].text = "one\ntwo\nthree".into();
        let reports = quality_reports(
            &project,
            &ExportOptions {
                format: "srt",
                language: Some("en"),
                subtitle_mode: SubtitleMode::Bilingual,
                include_cuts: false,
                allow_stale_translation: false,
            },
        );
        assert_eq!(reports.as_array().unwrap().len(), 2);
        assert!(
            reports[1]["report"]["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["kind"] == "too_many_lines")
        );

        project.translations.get_mut("en").unwrap().segments[0].text = "Translation".into();

        project.subtitle_style = crate::subtitle_style::resolve(
            crate::model::SubtitleStylePreset::Emphasis,
            crate::model::SubtitlePosition::Center,
        );
        let bilingual_ass = render(
            &project,
            &ExportOptions {
                format: "ass",
                language: Some("en"),
                subtitle_mode: SubtitleMode::Bilingual,
                include_cuts: false,
                allow_stale_translation: false,
            },
        )
        .unwrap();
        assert!(bilingual_ass.contains("PlayResX: 1920\nPlayResY: 1080"));
        assert!(bilingual_ass.contains("Style: Primary,Microsoft YaHei UI,60"));
        assert!(bilingual_ass.contains("Style: Secondary,Microsoft YaHei UI,46"));
        assert!(bilingual_ass.contains(
            "Dialogue: 0,0:00:00.00,0:00:01.00,Primary,{\\kf100}原文\\N{\\rSecondary}Translation"
        ));

        project.translations.get_mut("en").unwrap().status = "stale".into();
        let error = render(
            &project,
            &ExportOptions {
                format: "srt",
                language: Some("en"),
                subtitle_mode: SubtitleMode::Translated,
                include_cuts: false,
                allow_stale_translation: false,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("translation_stale"));

        let report = audit_for_options(
            &project,
            &ExportOptions {
                format: "srt",
                language: Some("en"),
                subtitle_mode: SubtitleMode::Translated,
                include_cuts: false,
                allow_stale_translation: true,
            },
        );
        assert!(report["readiness"].is_string());
        assert!(report["blockers"].is_array());
        assert!(
            report["nextActions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|action| action["code"] == "refresh-translation")
        );
        assert!(
            render(
                &project,
                &ExportOptions {
                    format: "srt",
                    language: Some("en"),
                    subtitle_mode: SubtitleMode::Translated,
                    include_cuts: false,
                    allow_stale_translation: true,
                },
            )
            .unwrap()
            .contains("Translation")
        );
    }

    #[test]
    fn word_cut_removes_selected_words_from_source_subtitles() {
        let mut project = Project {
            id: "p".into(),
            title: "test".into(),
            created_at: String::new(),
            updated_at: String::new(),
            canvas_settings: CanvasSettings::default(),
            subtitle_style: Default::default(),
            media: Media {
                source_path: String::new(),
                sha256: String::new(),
                extension: ".wav".into(),
                duration_seconds: Some(2.7),
            },
            media_artifacts: None,
            timeline: Default::default(),
            transcript: Transcript {
                source_language: "en".into(),
                segments: vec![Segment {
                    id: "a".into(),
                    start: 0.2,
                    end: 2.7,
                    text: "hello brave world".into(),
                    confidence: None,
                }],
                words: [
                    ("w1", 0.2, 0.7, "hello"),
                    ("w2", 1.0, 1.5, "brave"),
                    ("w3", 2.0, 2.7, "world"),
                ]
                .into_iter()
                .map(|(id, start, end, text)| Word {
                    id: id.into(),
                    segment_id: "a".into(),
                    start,
                    end,
                    text: text.into(),
                    confidence: None,
                })
                .collect(),
            },
            subtitle_quality: Default::default(),
            speech_insights: Default::default(),
            translations: BTreeMap::new(),
            glossary: Default::default(),
            edits: Vec::new(),
            tasks: Vec::new(),
            versions: Vec::new(),
            history: Default::default(),
            patch_sets: Vec::new(),
            workflows: Vec::new(),
        };
        project.edits.push(Edit {
            id: "cut".into(),
            kind: "word_cut".into(),
            status: "applied".into(),
            segment_id: "a".into(),
            start: 0.8,
            end: 1.7,
            reason: "word range".into(),
            created_at: String::new(),
            cut_range: Some(CutRange {
                from_word_id: "w2".into(),
                to_word_id: "w2".into(),
                selected_start: 1.0,
                selected_end: 1.5,
                padding_ms: 200,
                transcript_hash: "hash".into(),
                stale: false,
            }),
            suggestion: None,
        });

        let srt = render(
            &project,
            &ExportOptions {
                format: "srt",
                language: None,
                subtitle_mode: SubtitleMode::Source,
                include_cuts: false,
                allow_stale_translation: false,
            },
        )
        .unwrap();
        assert!(srt.contains("hello"));
        assert!(srt.contains("world"));
        assert!(!srt.contains("brave"));
        assert!(srt.contains("00:00:01,100 --> 00:00:01,800"));

        let ass = render(
            &project,
            &ExportOptions {
                format: "ass",
                language: None,
                subtitle_mode: SubtitleMode::Source,
                include_cuts: false,
                allow_stale_translation: false,
            },
        )
        .unwrap();
        assert!(ass.contains("{\\kf50}hello"));
        assert!(ass.contains("{\\kf70}world"));
        assert!(!ass.contains("brave"));
    }

    #[test]
    fn resolves_new_and_legacy_subtitle_arguments() {
        assert_eq!(
            resolve_subtitle_mode(None, None, false).unwrap(),
            SubtitleMode::Source
        );
        assert_eq!(
            resolve_subtitle_mode(None, Some("en"), false).unwrap(),
            SubtitleMode::Translated
        );
        assert_eq!(
            resolve_subtitle_mode(Some("bilingual"), Some("en"), false).unwrap(),
            SubtitleMode::Bilingual
        );
        assert!(resolve_subtitle_mode(Some("source"), Some("en"), false).is_err());
        assert!(resolve_subtitle_mode(Some("translated"), None, false).is_err());
    }
}
