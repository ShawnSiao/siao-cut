use crate::{
    db::home_dir,
    model::{Project, Segment, Word},
    project,
    util::{hidden_command, new_id},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn tool_path(variable: &str, default: &str) -> String {
    env::var(variable).unwrap_or_else(|_| default.to_owned())
}

pub fn whisper_cli_path() -> String {
    if let Some(path) = crate::runtime::selected_whisper_path() {
        return path.to_string_lossy().to_string();
    }
    if let Ok(path) = env::var("SIAOCUT_WHISPER_CLI") {
        return path;
    }
    let bundled = home_dir().join("bin").join("whisper-cli.exe");
    if bundled.is_file() {
        bundled.to_string_lossy().to_string()
    } else {
        "whisper-cli".to_owned()
    }
}

pub fn whisper_vad_model_path() -> Option<String> {
    env::var("SIAOCUT_WHISPER_VAD_MODEL")
        .ok()
        .or_else(|| {
            let bundled = home_dir().join("bin").join("ggml-silero-v6.2.0.bin");
            bundled
                .is_file()
                .then(|| bundled.to_string_lossy().to_string())
        })
        .filter(|path| Path::new(path).is_file())
}

fn resolved_whisper_runtime() -> Result<(String, String)> {
    if let Some(selection) = crate::runtime::verified_selected_runtime()? {
        return Ok((selection.whisper_path, selection.backend));
    }
    Ok((whisper_cli_path(), "cpu".into()))
}

pub fn whisper_vad_timeline_capability() -> crate::runtime::VadTimelineCapability {
    match resolved_whisper_runtime() {
        Ok((whisper, backend)) => {
            crate::runtime::vad_timeline_capability(Path::new(&whisper), &backend)
        }
        Err(_) => crate::runtime::vad_timeline_capability(Path::new(""), "cpu"),
    }
}

struct TemporaryRunDirectory(PathBuf);

impl TemporaryRunDirectory {
    fn create(path: PathBuf) -> Result<Self> {
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TemporaryRunDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("无法读取媒体：{}", path.display()))?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

pub fn ffprobe_duration(path: &Path) -> Option<f64> {
    hidden_command(tool_path("SIAOCUT_FFPROBE", "ffprobe"))
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(path)
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout).ok()?.trim().parse().ok()
            } else {
                None
            }
        })
}

pub fn ffprobe_video_dimensions(path: &Path) -> Option<(u32, u32)> {
    hidden_command(tool_path("SIAOCUT_FFPROBE", "ffprobe"))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0:s=x",
        ])
        .arg(path)
        .output()
        .ok()
        .and_then(|output| {
            if !output.status.success() {
                return None;
            }
            let value = String::from_utf8(output.stdout).ok()?;
            let (width, height) = value.trim().split_once('x')?;
            let dimensions = (width.parse().ok()?, height.parse().ok()?);
            (dimensions.0 > 0 && dimensions.1 > 0).then_some(dimensions)
        })
}

pub fn command_available(command: &str) -> bool {
    hidden_command(command)
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn parse_whisper_timestamp(value: &str) -> Result<f64> {
    let value = value.replace(',', ".");
    let pieces = value.split(':').collect::<Vec<_>>();
    if pieces.len() != 3 {
        anyhow::bail!("无效 whisper 时间戳：{value}")
    };
    Ok(pieces[0].parse::<f64>()? * 3600.0
        + pieces[1].parse::<f64>()? * 60.0
        + pieces[2].parse::<f64>()?)
}

pub fn transcribe(
    db: &mut Connection,
    project_id: &str,
    model: &Path,
    language: Option<&str>,
    expected_version_id: &str,
    confirm_replace: bool,
) -> Result<TranscriptionResult> {
    if !model.is_file() {
        bail!("模型不存在：{}", model.display())
    }
    let project = project::load(db, project_id)?;
    if project.history.current_version_id.as_deref() != Some(expected_version_id) {
        bail!("transcription_project_changed: 项目已在本地转录开始前发生变化，请重新确认")
    }
    let replacing_transcript = !project.transcript.segments.is_empty();
    if replacing_transcript && !confirm_replace {
        bail!("transcription_apply_confirmation_required: 重新生成字幕前必须明确确认替换当前字幕")
    }
    if replacing_transcript {
        project::assert_transcript_replacement_safe(db, project_id)?;
    }
    let source_sha256 = hash_file(Path::new(&project.media.source_path))?;
    if source_sha256 != project.media.sha256 {
        bail!("media_hash_changed: 原片校验值已变化，不能开始本地转录")
    }
    let audio_dir = home_dir().join("cache").join("asr");
    fs::create_dir_all(&audio_dir)?;
    let run_directory = audio_dir.join(format!("{}-{}", project.id, new_id("quick")));
    let _run_guard = TemporaryRunDirectory::create(run_directory.clone())?;
    let wav = run_directory.join("audio.wav");
    let ffmpeg = tool_path("SIAOCUT_FFMPEG", "ffmpeg");
    let result = hidden_command(&ffmpeg)
        .args([
            "-y",
            "-i",
            &project.media.source_path,
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&wav)
        .output()
        .with_context(|| format!("无法启动 FFmpeg：{ffmpeg}"))?;
    if !result.status.success() {
        bail!(
            "FFmpeg 音频提取失败：{}",
            String::from_utf8_lossy(&result.stderr).trim()
        )
    }
    let audio_duration = ffprobe_duration(&wav)
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .ok_or_else(|| {
            anyhow!("transcription_timing_invalid: 无法确认标准化音频时长，结果未应用")
        })?;

    let (whisper, backend) = resolved_whisper_runtime()?;
    let vad_capability = crate::runtime::vad_timeline_capability(Path::new(&whisper), &backend);
    let vad_model = vad_capability
        .verified
        .then(whisper_vad_model_path)
        .flatten();
    let timing_mode = if vad_model.is_some() {
        TranscriptionTimingMode::verified_vad()
    } else {
        TranscriptionTimingMode::no_vad()
    };
    let output_base = run_directory.join("transcript");
    run_whisper(
        &whisper,
        model,
        &wav,
        &output_base,
        language,
        vad_model.as_deref(),
    )?;
    import_whisper_json_at_baseline_with_mode(
        db,
        &project.id,
        &output_base.with_extension("json"),
        TranscriptionImportBaseline {
            expected_version_id,
            expected_source_path: &project.media.source_path,
            expected_source_sha256: &source_sha256,
            audio_duration,
            confirm_replace,
        },
        timing_mode,
    )
}

fn run_whisper(
    whisper: &str,
    model: &Path,
    wav: &Path,
    output_base: &Path,
    language: Option<&str>,
    vad_model: Option<&str>,
) -> Result<()> {
    let mut command = hidden_command(whisper);
    command
        .args(["-m"])
        .arg(model)
        .args(["-f"])
        .arg(wav)
        .args(["-ojf", "-sow", "-ml", "60", "-of"])
        .arg(output_base);
    if let Some(vad_model) = vad_model {
        command.args([
            "--vad",
            "-vm",
            vad_model,
            "--vad-min-silence-duration-ms",
            "250",
            "--vad-speech-pad-ms",
            "80",
        ]);
    }
    if let Some(language) = language {
        command.args(["-l", language]);
    }
    let result = command
        .output()
        .with_context(|| format!("无法启动 whisper.cpp：{whisper}"))?;
    if !result.status.success() {
        bail!(
            "whisper.cpp 转录失败（退出码 {}）：{}",
            result
                .status
                .code()
                .map_or_else(|| "unknown".to_owned(), |code| code.to_string()),
            String::from_utf8_lossy(&result.stderr).trim()
        )
    }
    Ok(())
}

const MAX_CAPTION_DURATION_SECONDS: f64 = 8.0;
const AUDIO_DURATION_TOLERANCE_SECONDS: f64 = 0.25;
const PARENT_SEGMENT_TOLERANCE_SECONDS: f64 = 0.5;
const TIMELINE_ORDER_TOLERANCE_SECONDS: f64 = 0.001;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingValidation {
    pub status: String,
    pub time_domain: String,
    pub mode: String,
    pub vad_used: bool,
    pub segment_count: usize,
    pub word_count: usize,
}

#[derive(Debug)]
pub struct TranscriptionResult {
    pub project: Project,
    pub timing_validation: TimingValidation,
}

#[derive(Debug)]
struct TimedToken {
    rendered_text: String,
    word: Word,
}

#[derive(Debug)]
struct ImportedSegment {
    start: f64,
    end: f64,
    text: String,
    words: Vec<Word>,
}

#[derive(Debug)]
struct ValidatedWhisperTranscript {
    language: Option<String>,
    segments: Vec<ImportedSegment>,
}

struct TranscriptionImportBaseline<'a> {
    expected_version_id: &'a str,
    expected_source_path: &'a str,
    expected_source_sha256: &'a str,
    audio_duration: f64,
    confirm_replace: bool,
}

#[derive(Clone, Copy)]
struct TranscriptionTimingMode {
    mode: &'static str,
    vad_used: bool,
}

impl TranscriptionTimingMode {
    const fn no_vad() -> Self {
        Self {
            mode: "whisper_no_vad",
            vad_used: false,
        }
    }

    const fn verified_vad() -> Self {
        Self {
            mode: "whisper_verified_vad",
            vad_used: true,
        }
    }
}

fn timing_error(detail: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("transcription_timing_invalid: {detail}")
}

fn parse_timing_field(value: Option<&str>, label: &str) -> Result<f64> {
    let timestamp =
        parse_whisper_timestamp(value.unwrap_or("")).map_err(|_| timing_error(label))?;
    if !timestamp.is_finite() || timestamp < 0.0 {
        return Err(timing_error(label));
    }
    Ok(timestamp)
}

fn is_special_token(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("[_") || text.starts_with("<|")
}

fn is_punctuation_only(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && text.chars().all(|character| {
            character.is_ascii_punctuation()
                || matches!(
                    character,
                    '，' | '。'
                        | '！'
                        | '？'
                        | '；'
                        | '：'
                        | '、'
                        | '（'
                        | '）'
                        | '【'
                        | '】'
                        | '《'
                        | '》'
                        | '〈'
                        | '〉'
                        | '「'
                        | '」'
                        | '『'
                        | '』'
                        | '〔'
                        | '〕'
                        | '…'
                        | '—'
                        | '–'
                        | '·'
                        | '～'
                        | '“'
                        | '”'
                        | '‘'
                        | '’'
                )
        })
}

fn comparable_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn item_has_lexical_tokens(item: &Value) -> bool {
    item.get("tokens")
        .and_then(Value::as_array)
        .is_some_and(|tokens| {
            tokens.iter().any(|token| {
                let text = token
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                !text.is_empty() && !is_special_token(text) && !is_punctuation_only(text)
            })
        })
}

fn whisper_item_segments(item: &Value, audio_duration: f64) -> Result<Vec<ImportedSegment>> {
    let timestamps = item.get("timestamps").unwrap_or(&Value::Null);
    let item_start = parse_timing_field(
        timestamps.get("from").and_then(Value::as_str),
        "字幕段缺少有效开始时间，结果未应用",
    )?;
    let item_end = parse_timing_field(
        timestamps.get("to").and_then(Value::as_str),
        "字幕段缺少有效结束时间，结果未应用",
    )?;
    if item_end <= item_start {
        return Err(timing_error("字幕段结束时间不晚于开始时间，结果未应用"));
    }
    if item_end > audio_duration + AUDIO_DURATION_TOLERANCE_SECONDS {
        return Err(timing_error("字幕段超出标准化音频时长，结果未应用"));
    }
    let text = item
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if text.is_empty() && !item_has_lexical_tokens(item) {
        return Ok(Vec::new());
    }

    let token_values = item
        .get("tokens")
        .and_then(Value::as_array)
        .ok_or_else(|| timing_error("非空字幕段缺少词级时间，结果未应用"))?;
    let mut pending_text = String::new();
    let mut pending_has_lexical_content = false;
    let mut tokens: Vec<TimedToken> = Vec::new();
    let mut previous_word_start = None;
    let mut previous_word_end = None;
    for (token_index, token) in token_values.iter().enumerate() {
        let raw_text = token.get("text").and_then(Value::as_str).unwrap_or("");
        let word_text = raw_text.trim();
        if is_special_token(word_text) {
            continue;
        }
        if word_text.is_empty() {
            pending_text.push_str(raw_text);
            continue;
        }
        if is_punctuation_only(word_text) {
            if pending_has_lexical_content {
                pending_text.push_str(raw_text);
            } else if let Some(previous) = tokens.last_mut() {
                previous.rendered_text.push_str(raw_text);
                previous.word.text.push_str(word_text);
            } else {
                pending_text.push_str(raw_text);
            }
            continue;
        }
        let timestamps = token
            .get("timestamps")
            .ok_or_else(|| timing_error("词级内容缺少时间戳，结果未应用"))?;
        let word_start = parse_timing_field(
            timestamps.get("from").and_then(Value::as_str),
            "词级内容缺少有效开始时间，结果未应用",
        )?;
        let mut word_end = parse_timing_field(
            timestamps.get("to").and_then(Value::as_str),
            "词级内容缺少有效结束时间，结果未应用",
        )?;
        if word_end < word_start {
            return Err(timing_error("词级内容结束时间不晚于开始时间，结果未应用"));
        }
        if word_start + PARENT_SEGMENT_TOLERANCE_SECONDS < item_start
            || word_start > item_end + PARENT_SEGMENT_TOLERANCE_SECONDS
        {
            return Err(timing_error("词级时间不在所属字幕段附近，结果未应用"));
        }
        if word_start > audio_duration + AUDIO_DURATION_TOLERANCE_SECONDS {
            return Err(timing_error("词级时间超出标准化音频时长，结果未应用"));
        }
        if word_end == word_start {
            let mut next_lexical_start = None;
            for candidate in &token_values[token_index + 1..] {
                let candidate_text = candidate
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if candidate_text.is_empty()
                    || is_special_token(candidate_text)
                    || is_punctuation_only(candidate_text)
                {
                    continue;
                }
                let candidate_timestamps = candidate
                    .get("timestamps")
                    .ok_or_else(|| timing_error("词级内容缺少时间戳，结果未应用"))?;
                let candidate_start = parse_timing_field(
                    candidate_timestamps.get("from").and_then(Value::as_str),
                    "词级内容缺少有效开始时间，结果未应用",
                )?;
                let candidate_end = parse_timing_field(
                    candidate_timestamps.get("to").and_then(Value::as_str),
                    "词级内容缺少有效结束时间，结果未应用",
                )?;
                if candidate_end < candidate_start {
                    return Err(timing_error("词级内容结束时间不晚于开始时间，结果未应用"));
                }
                next_lexical_start = Some(candidate_start);
                break;
            }

            match next_lexical_start {
                Some(next_start) if next_start + TIMELINE_ORDER_TOLERANCE_SECONDS < word_start => {
                    return Err(timing_error("词级时间出现倒退，结果未应用"));
                }
                Some(next_start) if next_start <= word_start + TIMELINE_ORDER_TOLERANCE_SECONDS => {
                    pending_text.push_str(raw_text);
                    pending_has_lexical_content = true;
                    continue;
                }
                Some(next_start) => word_end = next_start,
                None if item_end > word_start => word_end = item_end,
                None => {
                    let merged_text = format!("{pending_text}{raw_text}");
                    if let Some(previous) = tokens.last_mut() {
                        previous.rendered_text.push_str(&merged_text);
                        previous.word.text.push_str(merged_text.trim_end());
                    } else if item_end > item_start {
                        tokens.push(TimedToken {
                            rendered_text: merged_text.clone(),
                            word: Word {
                                id: new_id("w"),
                                segment_id: String::new(),
                                start: item_start,
                                end: item_end,
                                text: merged_text.trim().to_owned(),
                                confidence: token.get("p").and_then(Value::as_f64),
                            },
                        });
                        previous_word_start = Some(item_start);
                        previous_word_end = Some(item_end);
                    } else {
                        return Err(timing_error("词级内容结束时间不晚于开始时间，结果未应用"));
                    }
                    pending_text.clear();
                    pending_has_lexical_content = false;
                    continue;
                }
            }
        }
        if word_start + PARENT_SEGMENT_TOLERANCE_SECONDS < item_start
            || word_end > item_end + PARENT_SEGMENT_TOLERANCE_SECONDS
        {
            return Err(timing_error("词级时间不在所属字幕段附近，结果未应用"));
        }
        if word_end > audio_duration + AUDIO_DURATION_TOLERANCE_SECONDS {
            return Err(timing_error("词级时间超出标准化音频时长，结果未应用"));
        }
        if previous_word_start
            .is_some_and(|previous| word_start + TIMELINE_ORDER_TOLERANCE_SECONDS < previous)
            || previous_word_end
                .is_some_and(|previous| word_end + TIMELINE_ORDER_TOLERANCE_SECONDS < previous)
        {
            return Err(timing_error("词级时间出现倒退，结果未应用"));
        }
        let rendered_text = format!("{pending_text}{raw_text}");
        let stored_text = rendered_text.trim().to_owned();
        pending_text.clear();
        pending_has_lexical_content = false;
        tokens.push(TimedToken {
            rendered_text,
            word: Word {
                id: new_id("w"),
                segment_id: String::new(),
                start: word_start,
                end: word_end,
                text: stored_text,
                confidence: token.get("p").and_then(Value::as_f64),
            },
        });
        previous_word_start = Some(word_start);
        previous_word_end = Some(word_end);
    }

    if tokens.is_empty() {
        return Err(timing_error("非空字幕段没有可信词级时间，结果未应用"));
    }
    if !pending_text.is_empty()
        && let Some(previous) = tokens.last_mut()
    {
        previous.rendered_text.push_str(&pending_text);
        if pending_has_lexical_content {
            previous.word.text.push_str(pending_text.trim_end());
        }
    }
    let reconstructed = tokens
        .iter()
        .map(|token| token.rendered_text.as_str())
        .collect::<String>();
    if comparable_text(&reconstructed) != comparable_text(text) {
        return Err(timing_error("词级内容不能完整还原字幕段文本，结果未应用"));
    }

    let mut groups: Vec<Vec<TimedToken>> = Vec::new();
    let mut current: Vec<TimedToken> = Vec::new();
    for token in tokens {
        if let Some(first) = current.first()
            && token.word.end - first.word.start > MAX_CAPTION_DURATION_SECONDS
        {
            groups.push(std::mem::take(&mut current));
        }
        current.push(token);
    }
    if !current.is_empty() {
        groups.push(current);
    }

    Ok(groups
        .into_iter()
        .filter_map(|group| {
            let start = group.first()?.word.start;
            let end = group.last()?.word.end;
            let text = group
                .iter()
                .map(|token| token.rendered_text.as_str())
                .collect::<String>()
                .trim()
                .to_owned();
            (!text.is_empty()).then(|| ImportedSegment {
                start,
                end,
                text,
                words: group.into_iter().map(|token| token.word).collect(),
            })
        })
        .collect())
}

fn validate_whisper_transcript(
    raw: &Value,
    audio_duration: f64,
) -> Result<ValidatedWhisperTranscript> {
    if !audio_duration.is_finite() || audio_duration <= 0.0 {
        return Err(timing_error("标准化音频时长无效，结果未应用"));
    }
    let entries = raw
        .get("transcription")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("无法识别 whisper.cpp JSON 格式"))?;
    let language = raw
        .pointer("/result/language")
        .and_then(Value::as_str)
        .map(|reported| {
            crate::model::reconcile_source_language(
                reported,
                entries
                    .iter()
                    .filter_map(|item| item.get("text").and_then(Value::as_str)),
            )
        });
    let mut previous_segment_start = None;
    let mut previous_segment_end = None;
    let mut segments: Vec<ImportedSegment> = Vec::new();
    let mut pending_prefix = String::new();
    for item in entries {
        let item_text = item
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let has_lexical_tokens = item_has_lexical_tokens(item);
        if !has_lexical_tokens && (item_text.is_empty() || is_special_token(item_text)) {
            continue;
        }
        if !has_lexical_tokens && is_punctuation_only(item_text) {
            if let Some(previous) = segments.last_mut() {
                previous.text.push_str(item_text);
                if let Some(word) = previous.words.last_mut() {
                    word.text.push_str(item_text);
                }
            } else {
                pending_prefix.push_str(item_text);
            }
            continue;
        }
        let timestamps = item.get("timestamps").unwrap_or(&Value::Null);
        let item_start = parse_timing_field(
            timestamps.get("from").and_then(Value::as_str),
            "字幕段缺少有效开始时间，结果未应用",
        )?;
        let item_end = parse_timing_field(
            timestamps.get("to").and_then(Value::as_str),
            "字幕段缺少有效结束时间，结果未应用",
        )?;
        if previous_segment_start
            .is_some_and(|previous| item_start + TIMELINE_ORDER_TOLERANCE_SECONDS < previous)
            || previous_segment_end
                .is_some_and(|previous| item_end + TIMELINE_ORDER_TOLERANCE_SECONDS < previous)
        {
            return Err(timing_error("字幕段时间出现倒退，结果未应用"));
        }
        let mut item_segments = whisper_item_segments(item, audio_duration)?;
        if !pending_prefix.is_empty()
            && let Some(first) = item_segments.first_mut()
        {
            first.text.insert_str(0, &pending_prefix);
            if let Some(word) = first.words.first_mut() {
                word.text.insert_str(0, &pending_prefix);
            }
            pending_prefix.clear();
        }
        segments.extend(item_segments);
        previous_segment_start = Some(item_start);
        previous_segment_end = Some(item_end);
    }

    let mut previous_word_start = None;
    let mut previous_word_end = None;
    for word in segments.iter().flat_map(|segment| segment.words.iter()) {
        if previous_word_start
            .is_some_and(|previous| word.start + TIMELINE_ORDER_TOLERANCE_SECONDS < previous)
            || previous_word_end
                .is_some_and(|previous| word.end + TIMELINE_ORDER_TOLERANCE_SECONDS < previous)
        {
            return Err(timing_error("跨字幕段的词级时间出现倒退，结果未应用"));
        }
        previous_word_start = Some(word.start);
        previous_word_end = Some(word.end);
    }

    Ok(ValidatedWhisperTranscript { language, segments })
}

#[cfg(test)]
fn import_whisper_json(
    db: &mut Connection,
    project_id: &str,
    json_path: &Path,
    audio_duration: f64,
    confirm_replace: bool,
) -> Result<(Project, usize)> {
    let project = project::load(db, project_id)?;
    let result = import_whisper_json_at_baseline(
        db,
        project_id,
        json_path,
        TranscriptionImportBaseline {
            expected_version_id: project
                .history
                .current_version_id
                .as_deref()
                .ok_or_else(|| anyhow!("project_version_missing: 项目没有可确认的当前版本"))?,
            expected_source_path: &project.media.source_path,
            expected_source_sha256: &project.media.sha256,
            audio_duration,
            confirm_replace,
        },
    )?;
    Ok((result.project, result.timing_validation.segment_count))
}

#[cfg(test)]
fn import_whisper_json_at_baseline(
    db: &mut Connection,
    project_id: &str,
    json_path: &Path,
    baseline: TranscriptionImportBaseline<'_>,
) -> Result<TranscriptionResult> {
    import_whisper_json_at_baseline_with_mode(
        db,
        project_id,
        json_path,
        baseline,
        TranscriptionTimingMode::no_vad(),
    )
}

fn import_whisper_json_at_baseline_with_mode(
    db: &mut Connection,
    project_id: &str,
    json_path: &Path,
    baseline: TranscriptionImportBaseline<'_>,
    timing_mode: TranscriptionTimingMode,
) -> Result<TranscriptionResult> {
    let raw: Value = serde_json::from_str(
        &fs::read_to_string(json_path).context("whisper.cpp 未生成 JSON 输出")?,
    )?;
    let validated = validate_whisper_transcript(&raw, baseline.audio_duration)?;
    let current = project::load(db, project_id)?;
    if current.history.current_version_id.as_deref() != Some(baseline.expected_version_id) {
        bail!("transcription_project_changed: 本地转录期间项目已被修改，结果未应用")
    }
    let replacing_transcript = !current.transcript.segments.is_empty();
    if replacing_transcript && !baseline.confirm_replace {
        bail!("transcription_apply_confirmation_required: 重新生成字幕前必须明确确认替换当前字幕")
    }
    if replacing_transcript && validated.segments.is_empty() {
        bail!("transcription_timing_invalid: 重新转录未识别到可信人声，当前字幕已保留")
    }
    if hash_file(Path::new(baseline.expected_source_path))? != baseline.expected_source_sha256 {
        bail!("transcription_source_changed: 本地转录期间原始媒体内容发生变化，结果未应用")
    }
    let segment_count = validated.segments.len();
    let word_count = validated
        .segments
        .iter()
        .map(|segment| segment.words.len())
        .sum();
    let ValidatedWhisperTranscript { language, segments } = validated;
    project::mutate_with_snapshot_at_version(
        db,
        project_id,
        Some(baseline.expected_version_id),
        "transcription_project_changed: 本地转录期间项目已被修改，结果未应用",
        "whisper.cpp 本地转录",
        |tx| {
            let recorded: (String, String) = tx.query_row(
                "SELECT source_path,sha256 FROM media WHERE project_id=?1",
                [project_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if recorded.0 != baseline.expected_source_path
                || recorded.1 != baseline.expected_source_sha256
            {
                bail!("transcription_source_changed: 本地转录期间项目媒体绑定发生变化，结果未应用")
            }
            if hash_file(Path::new(baseline.expected_source_path))?
                != baseline.expected_source_sha256
            {
                bail!("transcription_source_changed: 本地转录期间原始媒体内容发生变化，结果未应用")
            }
            project::assert_transcript_replacement_safe(tx, project_id)?;
            tx.execute("DELETE FROM segments WHERE project_id=?1", [project_id])?;
            for imported in segments {
                let values = imported
                    .words
                    .iter()
                    .filter_map(|word| word.confidence)
                    .collect::<Vec<_>>();
                let confidence =
                    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64);
                let segment = Segment {
                    id: new_id("s"),
                    start: imported.start,
                    end: imported.end,
                    text: imported.text,
                    confidence,
                };
                tx.execute("INSERT INTO segments(id,project_id,start_seconds,end_seconds,text,confidence) VALUES(?1,?2,?3,?4,?5,?6)",params![&segment.id,project_id,segment.start,segment.end,&segment.text,segment.confidence])?;
                for (ordinal, mut word) in imported.words.into_iter().enumerate() {
                    word.segment_id.clone_from(&segment.id);
                    tx.execute("INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,confidence,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![&word.id,project_id,&word.segment_id,word.start,word.end,&word.text,word.confidence,ordinal as i64])?;
                }
            }
            if let Some(language) = language {
                tx.execute(
                    "UPDATE projects SET source_language=?2 WHERE id=?1",
                    params![project_id, language],
                )?;
            }
            tx.execute(
                "UPDATE translations SET status='stale' WHERE project_id=?1",
                [project_id],
            )?;
            Ok(())
        },
    )?;
    Ok(TranscriptionResult {
        project: project::load(db, project_id)?,
        timing_validation: TimingValidation {
            status: "verified".into(),
            time_domain: "original_media".into(),
            mode: timing_mode.mode.into(),
            vad_used: timing_mode.vad_used,
            segment_count,
            word_count,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, export, project};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parses_whisper_times() {
        assert_eq!(parse_whisper_timestamp("00:01:02,500").unwrap(), 62.5)
    }

    #[test]
    fn rejects_subtle_vad_timeline_compression_without_a_drift_threshold() {
        let compressed = serde_json::json!({
            "transcription": [
                {
                    "timestamps": {"from":"00:00:00,200","to":"00:00:03,000"},
                    "text":" first",
                    "tokens":[{"text":" first","timestamps":{"from":"00:00:00,250","to":"00:00:02,800"}}]
                },
                {
                    "timestamps":{"from":"00:00:12,500","to":"00:00:16,060"},
                    "text":" last",
                    "tokens":[{"text":" last","timestamps":{"from":"00:00:09,000","to":"00:00:12,200"}}]
                }
            ]
        });

        let error = validate_whisper_transcript(&compressed, 16.77)
            .unwrap_err()
            .to_string();

        assert!(error.contains("transcription_timing_invalid"));
        assert!(error.contains("所属字幕段"));
    }

    #[test]
    fn accepts_word_timing_in_the_original_media_domain() {
        let aligned = serde_json::json!({
            "transcription": [
                {
                    "timestamps": {"from":"00:00:00,200","to":"00:00:03,000"},
                    "text":" first",
                    "tokens":[{"text":" first","timestamps":{"from":"00:00:00,250","to":"00:00:02,800"}}]
                },
                {
                    "timestamps":{"from":"00:00:12,500","to":"00:00:16,060"},
                    "text":" last",
                    "tokens":[{"text":" last","timestamps":{"from":"00:00:12,700","to":"00:00:15,800"}}]
                }
            ]
        });

        let validated = validate_whisper_transcript(&aligned, 16.77).unwrap();

        assert_eq!(validated.segments.len(), 2);
        assert_eq!(validated.segments[1].end, 15.8);
    }

    #[test]
    fn attaches_zero_duration_punctuation_without_storing_a_separate_word() {
        let item = serde_json::json!({
            "timestamps":{"from":"00:00:00,000","to":"00:00:01,500"},
            "text":"你好。",
            "tokens":[
                {"text":"你好","timestamps":{"from":"00:00:00,100","to":"00:00:01,200"}},
                {"text":"。","timestamps":{"from":"00:00:01,200","to":"00:00:01,200"}}
            ]
        });

        let segments = whisper_item_segments(&item, 2.0).unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "你好。");
        assert_eq!(segments[0].words.len(), 1);
        assert_eq!(segments[0].words[0].text, "你好。");
    }

    #[test]
    fn bounds_a_zero_duration_content_word_by_the_next_word() {
        let item = serde_json::json!({
            "timestamps":{"from":"00:00:04,400","to":"00:00:08,000"},
            "text":" The result.",
            "tokens":[
                {"text":" The","timestamps":{"from":"00:00:04,400","to":"00:00:04,400"}},
                {"text":" result","timestamps":{"from":"00:00:04,510","to":"00:00:07,900"}},
                {"text":".","timestamps":{"from":"00:00:08,000","to":"00:00:08,000"}}
            ]
        });

        let segments = whisper_item_segments(&item, 9.0).unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "The result.");
        assert_eq!(segments[0].words.len(), 2);
        assert_eq!(segments[0].words[0].start, 4.4);
        assert_eq!(segments[0].words[0].end, 4.51);
    }

    #[test]
    fn merges_a_zero_duration_word_into_the_next_word_with_the_same_start() {
        let item = serde_json::json!({
            "timestamps":{"from":"00:06:03,320","to":"00:06:04,000"},
            "text":" And or the best",
            "tokens":[
                {"text":" And","timestamps":{"from":"00:06:03,320","to":"00:06:03,560"}},
                {"text":" or","timestamps":{"from":"00:06:03,680","to":"00:06:03,680"}},
                {"text":" the","timestamps":{"from":"00:06:03,680","to":"00:06:03,780"}},
                {"text":" best","timestamps":{"from":"00:06:03,780","to":"00:06:04,000"}}
            ]
        });

        let segments = whisper_item_segments(&item, 365.0).unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "And or the best");
        assert_eq!(segments[0].words.len(), 3);
        assert_eq!(segments[0].words[1].text, "or the");
        assert_eq!(segments[0].words[1].start, 363.68);
        assert_eq!(segments[0].words[1].end, 363.78);
    }

    #[test]
    fn merges_a_terminal_zero_duration_word_into_the_previous_word() {
        let item = serde_json::json!({
            "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
            "text":" hello world",
            "tokens":[
                {"text":" hello","timestamps":{"from":"00:00:00,100","to":"00:00:00,800"}},
                {"text":" world","timestamps":{"from":"00:00:01,000","to":"00:00:01,000"}}
            ]
        });

        let segments = whisper_item_segments(&item, 2.0).unwrap();

        assert_eq!(segments[0].text, "hello world");
        assert_eq!(segments[0].words.len(), 1);
        assert_eq!(segments[0].words[0].text, "hello world");
        assert_eq!(segments[0].words[0].end, 0.8);
    }

    #[test]
    fn uses_the_parent_range_when_all_content_words_are_terminal_zero_duration() {
        let item = serde_json::json!({
            "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
            "text":" one two",
            "tokens":[
                {"text":" one","timestamps":{"from":"00:00:01,000","to":"00:00:01,000"}},
                {"text":" two","timestamps":{"from":"00:00:01,000","to":"00:00:01,000"}}
            ]
        });

        let segments = whisper_item_segments(&item, 2.0).unwrap();

        assert_eq!(segments[0].text, "one two");
        assert_eq!(segments[0].words.len(), 1);
        assert_eq!(segments[0].words[0].text, "one two");
        assert_eq!(segments[0].words[0].start, 0.0);
        assert_eq!(segments[0].words[0].end, 1.0);
    }

    #[test]
    fn rejects_a_terminal_zero_duration_word_outside_its_parent_item() {
        let item = serde_json::json!({
            "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
            "text":" hello stray",
            "tokens":[
                {"text":" hello","timestamps":{"from":"00:00:00,100","to":"00:00:00,800"}},
                {"text":" stray","timestamps":{"from":"00:00:02,000","to":"00:00:02,000"}}
            ]
        });

        let error = whisper_item_segments(&item, 3.0).unwrap_err().to_string();

        assert!(error.contains("transcription_timing_invalid"));
        assert!(error.contains("所属字幕段"));
    }

    #[test]
    fn ignores_empty_control_only_items_without_valid_segment_timing() {
        let raw = serde_json::json!({
            "transcription":[
                {
                    "text":"",
                    "tokens":[{"text":"[_BEG_]"}]
                },
                {
                    "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
                    "text":" hello",
                    "tokens":[{"text":" hello","timestamps":{"from":"00:00:00,100","to":"00:00:00,900"}}]
                }
            ]
        });

        let validated = validate_whisper_transcript(&raw, 2.0).unwrap();

        assert_eq!(validated.segments.len(), 1);
        assert_eq!(validated.segments[0].text, "hello");
    }

    #[test]
    fn attaches_a_punctuation_only_item_to_the_previous_timed_item() {
        let raw = serde_json::json!({
            "transcription":[
                {
                    "timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},
                    "text":" Hello",
                    "tokens":[{"text":" Hello","timestamps":{"from":"00:00:00,100","to":"00:00:00,900"}}]
                },
                {
                    "timestamps":{"from":"00:00:00,000","to":"00:00:00,000"},
                    "text":"。",
                    "tokens":[{"text":"。","timestamps":{"from":"00:00:00,000","to":"00:00:00,000"}}]
                },
                {
                    "timestamps":{"from":"00:00:01,000","to":"00:00:02,000"},
                    "text":" Next",
                    "tokens":[{"text":" Next","timestamps":{"from":"00:00:01,100","to":"00:00:01,900"}}]
                }
            ]
        });

        let validated = validate_whisper_transcript(&raw, 3.0).unwrap();

        assert_eq!(validated.segments.len(), 2);
        assert_eq!(validated.segments[0].text, "Hello。");
        assert_eq!(validated.segments[0].words[0].text, "Hello。");
    }

    #[test]
    fn accepts_a_single_long_word_for_quality_review_instead_of_rejecting_import() {
        let item = serde_json::json!({
            "timestamps":{"from":"00:00:00,000","to":"00:00:09,500"},
            "text":" elongated",
            "tokens":[
                {"text":" elongated","timestamps":{"from":"00:00:00,100","to":"00:00:09,100"}}
            ]
        });

        let segments = whisper_item_segments(&item, 10.0).unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "elongated");
        assert_eq!(segments[0].end - segments[0].start, 9.0);
    }

    #[test]
    fn imports_whisper_json_into_sqlite() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("core.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut db, &media, None).unwrap();
        let result = temp.path().join("result.json");
        fs::write(
            &result,
            r#"{"transcription":[{"timestamps":{"from":"00:00:00,000","to":"00:00:01,500"},"text":" hello ","tokens":[{"text":" hello ","timestamps":{"from":"00:00:00,100","to":"00:00:01,400"}}]}]}"#,
        )
        .unwrap();
        let (updated, count) =
            import_whisper_json(&mut db, &project.id, &result, 2.0, false).unwrap();
        assert_eq!(count, 1);
        assert_eq!(updated.transcript.segments[0].text, "hello");
        assert_eq!(updated.transcript.segments[0].end, 1.4);
        assert_eq!(updated.transcript.words.len(), 1);
    }

    #[test]
    fn local_transcription_never_overwrites_a_newer_project_version() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("stale-local-transcription.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let created = project::create(&mut db, &media, None).unwrap();
        let baseline_version = created.history.current_version_id.clone();
        project::add_segment(&mut db, &created.id, 0.0, 1.0, "human change".into(), None).unwrap();
        let result = temp.path().join("stale-result.json");
        fs::write(
            &result,
            r#"{"transcription":[{"timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},"text":" stale result","tokens":[{"text":" stale result","timestamps":{"from":"00:00:00,100","to":"00:00:00,900"}}]}]}"#,
        )
        .unwrap();

        let error = import_whisper_json_at_baseline(
            &mut db,
            &created.id,
            &result,
            TranscriptionImportBaseline {
                expected_version_id: baseline_version.as_deref().unwrap(),
                expected_source_path: &created.media.source_path,
                expected_source_sha256: &created.media.sha256,
                audio_duration: 2.0,
                confirm_replace: true,
            },
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("transcription_project_changed"));
        assert_eq!(
            project::load(&db, &created.id).unwrap().transcript.segments[0].text,
            "human change"
        );
    }

    #[test]
    fn local_retranscription_preserves_segments_with_edit_dependencies() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("dependent-local-transcription.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let created = project::create(&mut db, &media, None).unwrap();
        let segment =
            project::add_segment(&mut db, &created.id, 0.0, 1.0, "keep me".into(), None).unwrap();
        db.execute(
            "INSERT INTO edits(
                 id,project_id,kind,status,segment_id,start_seconds,end_seconds,reason,created_at
             ) VALUES('dependent-edit',?1,'semantic_cut','applied',?2,0,1,'keep','now')",
            params![&created.id, &segment.id],
        )
        .unwrap();
        let baseline = project::load(&db, &created.id).unwrap();
        let preflight = project::transcript_replacement_preflight(&db, &created.id).unwrap();
        assert!(!preflight.can_replace);
        assert_eq!(
            preflight.current_version_id,
            baseline.history.current_version_id.as_deref().unwrap()
        );
        assert_eq!(preflight.blockers.edits, 1);
        assert_eq!(preflight.blockers.patch_items, 0);
        assert_eq!(preflight.blockers.task_segments, 0);
        let result = temp.path().join("replacement.json");
        fs::write(
            &result,
            r#"{"transcription":[{"timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},"text":" replacement","tokens":[{"text":" replacement","timestamps":{"from":"00:00:00,100","to":"00:00:00,900"}}]}]}"#,
        )
        .unwrap();

        let error = import_whisper_json_at_baseline(
            &mut db,
            &created.id,
            &result,
            TranscriptionImportBaseline {
                expected_version_id: baseline.history.current_version_id.as_deref().unwrap(),
                expected_source_path: &created.media.source_path,
                expected_source_sha256: &created.media.sha256,
                audio_duration: 2.0,
                confirm_replace: true,
            },
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("transcription_replacement_conflict"));
        assert_eq!(
            project::load(&db, &created.id).unwrap().transcript.segments[0].text,
            "keep me"
        );
    }

    #[test]
    fn invalid_word_timing_never_replaces_an_existing_transcript() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("invalid-timing.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let created = project::create(&mut db, &media, None).unwrap();
        project::add_segment(&mut db, &created.id, 0.0, 1.0, "keep me".into(), None).unwrap();
        let baseline = project::load(&db, &created.id).unwrap();
        let result = temp.path().join("compressed.json");
        fs::write(
            &result,
            r#"{"transcription":[{"timestamps":{"from":"00:00:12,500","to":"00:00:16,060"},"text":" replacement","tokens":[{"text":" replacement","timestamps":{"from":"00:00:09,000","to":"00:00:12,200"}}]}]}"#,
        )
        .unwrap();

        let error = import_whisper_json_at_baseline(
            &mut db,
            &created.id,
            &result,
            TranscriptionImportBaseline {
                expected_version_id: baseline.history.current_version_id.as_deref().unwrap(),
                expected_source_path: &created.media.source_path,
                expected_source_sha256: &created.media.sha256,
                audio_duration: 16.77,
                confirm_replace: true,
            },
        )
        .unwrap_err()
        .to_string();

        let preserved = project::load(&db, &created.id).unwrap();
        assert!(error.contains("transcription_timing_invalid"));
        assert_eq!(preserved.transcript.segments[0].text, "keep me");
        assert_eq!(
            preserved.history.current_version_id,
            baseline.history.current_version_id
        );
    }

    #[test]
    fn empty_retranscription_preserves_existing_subtitles() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("empty-retranscription.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let created = project::create(&mut db, &media, None).unwrap();
        project::add_segment(&mut db, &created.id, 0.0, 1.0, "keep me".into(), None).unwrap();
        let baseline = project::load(&db, &created.id).unwrap();
        let result = temp.path().join("empty.json");
        fs::write(&result, r#"{"transcription":[]}"#).unwrap();

        let error = import_whisper_json_at_baseline(
            &mut db,
            &created.id,
            &result,
            TranscriptionImportBaseline {
                expected_version_id: baseline.history.current_version_id.as_deref().unwrap(),
                expected_source_path: &created.media.source_path,
                expected_source_sha256: &created.media.sha256,
                audio_duration: 2.0,
                confirm_replace: true,
            },
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("当前字幕已保留"));
        assert_eq!(
            project::load(&db, &created.id).unwrap().transcript.segments[0].text,
            "keep me"
        );
    }

    #[test]
    fn empty_first_transcription_is_a_verified_no_speech_result() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("empty-first-transcription.db")).unwrap();
        let media = temp.path().join("silence.wav");
        fs::write(&media, b"audio").unwrap();
        let created = project::create(&mut db, &media, None).unwrap();
        let result = temp.path().join("empty.json");
        fs::write(&result, r#"{"transcription":[]}"#).unwrap();

        let imported = import_whisper_json_at_baseline(
            &mut db,
            &created.id,
            &result,
            TranscriptionImportBaseline {
                expected_version_id: created.history.current_version_id.as_deref().unwrap(),
                expected_source_path: &created.media.source_path,
                expected_source_sha256: &created.media.sha256,
                audio_duration: 2.0,
                confirm_replace: false,
            },
        )
        .unwrap();

        assert!(imported.project.transcript.segments.is_empty());
        assert_eq!(imported.timing_validation.status, "verified");
        assert_eq!(imported.timing_validation.segment_count, 0);
        assert_eq!(imported.timing_validation.word_count, 0);
        assert!(!imported.timing_validation.vad_used);
    }

    #[test]
    fn records_verified_vad_as_the_applied_timing_mode() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("verified-vad-mode.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let created = project::create(&mut db, &media, None).unwrap();
        let result = temp.path().join("verified-vad.json");
        fs::write(
            &result,
            r#"{"transcription":[{"timestamps":{"from":"00:00:00,000","to":"00:00:01,000"},"text":" verified","tokens":[{"text":" verified","timestamps":{"from":"00:00:00,100","to":"00:00:00,900"}}]}]}"#,
        )
        .unwrap();

        let imported = import_whisper_json_at_baseline_with_mode(
            &mut db,
            &created.id,
            &result,
            TranscriptionImportBaseline {
                expected_version_id: created.history.current_version_id.as_deref().unwrap(),
                expected_source_path: &created.media.source_path,
                expected_source_sha256: &created.media.sha256,
                audio_duration: 2.0,
                confirm_replace: false,
            },
            TranscriptionTimingMode::verified_vad(),
        )
        .unwrap();

        assert_eq!(imported.timing_validation.mode, "whisper_verified_vad");
        assert!(imported.timing_validation.vad_used);
        assert_eq!(imported.timing_validation.time_domain, "original_media");
    }

    #[test]
    fn imports_word_timestamps_and_confidence() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("words.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut db, &media, None).unwrap();
        let result = temp.path().join("result-full.json");
        fs::write(
            &result,
            r#"{"result":{"language":"en"},"transcription":[{"timestamps":{"from":"00:00:00,500","to":"00:00:01,500"},"text":" hello world","tokens":[{"text":"[_BEG_]","timestamps":{"from":"00:00:00,000","to":"00:00:00,000"},"p":0.9},{"text":" hello","timestamps":{"from":"00:00:00,100","to":"00:00:00,600"},"p":0.8},{"text":" world","timestamps":{"from":"00:00:00,700","to":"00:00:01,300"},"p":0.6}]}]}"#,
        )
        .unwrap();
        let (updated, count) =
            import_whisper_json(&mut db, &project.id, &result, 2.0, false).unwrap();
        assert_eq!(count, 1);
        assert_eq!(updated.transcript.source_language, "en");
        assert_eq!(updated.transcript.words.len(), 2);
        assert_eq!(updated.transcript.words[0].text, "hello");
        assert_eq!(updated.transcript.segments[0].start, 0.1);
        assert_eq!(updated.transcript.segments[0].end, 1.3);
        assert!(updated.transcript.words.iter().all(|word| {
            word.start >= updated.transcript.segments[0].start
                && word.end <= updated.transcript.segments[0].end
        }));
        assert!((updated.transcript.segments[0].confidence.unwrap() - 0.7).abs() < 0.001);
    }

    #[test]
    fn corrects_a_strong_english_transcript_mislabeled_as_chinese() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("language.db")).unwrap();
        let media = temp.path().join("english.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut db, &media, None).unwrap();
        let result = temp.path().join("language.json");
        fs::write(
            &result,
            r#"{"result":{"language":"zh"},"transcription":[{"timestamps":{"from":"00:00:00,000","to":"00:00:04,000"},"text":"What racing reveals about working with artificial intelligence and how teams use data to improve performance.","tokens":[{"text":"What racing reveals about working with artificial intelligence and how teams use data to improve performance.","timestamps":{"from":"00:00:00,100","to":"00:00:03,900"}}]}]}"#,
        )
        .unwrap();

        let (updated, _) = import_whisper_json(&mut db, &project.id, &result, 5.0, false).unwrap();

        assert_eq!(updated.transcript.source_language, "en");
    }

    #[test]
    fn splits_a_long_whisper_caption_at_word_boundaries_before_export() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("long-caption.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut db, &media, None).unwrap();
        let result = temp.path().join("long-caption.json");
        fs::write(
            &result,
            r#"{"result":{"language":"zh"},"transcription":[{"timestamps":{"from":"00:00:00,000","to":"00:00:24,200"},"text":"請你好呀 請你好呀","tokens":[{"text":"請","timestamps":{"from":"00:00:00,060","to":"00:00:07,680"},"p":0.01},{"text":"你","timestamps":{"from":"00:00:07,680","to":"00:00:15,360"},"p":0.09},{"text":"好","timestamps":{"from":"00:00:15,360","to":"00:00:23,040"},"p":0.56},{"text":"呀","timestamps":{"from":"00:00:23,040","to":"00:00:23,270"},"p":0.70},{"text":" ","timestamps":{"from":"00:00:23,270","to":"00:00:23,270"},"p":0.34},{"text":"請","timestamps":{"from":"00:00:23,270","to":"00:00:23,500"},"p":0.96},{"text":"你","timestamps":{"from":"00:00:23,500","to":"00:00:23,730"},"p":0.92},{"text":"好","timestamps":{"from":"00:00:23,730","to":"00:00:23,960"},"p":0.99},{"text":"呀","timestamps":{"from":"00:00:23,960","to":"00:00:24,090"},"p":0.99}]}]}"#,
        )
        .unwrap();

        let (updated, count) =
            import_whisper_json(&mut db, &project.id, &result, 25.0, false).unwrap();

        assert_eq!(count, 4);
        assert!(
            updated
                .transcript
                .segments
                .iter()
                .all(|segment| segment.end - segment.start <= MAX_CAPTION_DURATION_SECONDS)
        );
        assert_eq!(
            updated
                .transcript
                .segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect::<String>(),
            "請你好呀請你好呀"
        );
        assert_eq!(updated.transcript.words.len(), 8);
        assert!(
            export::audit(&updated)["issues"]
                .as_array()
                .unwrap()
                .iter()
                .all(|issue| issue["code"] != "caption-too-long")
        );
    }

    #[test]
    fn rejects_nonempty_captions_without_word_evidence() {
        let item = serde_json::json!({
            "timestamps":{"from":"00:00:00,000","to":"00:00:24,000"},
            "text":"这是没有词级时间的长字幕"
        });

        let error = whisper_item_segments(&item, 25.0).unwrap_err().to_string();

        assert!(error.contains("transcription_timing_invalid"));
        assert!(error.contains("缺少词级时间"));
    }
}
