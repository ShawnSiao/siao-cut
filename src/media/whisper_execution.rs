//! Shared execution policy for synchronous CLI and background desktop transcription.
use super::*;
use std::process::Command;

pub(crate) struct WhisperOutput {
    pub raw: Value,
    pub duration: f64,
    pub timing_mode: TranscriptionTimingMode,
}

pub(crate) fn transcribe_whisper_audio(
    model: &Path,
    wav: &Path,
    language: Option<&str>,
) -> Result<WhisperOutput> {
    if !model.is_file() {
        bail!("transcription_model_missing: 本地转写模型不存在")
    }
    let duration = ffprobe_duration(wav)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| anyhow!("transcription_timing_invalid: 无法核实音频时长"))?;
    // Reject a changed selected runtime instead of silently selecting another executable.
    let (whisper, backend) = resolved_whisper_runtime()?;
    let capability = crate::runtime::vad_timeline_capability(Path::new(&whisper), &backend);
    let vad_model = capability.verified.then(whisper_vad_model_path).flatten();
    let timing_mode = if vad_model.is_some() {
        TranscriptionTimingMode::verified_vad()
    } else {
        TranscriptionTimingMode::no_vad()
    };
    let directory = TemporaryRunDirectory::create(
        wav.parent()
            .ok_or_else(|| anyhow!("transcription_input_invalid: 音频路径缺少父目录"))?
            .join(new_id("whisper")),
    )?;
    let output_base = directory.0.join("transcript");
    let result = whisper_command(
        &whisper,
        model,
        wav,
        &output_base,
        language,
        vad_model.as_deref(),
    )
    .output()
    .context("transcription_provider_unavailable: 无法启动 Whisper")?;
    if !result.status.success() {
        bail!(
            "transcription_model_failed: whisper.cpp 转录失败（退出码 {:?}）：{}",
            result.status.code(),
            String::from_utf8_lossy(&result.stderr).trim()
        )
    }
    let raw = serde_json::from_slice(
        &fs::read(output_base.with_extension("json")).context("whisper.cpp 未生成 JSON 输出")?,
    )?;
    Ok(WhisperOutput {
        raw,
        duration,
        timing_mode,
    })
}

fn whisper_command(
    whisper: &str,
    model: &Path,
    wav: &Path,
    output_base: &Path,
    language: Option<&str>,
    vad_model: Option<&str>,
) -> Command {
    let mut command = hidden_command(whisper);
    command
        .arg("-m")
        .arg(model)
        .arg("-f")
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
    command
}

#[cfg(test)]
#[path = "tests/whisper_execution.rs"]
mod tests;
