use crate::{
    media::hash_file,
    model::{Project, Segment, SubtitleMode},
    subtitle_quality, subtitle_style, timeline,
};

const BILINGUAL_SEPARATOR: char = '\u{001e}';
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

pub struct ExportOptions<'a> {
    pub format: &'a str,
    pub language: Option<&'a str>,
    pub subtitle_mode: SubtitleMode,
    pub include_cuts: bool,
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
    if translation.status == "stale" {
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
    report["subtitleQuality"] = quality_reports(project, options);
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
        if translation.status == "stale" {
            issues.push(json!({"code":"stale-translation","language":language}))
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
    json!({
        "ready": !issues.iter().any(blocks_export),
        "issues": issues,
        "warnings": warnings
    })
}

fn blocks_export(issue: &Value) -> bool {
    !matches!(
        issue["code"].as_str(),
        Some("stale-translation" | "caption-too-long" | "overlapping-caption")
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

fn ass_dialogue_text(text: &str, subtitle_mode: SubtitleMode) -> String {
    if subtitle_mode == SubtitleMode::Bilingual
        && let Some((primary, secondary)) = text.split_once(BILINGUAL_SEPARATOR)
    {
        return format!(
            "{}\\N{{\\rSecondary}}{}",
            escape_ass_text(primary),
            escape_ass_text(secondary)
        );
    }
    escape_ass_text(text)
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
                    segments.push((
                        start,
                        end,
                        wrap_subtitle_text(&text, &project.transcript.source_language),
                    ));
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
        segments.push((start, end, text));
    }
    Ok(match options.format {
        "markdown" => format!(
            "# {}\n\n{}\n",
            project.title,
            segments
                .iter()
                .map(|(start, _, text)| format!(
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
                .map(|(start, end, text)| format!(
                    "{} --> {}\n{}\n",
                    timestamp(*start, '.'),
                    timestamp(*end, '.'),
                    display_subtitle_text(text)
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        "ass" => {
            let header =
                subtitle_style::ass_header(&project.subtitle_style, project.canvas_settings)?;
            format!(
                "{}\n{}\n",
                header,
                segments
                    .iter()
                    .map(|(start, end, text)| format!(
                        "Dialogue: 0,{},{},Primary,{}",
                        timestamp(*start, '.')[1..].trim_end_matches(".000"),
                        timestamp(*end, '.')[1..].trim_end_matches(".000"),
                        ass_dialogue_text(text, options.subtitle_mode)
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
        _ => segments
            .iter()
            .enumerate()
            .map(|(index, (start, end, text))| {
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
                segments: vec![TranslationSegment {
                    segment_id: "a".into(),
                    text: "Translation".into(),
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
            },
        )
        .unwrap();
        assert!(bilingual_ass.contains("PlayResX: 1920\nPlayResY: 1080"));
        assert!(bilingual_ass.contains("Style: Primary,Microsoft YaHei UI,60"));
        assert!(bilingual_ass.contains("Style: Secondary,Microsoft YaHei UI,46"));
        assert!(
            bilingual_ass
                .contains("Dialogue: 0,0:00:00,0:00:01,Primary,原文\\N{\\rSecondary}Translation")
        );

        project.translations.get_mut("en").unwrap().status = "stale".into();
        let error = render(
            &project,
            &ExportOptions {
                format: "srt",
                language: Some("en"),
                subtitle_mode: SubtitleMode::Translated,
                include_cuts: false,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("translation_stale"));
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
            },
        )
        .unwrap();
        assert!(srt.contains("hello"));
        assert!(srt.contains("world"));
        assert!(!srt.contains("brave"));
        assert!(srt.contains("00:00:01,100 --> 00:00:01,800"));
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
