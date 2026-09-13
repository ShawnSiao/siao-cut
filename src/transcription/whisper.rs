use super::*;
use serde_json::Value;

pub(super) const PROVIDER: &str = "whisper_local";

pub(super) fn transcribe(job: &TranscriptionJob, wav: &Path) -> Result<String> {
    let output = crate::media::transcribe_whisper_audio(
        Path::new(&job.model_id),
        wav,
        job.language.as_deref(),
    )?;
    Ok(crate::media::normalized_whisper_candidate(
        &output.raw,
        output.duration,
        output.timing_mode,
    )?
    .to_string())
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
