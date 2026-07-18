use crate::{
    db,
    media::tool_path,
    project,
    util::{hidden_command, new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path, process::Stdio, thread, time::Duration};

pub const ANALYZER_VERSION: &str = "ffmpeg-audio-v1";
pub const SILENCE_NOISE_DB: f64 = -40.0;
pub const SILENCE_MIN_SECONDS: f64 = 0.8;
pub const CLIPPING_PEAK_DBFS: f64 = -0.1;
pub const QUIET_LOUDNESS_LUFS: f64 = -24.0;
pub const LOUD_LOUDNESS_LUFS: f64 = -14.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioAnalysisThresholds {
    pub silence_noise_db: f64,
    pub silence_min_seconds: f64,
    pub clipping_peak_dbfs: f64,
    pub quiet_loudness_lufs: f64,
    pub loud_loudness_lufs: f64,
}

impl Default for AudioAnalysisThresholds {
    fn default() -> Self {
        Self {
            silence_noise_db: SILENCE_NOISE_DB,
            silence_min_seconds: SILENCE_MIN_SECONDS,
            clipping_peak_dbfs: CLIPPING_PEAK_DBFS,
            quiet_loudness_lufs: QUIET_LOUDNESS_LUFS,
            loud_loudness_lufs: LOUD_LOUDNESS_LUFS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioRiskKind {
    Silence,
    SuspectedClipping,
    LoudnessLow,
    LoudnessHigh,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRisk {
    pub kind: AudioRiskKind,
    pub start: f64,
    pub end: f64,
    pub measured_value: f64,
    pub threshold: f64,
    pub unit: String,
    pub tool_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioAnalysisReport {
    pub analyzer_version: String,
    pub tool_version: String,
    pub duration_seconds: f64,
    pub integrated_loudness_lufs: Option<f64>,
    pub true_peak_dbfs: Option<f64>,
    pub silence_duration_seconds: f64,
    pub thresholds: AudioAnalysisThresholds,
    pub risks: Vec<AudioRisk>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioAnalysisJob {
    pub id: String,
    pub project_id: String,
    pub status: String,
    pub progress: f64,
    pub report: Option<AudioAnalysisReport>,
    pub cancel_requested_at: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub worker_pid: Option<u32>,
    pub attempt_count: u32,
}

pub fn start(
    db: &Connection,
    project_id: &str,
    start_delay_ms: Option<u64>,
) -> Result<AudioAnalysisJob> {
    let project = project::load(db, project_id)?;
    let source = Path::new(&project.media.source_path)
        .canonicalize()
        .context("无法读取项目关联的原始媒体")?;
    if !source.is_file() {
        bail!("audio_source_missing: 项目关联的原始媒体不存在")
    }
    if let Some(job) = latest_active(db, project_id)? {
        return Ok(job);
    }
    let timestamp = now();
    let id = new_id("audio");
    db.execute(
        "INSERT INTO audio_analysis_jobs(id,project_id,status,progress,created_at,updated_at,attempt_count) VALUES(?1,?2,'queued',0,?3,?3,1)",
        params![id, project_id, timestamp],
    )?;
    if let Err(error) = spawn_worker(&id, start_delay_ms) {
        let failed_at = now();
        db.execute(
            "UPDATE audio_analysis_jobs SET status='failed',error_message=?2,completed_at=?3,updated_at=?3 WHERE id=?1",
            params![id, error.to_string(), failed_at],
        )?;
        return Err(error);
    }
    load(db, &id)
}

pub fn load(db: &Connection, job_id: &str) -> Result<AudioAnalysisJob> {
    db.query_row(
        "SELECT id,project_id,status,progress,report_json,cancel_requested_at,error_message,created_at,updated_at,completed_at,worker_pid,attempt_count FROM audio_analysis_jobs WHERE id=?1",
        [job_id],
        |row| {
            let report: Option<String> = row.get(4)?;
            Ok(AudioAnalysisJob {
                id: row.get(0)?,
                project_id: row.get(1)?,
                status: row.get(2)?,
                progress: row.get(3)?,
                report: report.and_then(|value| serde_json::from_str(&value).ok()),
                cancel_requested_at: row.get(5)?,
                error_message: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                completed_at: row.get(9)?,
                worker_pid: row.get(10)?,
                attempt_count: row.get::<_, i64>(11)? as u32,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("audio_job_not_found: 音频分析任务不存在：{job_id}"))
}

pub fn latest(db: &Connection, project_id: &str) -> Result<Option<AudioAnalysisJob>> {
    db.query_row(
        "SELECT id FROM audio_analysis_jobs WHERE project_id=?1 ORDER BY created_at DESC LIMIT 1",
        [project_id],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|id| load(db, &id))
    .transpose()
}

fn latest_active(db: &Connection, project_id: &str) -> Result<Option<AudioAnalysisJob>> {
    db.query_row(
        "SELECT id FROM audio_analysis_jobs WHERE project_id=?1 AND status IN ('queued','running') ORDER BY created_at DESC LIMIT 1",
        [project_id],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|id| load(db, &id))
    .transpose()
}

pub fn cancel(db: &Connection, job_id: &str) -> Result<AudioAnalysisJob> {
    let job = load(db, job_id)?;
    if !["queued", "running"].contains(&job.status.as_str()) {
        bail!("audio_job_not_cancellable: 当前音频分析任务不能取消")
    }
    if let Some(worker_pid) = job.worker_pid
        && worker_pid != std::process::id()
        && crate::util::process_is_active(worker_pid)
    {
        let _ = crate::util::terminate_process_tree_by_id(worker_pid);
    }
    let cancelled_at = now();
    db.execute(
        "UPDATE audio_analysis_jobs SET status='cancelled',cancel_requested_at=?2,worker_pid=NULL,completed_at=?2,updated_at=?2 WHERE id=?1",
        params![job_id, cancelled_at],
    )?;
    load(db, job_id)
}

pub fn resume(db: &Connection, job_id: &str) -> Result<AudioAnalysisJob> {
    let job = load(db, job_id)?;
    if !["cancelled", "failed", "interrupted"].contains(&job.status.as_str()) {
        bail!("audio_job_not_resumable: 当前音频分析任务不能继续")
    }
    db.execute(
        "UPDATE audio_analysis_jobs SET status='queued',progress=0,report_json=NULL,cancel_requested_at=NULL,error_message=NULL,completed_at=NULL,worker_pid=NULL,attempt_count=attempt_count+1,updated_at=?2 WHERE id=?1",
        params![job_id, now()],
    )?;
    if let Err(error) = spawn_worker(job_id, None).context("无法继续本地音频分析") {
        let failed_at = now();
        db.execute(
            "UPDATE audio_analysis_jobs SET status='failed',error_message=?2,completed_at=?3,updated_at=?3 WHERE id=?1",
            params![job_id, error.to_string(), failed_at],
        )?;
        return Err(error);
    }
    load(db, job_id)
}

pub fn reconcile_interrupted(db: &Connection) -> Result<()> {
    let jobs = db
        .prepare("SELECT id,worker_pid,updated_at FROM audio_analysis_jobs WHERE status IN ('queued','running')")?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<u32>>(1)?, row.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, worker_pid, updated_at) in jobs {
        let stale = chrono::DateTime::parse_from_rfc3339(&updated_at)
            .map(|time| {
                chrono::Utc::now()
                    .signed_duration_since(time.with_timezone(&chrono::Utc))
                    .num_seconds()
                    >= 5
            })
            .unwrap_or(true);
        if stale && !worker_pid.is_some_and(crate::util::process_is_active) {
            db.execute(
                "UPDATE audio_analysis_jobs SET status='interrupted',error_message='上次音频分析进程已中断，可以显式继续。',worker_pid=NULL,updated_at=?2 WHERE id=?1",
                params![id, now()],
            )?;
        }
    }
    Ok(())
}

fn spawn_worker(job_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let delay = start_delay_ms.map(|value| value.to_string());
    let mut arguments = vec!["__audio_analysis_worker", job_id];
    if let Some(delay) = delay.as_deref() {
        arguments.push(delay);
    }
    crate::util::spawn_detached_current(&arguments)?;
    Ok(())
}

pub fn run_worker(job_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let db = db::open()?;
    let claimed = db.execute(
        "UPDATE audio_analysis_jobs SET status='running',progress=0.05,worker_pid=?2,updated_at=?3 WHERE id=?1 AND status='queued'",
        params![job_id, std::process::id(), now()],
    )?;
    if claimed == 0 {
        return Ok(());
    }
    if let Some(delay) = start_delay_ms {
        thread::sleep(Duration::from_millis(delay));
    }
    match analyze_job(&db, job_id) {
        Ok(report) => {
            let completed_at = now();
            db.execute(
                "UPDATE audio_analysis_jobs SET status='completed',progress=1,report_json=?2,error_message=NULL,worker_pid=NULL,completed_at=?3,updated_at=?3 WHERE id=?1",
                params![job_id, serde_json::to_string(&report)?, completed_at],
            )?;
            Ok(())
        }
        Err(error) if error.to_string() == "audio_analysis_cancelled" => {
            let completed_at = now();
            db.execute(
                "UPDATE audio_analysis_jobs SET status='cancelled',worker_pid=NULL,error_message=NULL,completed_at=?2,updated_at=?2 WHERE id=?1",
                params![job_id, completed_at],
            )?;
            Ok(())
        }
        Err(error) => {
            let completed_at = now();
            db.execute(
                "UPDATE audio_analysis_jobs SET status='failed',worker_pid=NULL,error_message=?2,completed_at=?3,updated_at=?3 WHERE id=?1",
                params![job_id, error.to_string(), completed_at],
            )?;
            Err(error)
        }
    }
}

fn analyze_job(db: &Connection, job_id: &str) -> Result<AudioAnalysisReport> {
    let job = load(db, job_id)?;
    let project = project::load(db, &job.project_id)?;
    let source = Path::new(&project.media.source_path).canonicalize()?;
    let duration = project
        .media
        .duration_seconds
        .or_else(|| crate::media::ffprobe_duration(&source))
        .unwrap_or_default()
        .max(0.0);
    if duration <= 0.0 {
        bail!("audio_duration_unavailable: 无法确定项目媒体时长")
    }
    let ffmpeg = tool_path("SIAOCUT_FFMPEG", "ffmpeg");
    let tool_version = ffmpeg_version(&ffmpeg)?;
    let mut child = hidden_command(&ffmpeg)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(&source)
        .args([
            "-vn",
            "-af",
            "ebur128=peak=true,silencedetect=noise=-40dB:d=0.8",
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("audio_analysis_unavailable: 无法启动固定 FFmpeg")?;
    let mut stderr = child.stderr.take().context("无法读取 FFmpeg 分析输出")?;
    let reader = thread::spawn(move || {
        let mut output = String::new();
        let _ = stderr.read_to_string(&mut output);
        output
    });
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if cancellation_requested(db, job_id)? {
            crate::util::terminate_process_tree(&mut child);
            let _ = reader.join();
            bail!("audio_analysis_cancelled")
        }
        db.execute(
            "UPDATE audio_analysis_jobs SET progress=0.5,updated_at=?2 WHERE id=?1",
            params![job_id, now()],
        )?;
        thread::sleep(Duration::from_millis(100));
    }
    let status = child.wait()?;
    let output = reader
        .join()
        .map_err(|_| anyhow!("无法汇总 FFmpeg 分析输出"))?;
    if !status.success() {
        bail!("audio_analysis_failed: FFmpeg 无法分析项目音轨")
    }
    parse_output(&output, duration, &tool_version)
}

fn cancellation_requested(db: &Connection, job_id: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT cancel_requested_at IS NOT NULL FROM audio_analysis_jobs WHERE id=?1",
        [job_id],
        |row| row.get(0),
    )?)
}

fn ffmpeg_version(ffmpeg: &str) -> Result<String> {
    let output = hidden_command(ffmpeg).arg("-version").output()?;
    if !output.status.success() {
        bail!("audio_analysis_unavailable: FFmpeg 版本检查失败")
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("ffmpeg unknown")
        .trim()
        .to_owned())
}

pub fn parse_output(
    output: &str,
    duration_seconds: f64,
    tool_version: &str,
) -> Result<AudioAnalysisReport> {
    let integrated = last_metric(output, "I:");
    let true_peak = last_metric(output, "Peak:");
    let mut silence_starts = Vec::new();
    let mut silences = Vec::new();
    for line in output.lines() {
        if let Some(value) = metric_after(line, "silence_start:") {
            silence_starts.push(value.max(0.0));
        }
        if let Some(end) = metric_after(line, "silence_end:")
            && let Some(start) = silence_starts.pop()
            && end > start
        {
            silences.push((
                start,
                if duration_seconds > 0.0 {
                    end.min(duration_seconds)
                } else {
                    end
                },
            ));
        }
    }
    if let Some(start) = silence_starts.pop()
        && duration_seconds > start
    {
        silences.push((start, duration_seconds));
    }
    if integrated.is_none() && true_peak.is_none() {
        bail!("audio_analysis_invalid_output: FFmpeg 未返回响度或峰值摘要")
    }
    let thresholds = AudioAnalysisThresholds::default();
    let mut risks = silences
        .iter()
        .map(|(start, end)| AudioRisk {
            kind: AudioRiskKind::Silence,
            start: rounded(*start),
            end: rounded(*end),
            measured_value: rounded(end - start),
            threshold: thresholds.silence_min_seconds,
            unit: "seconds".into(),
            tool_version: tool_version.into(),
        })
        .collect::<Vec<_>>();
    if let Some(value) = integrated {
        let kind = if value < thresholds.quiet_loudness_lufs {
            Some((AudioRiskKind::LoudnessLow, thresholds.quiet_loudness_lufs))
        } else if value > thresholds.loud_loudness_lufs {
            Some((AudioRiskKind::LoudnessHigh, thresholds.loud_loudness_lufs))
        } else {
            None
        };
        if let Some((kind, threshold)) = kind {
            risks.push(AudioRisk {
                kind,
                start: 0.0,
                end: rounded(duration_seconds),
                measured_value: value,
                threshold,
                unit: "LUFS".into(),
                tool_version: tool_version.into(),
            });
        }
    }
    if let Some(value) = true_peak
        && value >= thresholds.clipping_peak_dbfs
    {
        risks.push(AudioRisk {
            kind: AudioRiskKind::SuspectedClipping,
            start: 0.0,
            end: rounded(duration_seconds),
            measured_value: value,
            threshold: thresholds.clipping_peak_dbfs,
            unit: "dBFS".into(),
            tool_version: tool_version.into(),
        });
    }
    Ok(AudioAnalysisReport {
        analyzer_version: ANALYZER_VERSION.into(),
        tool_version: tool_version.into(),
        duration_seconds: rounded(duration_seconds),
        integrated_loudness_lufs: integrated,
        true_peak_dbfs: true_peak,
        silence_duration_seconds: rounded(silences.iter().map(|(start, end)| end - start).sum()),
        thresholds,
        risks,
    })
}

fn last_metric(output: &str, marker: &str) -> Option<f64> {
    output
        .lines()
        .rev()
        .find_map(|line| metric_after(line, marker))
}

fn metric_after(line: &str, marker: &str) -> Option<f64> {
    let value = line.split_once(marker)?.1.split_whitespace().next()?;
    value.parse().ok()
}

fn rounded(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_analysis_parses_loudness_peak_silence_and_threshold_evidence() {
        let output = r#"
[silencedetect @ 1] silence_start: 1.2
[silencedetect @ 1] silence_end: 2.5 | silence_duration: 1.3
  Integrated loudness:
    I:         -25.4 LUFS
  True peak:
    Peak:       -0.1 dBFS
"#;
        let report = parse_output(output, 5.0, "ffmpeg version test").unwrap();
        assert_eq!(report.integrated_loudness_lufs, Some(-25.4));
        assert_eq!(report.true_peak_dbfs, Some(-0.1));
        assert_eq!(report.silence_duration_seconds, 1.3);
        assert_eq!(report.risks.len(), 3);
        assert!(report.risks.iter().all(|risk| risk.end > risk.start));
        assert!(
            report
                .risks
                .iter()
                .all(|risk| risk.tool_version == "ffmpeg version test")
        );
    }

    #[test]
    fn audio_analysis_accepts_clean_audio_without_risks() {
        let output = "I: -18.0 LUFS\nPeak: -2.0 dBFS\n";
        let report = parse_output(output, 10.0, "ffmpeg test").unwrap();
        assert!(report.risks.is_empty());
    }

    #[test]
    fn audio_analysis_rejects_output_without_measurements() {
        assert!(parse_output("no audio", 1.0, "ffmpeg test").is_err());
    }
}
