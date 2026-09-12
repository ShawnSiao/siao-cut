use super::*;
use serde_json::Value;

pub(super) const PROVIDER: &str = "whisper_local";

pub(super) fn transcribe(job: &TranscriptionJob, wav: &Path) -> Result<String> {
    let model = Path::new(&job.model_id);
    if !model.is_file() {
        bail!("transcription_model_missing: 本地转写模型不存在")
    }
    let duration = crate::media::ffprobe_duration(wav)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| anyhow!("transcription_timing_invalid: 无法核实音频时长"))?;
    let output = wav.with_extension("transcript");
    let json_path = output.with_extension("transcript.json");
    let _guard = TemporaryFile::new(json_path.clone());
    let mut command = hidden_command(crate::media::whisper_cli_path());
    command
        .arg("-m")
        .arg(model)
        .arg("-f")
        .arg(wav)
        .args(["-ojf", "-sow", "-ml", "60", "-of"])
        .arg(&output);
    if let Some(language) = &job.language {
        command.args(["-l", language]);
    }
    let result = command
        .output()
        .context("transcription_provider_unavailable: 无法启动 Whisper")?;
    if !result.status.success() {
        bail!(
            "transcription_model_failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )
    }
    let raw: Value = serde_json::from_str(&fs::read_to_string(json_path)?)?;
    Ok(crate::media::normalized_whisper_candidate(&raw, duration)?.to_string())
}

pub(super) fn import_words(
    db: &Connection,
    project_id: &str,
    segments: &[ImportedSegment],
    raw: &str,
) -> Result<()> {
    let value: Value = serde_json::from_str(raw)?;
    let entries = value["segments"]
        .as_array()
        .ok_or_else(|| anyhow!("transcription_response_invalid: 缺少字幕段"))?;
    if entries.len() != segments.len() {
        bail!("transcription_response_invalid: 候选字幕段数量不一致")
    }
    for (segment, entry) in segments.iter().zip(entries) {
        let words: Vec<crate::model::Word> = serde_json::from_value(entry["words"].clone())?;
        for (ordinal, word) in words.into_iter().enumerate() {
            db.execute("INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,confidence,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![new_id("w"), project_id, segment.id, word.start, word.end, word.text, word.confidence, ordinal as i64])?;
        }
    }
    if let Some(language) = value["language"].as_str() {
        db.execute(
            "UPDATE projects SET source_language=?2 WHERE id=?1",
            params![project_id, language],
        )?;
    }
    Ok(())
}
