use crate::{
    db::home_dir,
    model::{Project, Segment, Word},
    project,
    util::{hidden_command, new_id},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, params};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};

const VAD_RETRY_MIN_MEAN_VOLUME_DBFS: f64 = -55.0;

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
) -> Result<(Project, usize)> {
    if !model.is_file() {
        bail!("模型不存在：{}", model.display())
    }
    let project = project::load(db, project_id)?;
    let audio_dir = home_dir().join("cache").join("asr");
    fs::create_dir_all(&audio_dir)?;
    let wav = audio_dir.join(format!("{}.wav", project.id));
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

    let whisper = whisper_cli_path();
    let output_base = audio_dir.join(&project.id);
    let vad_model = whisper_vad_model_path();
    run_whisper(
        &whisper,
        model,
        &wav,
        &output_base,
        language,
        vad_model.as_deref(),
    )?;
    if vad_model.is_some()
        && whisper_transcription_is_empty(&output_base.with_extension("json"))?
        && audio_has_retryable_signal(&ffmpeg, &wav)?
    {
        run_whisper(&whisper, model, &wav, &output_base, language, None)?;
    }
    import_whisper_json(db, &project.id, &output_base.with_extension("json"))
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

fn whisper_transcription_is_empty(json_path: &Path) -> Result<bool> {
    let raw: Value = serde_json::from_str(
        &fs::read_to_string(json_path).context("whisper.cpp 未生成 JSON 输出")?,
    )?;
    Ok(raw
        .get("transcription")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty))
}

fn audio_has_retryable_signal(ffmpeg: &str, wav: &Path) -> Result<bool> {
    let result = hidden_command(ffmpeg)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(wav)
        .args(["-af", "volumedetect", "-f", "null", "-"])
        .output()
        .with_context(|| format!("无法启动 FFmpeg 音量检查：{ffmpeg}"))?;
    if !result.status.success() {
        bail!(
            "FFmpeg 音量检查失败：{}",
            String::from_utf8_lossy(&result.stderr).trim()
        )
    }
    Ok(
        parse_mean_volume_dbfs(&String::from_utf8_lossy(&result.stderr))
            .is_some_and(|value| value > VAD_RETRY_MIN_MEAN_VOLUME_DBFS),
    )
}

fn parse_mean_volume_dbfs(output: &str) -> Option<f64> {
    output.lines().rev().find_map(|line| {
        let value = line.split("mean_volume:").nth(1)?.trim();
        let numeric = value.split_whitespace().next()?;
        numeric.parse::<f64>().ok()
    })
}

const MAX_CAPTION_DURATION_SECONDS: f64 = 8.0;

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

fn fallback_text_segments(start: f64, end: f64, text: &str) -> Vec<ImportedSegment> {
    let characters = text.chars().collect::<Vec<_>>();
    if characters.is_empty() {
        return Vec::new();
    }
    let required = ((end - start) / MAX_CAPTION_DURATION_SECONDS)
        .ceil()
        .max(1.0) as usize;
    let chunk_count = required.min(characters.len());
    (0..chunk_count)
        .filter_map(|index| {
            let from = index * characters.len() / chunk_count;
            let to = (index + 1) * characters.len() / chunk_count;
            let text = characters[from..to].iter().collect::<String>();
            let segment_start = start + (end - start) * index as f64 / chunk_count as f64;
            let natural_end = start + (end - start) * (index + 1) as f64 / chunk_count as f64;
            let segment_end = natural_end.min(segment_start + MAX_CAPTION_DURATION_SECONDS);
            (!text.trim().is_empty()).then(|| ImportedSegment {
                start: segment_start,
                end: segment_end,
                text: text.trim().to_owned(),
                words: Vec::new(),
            })
        })
        .collect()
}

fn whisper_item_segments(item: &Value) -> Result<Vec<ImportedSegment>> {
    let timestamps = item.get("timestamps").unwrap_or(&Value::Null);
    let item_start =
        parse_whisper_timestamp(timestamps.get("from").and_then(Value::as_str).unwrap_or(""))?;
    let item_end =
        parse_whisper_timestamp(timestamps.get("to").and_then(Value::as_str).unwrap_or(""))?;
    let text = item
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }

    let mut pending_text = String::new();
    let mut tokens = Vec::new();
    for token in item
        .get("tokens")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let raw_text = token.get("text").and_then(Value::as_str).unwrap_or("");
        let word_text = raw_text.trim();
        if word_text.starts_with("[_") || word_text.starts_with("<|") {
            continue;
        }
        if word_text.is_empty() {
            pending_text.push_str(raw_text);
            continue;
        }
        let Some(timestamps) = token.get("timestamps") else {
            continue;
        };
        let Some(from) = timestamps.get("from").and_then(Value::as_str) else {
            continue;
        };
        let Some(to) = timestamps.get("to").and_then(Value::as_str) else {
            continue;
        };
        let Ok(word_start) = parse_whisper_timestamp(from) else {
            continue;
        };
        let Ok(word_end) = parse_whisper_timestamp(to) else {
            continue;
        };
        if word_end <= word_start {
            continue;
        }
        let rendered_text = format!("{pending_text}{raw_text}");
        pending_text.clear();
        tokens.push(TimedToken {
            rendered_text,
            word: Word {
                id: new_id("w"),
                segment_id: String::new(),
                start: word_start,
                end: word_end,
                text: word_text.to_owned(),
                confidence: token.get("p").and_then(Value::as_f64),
            },
        });
    }

    if tokens.is_empty()
        || tokens
            .iter()
            .any(|token| token.word.end - token.word.start > MAX_CAPTION_DURATION_SECONDS)
    {
        return Ok(fallback_text_segments(item_start, item_end, text));
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

fn import_whisper_json(
    db: &mut Connection,
    project_id: &str,
    json_path: &Path,
) -> Result<(Project, usize)> {
    let raw: Value = serde_json::from_str(
        &fs::read_to_string(json_path).context("whisper.cpp 未生成 JSON 输出")?,
    )?;
    let entries = raw
        .get("transcription")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("无法识别 whisper.cpp JSON 格式"))?;
    let language = raw
        .pointer("/result/language")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let tx = db.transaction()?;
    tx.execute("DELETE FROM segments WHERE project_id=?1", [project_id])?;
    let mut count = 0;
    for item in entries {
        for imported in whisper_item_segments(item)? {
            let confidence = if imported.words.is_empty() {
                None
            } else {
                let values = imported
                    .words
                    .iter()
                    .filter_map(|word| word.confidence)
                    .collect::<Vec<_>>();
                (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
            };
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
            count += 1;
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
    tx.commit()?;
    project::snapshot(db, project_id, "whisper.cpp 本地转录")?;
    Ok((project::load(db, project_id)?, count))
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
    fn detects_an_empty_vad_transcription_for_safe_retry() {
        let temp = tempdir().unwrap();
        let empty = temp.path().join("empty.json");
        let populated = temp.path().join("populated.json");
        fs::write(&empty, r#"{"transcription":[]}"#).unwrap();
        fs::write(&populated, r#"{"transcription":[{"text":"speech"}]}"#).unwrap();
        assert!(whisper_transcription_is_empty(&empty).unwrap());
        assert!(!whisper_transcription_is_empty(&populated).unwrap());
    }

    #[test]
    fn vad_retry_requires_audible_signal_instead_of_digital_silence() {
        let audible = "[Parsed_volumedetect_0] mean_volume: -27.4 dB";
        let silence = "[Parsed_volumedetect_0] mean_volume: -91.0 dB";
        let negative_infinity = "[Parsed_volumedetect_0] mean_volume: -inf dB";
        assert_eq!(parse_mean_volume_dbfs(audible), Some(-27.4));
        assert_eq!(parse_mean_volume_dbfs(silence), Some(-91.0));
        assert_eq!(
            parse_mean_volume_dbfs(negative_infinity),
            Some(f64::NEG_INFINITY)
        );
        assert!(parse_mean_volume_dbfs(audible).unwrap() > VAD_RETRY_MIN_MEAN_VOLUME_DBFS);
        assert!(parse_mean_volume_dbfs(silence).unwrap() <= VAD_RETRY_MIN_MEAN_VOLUME_DBFS);
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
            r#"{"transcription":[{"timestamps":{"from":"00:00:00,000","to":"00:00:01,500"},"text":" hello "}]}"#,
        )
        .unwrap();
        let (updated, count) = import_whisper_json(&mut db, &project.id, &result).unwrap();
        assert_eq!(count, 1);
        assert_eq!(updated.transcript.segments[0].text, "hello");
        assert_eq!(updated.transcript.segments[0].end, 1.5);
        assert!(updated.transcript.words.is_empty());
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
        let (updated, count) = import_whisper_json(&mut db, &project.id, &result).unwrap();
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

        let (updated, count) = import_whisper_json(&mut db, &project.id, &result).unwrap();

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
    fn bounds_long_captions_without_word_evidence() {
        let item = serde_json::json!({
            "timestamps":{"from":"00:00:00,000","to":"00:00:24,000"},
            "text":"这是没有词级时间的长字幕"
        });

        let segments = whisper_item_segments(&item).unwrap();

        assert!(segments.len() >= 3);
        assert!(segments.iter().all(|segment| {
            !segment.text.is_empty()
                && segment.end - segment.start <= MAX_CAPTION_DURATION_SECONDS
                && segment.words.is_empty()
        }));
    }
}
