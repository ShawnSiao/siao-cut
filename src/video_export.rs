use crate::{
    artifacts,
    canvas::{self, CanvasTarget},
    db,
    export::{self, ExportOptions},
    media::{hash_file, tool_path},
    model::{ExportJob, SubtitleMode, TimelineMap},
    project, subtitle_style, timeline,
    util::{hidden_command, new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use std::{
    env, fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

pub struct ExportRequest<'a> {
    pub output: &'a Path,
    pub burn_subtitles: bool,
    pub language: Option<String>,
    pub subtitle_mode: SubtitleMode,
    pub start_delay_ms: Option<u64>,
    pub job_id: Option<String>,
}

struct CommandSpec<'a> {
    ffmpeg: &'a str,
    source: &'a Path,
    output: &'a Path,
    map: &'a TimelineMap,
    has_video: bool,
    has_audio: bool,
    subtitle_path: Option<&'a Path>,
    encoder: &'a str,
    canvas_settings: crate::model::CanvasSettings,
}

pub fn create(
    db: &mut Connection,
    project_id: &str,
    request: ExportRequest<'_>,
) -> Result<ExportJob> {
    let ExportRequest {
        output,
        burn_subtitles,
        language,
        subtitle_mode,
        start_delay_ms,
        job_id,
    } = request;
    if output.extension().and_then(|value| value.to_str()) != Some("mp4") {
        bail!("视频导出路径必须使用 .mp4 扩展名")
    }
    let project = project::load(db, project_id)?;
    let report = export::audit(&project);
    if report["ready"] != true {
        bail!("导出前审计未通过，请先处理媒体或字幕问题")
    }
    if timeline::build(&project).output_duration <= 0.001 {
        bail!("全部内容都被软剪辑移除，无法导出空视频")
    }
    if burn_subtitles {
        export::validate_subtitle_mode(
            &project,
            &ExportOptions {
                format: "ass",
                language: language.as_deref(),
                subtitle_mode,
                include_cuts: false,
            },
        )?;
    }
    let source = Path::new(&project.media.source_path).canonicalize()?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let output_absolute = if output.is_absolute() {
        output.to_path_buf()
    } else {
        env::current_dir()?.join(output)
    };
    let output_comparable = output_absolute
        .canonicalize()
        .unwrap_or_else(|_| output_absolute.clone());
    if output_comparable == source {
        bail!("不能用导出结果覆盖原始媒体")
    }
    let required = fs::metadata(&source)?.len().saturating_mul(5) / 4 + 100 * 1024 * 1024;
    let available = crate::util::available_space(parent)?;
    if available < required {
        bail!(
            "disk_space_low: 导出预计需要至少 {:.1} GB，可用空间仅 {:.1} GB",
            required as f64 / 1_073_741_824.0,
            available as f64 / 1_073_741_824.0
        )
    }

    let created_at = now();
    let job = ExportJob {
        id: job_id.unwrap_or_else(|| new_id("x")),
        project_id: project_id.to_owned(),
        output_path: output_absolute.to_string_lossy().to_string(),
        status: "queued".into(),
        progress: 0.0,
        burn_subtitles,
        language,
        bilingual: subtitle_mode == SubtitleMode::Bilingual,
        subtitle_mode,
        canvas_settings: project.canvas_settings,
        subtitle_style: project.subtitle_style.clone(),
        cancel_requested_at: None,
        error_message: None,
        manifest_path: None,
        created_at: created_at.clone(),
        updated_at: created_at.clone(),
        completed_at: None,
        worker_pid: None,
    };
    db.execute(
        "INSERT INTO export_jobs(id,project_id,output_path,status,progress,burn_subtitles,language,bilingual,subtitle_mode,canvas_aspect_ratio,canvas_framing,subtitle_style_json,created_at,updated_at) VALUES(?1,?2,?3,'queued',0,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
        params![&job.id, &job.project_id, &job.output_path, job.burn_subtitles, &job.language, job.bilingual, job.subtitle_mode.as_str(), job.canvas_settings.aspect_ratio.as_str(), job.canvas_settings.framing.as_str(), subtitle_style::storage_json(&job.subtitle_style)?, &job.created_at],
    )?;
    spawn_worker(&job.id, start_delay_ms)?;
    Ok(job)
}

pub fn load(db: &Connection, job_id: &str) -> Result<ExportJob> {
    db.query_row(
        "SELECT id,project_id,output_path,status,progress,burn_subtitles,language,bilingual,subtitle_mode,canvas_aspect_ratio,canvas_framing,subtitle_style_json,cancel_requested_at,error_message,manifest_path,created_at,updated_at,completed_at,worker_pid FROM export_jobs WHERE id=?1",
        [job_id],
        |row| {
            Ok(ExportJob {
                id: row.get(0)?,
                project_id: row.get(1)?,
                output_path: row.get(2)?,
                status: row.get(3)?,
                progress: row.get(4)?,
                burn_subtitles: row.get(5)?,
                language: row.get(6)?,
                bilingual: row.get(7)?,
                subtitle_mode: SubtitleMode::parse(&row.get::<_, String>(8)?)
                    .ok_or(rusqlite::Error::InvalidQuery)?,
                canvas_settings: crate::model::CanvasSettings {
                    aspect_ratio: crate::model::CanvasAspectRatio::parse(
                        &row.get::<_, String>(9)?,
                    )
                    .ok_or(rusqlite::Error::InvalidQuery)?,
                    framing: crate::model::CanvasFraming::parse(&row.get::<_, String>(10)?)
                        .ok_or(rusqlite::Error::InvalidQuery)?,
                },
                subtitle_style: subtitle_style::from_storage(&row.get::<_, String>(11)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                cancel_requested_at: row.get(12)?,
                error_message: row.get(13)?,
                manifest_path: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
                completed_at: row.get(17)?,
                worker_pid: row.get(18)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("导出任务不存在：{job_id}"))
}

pub fn for_project(db: &Connection, project_id: &str) -> Result<Vec<ExportJob>> {
    db.prepare("SELECT id FROM export_jobs WHERE project_id=?1 ORDER BY created_at DESC")?
        .query_map([project_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| load(db, &id))
        .collect()
}

pub fn cancel(db: &Connection, job_id: &str) -> Result<ExportJob> {
    let changed = db.execute(
        "UPDATE export_jobs SET cancel_requested_at=?2,updated_at=?2 WHERE id=?1 AND status IN ('queued','running')",
        params![job_id, now()],
    )?;
    if changed == 0 {
        let job = load(db, job_id)?;
        bail!("导出任务当前状态不能取消：{}", job.status)
    }
    load(db, job_id)
}

pub fn retry(db: &Connection, job_id: &str) -> Result<ExportJob> {
    let job = load(db, job_id)?;
    if !["failed", "interrupted"].contains(&job.status.as_str()) {
        bail!("视频导出任务当前状态不能重试：{}", job.status)
    }
    db.execute(
        "UPDATE export_jobs SET status='queued',progress=0,cancel_requested_at=NULL,error_message=NULL,completed_at=NULL,worker_pid=NULL,updated_at=?2 WHERE id=?1",
        params![job_id, now()],
    )?;
    if let Err(error) = spawn_worker(job_id, None) {
        db.execute(
            "UPDATE export_jobs SET status='failed',error_message=?2,completed_at=?3,updated_at=?3 WHERE id=?1",
            params![job_id, error.to_string(), now()],
        )?;
        return Err(error);
    }
    load(db, job_id)
}

pub fn reconcile_interrupted(db: &Connection) -> Result<()> {
    let jobs = db
        .prepare("SELECT id FROM export_jobs WHERE status IN ('queued','running')")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in jobs {
        let job = load(db, &id)?;
        let stale = chrono::DateTime::parse_from_rfc3339(&job.updated_at)
            .map(|time| {
                chrono::Utc::now()
                    .signed_duration_since(time.with_timezone(&chrono::Utc))
                    .num_seconds()
                    >= 5
            })
            .unwrap_or(true);
        let worker_alive = job.worker_pid.is_some_and(crate::util::process_is_active);
        if stale && !worker_alive {
            db.execute(
                "UPDATE export_jobs SET status='interrupted',error_message='上次导出进程已中断，可以从 App 重新开始。',worker_pid=NULL,updated_at=?2 WHERE id=?1",
                params![id, now()],
            )?;
        }
    }
    Ok(())
}

fn spawn_worker(job_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let delay = start_delay_ms.map(|value| value.to_string());
    let mut arguments = vec!["__export_worker", job_id];
    if let Some(delay) = delay.as_deref() {
        arguments.push(delay);
    }
    crate::util::spawn_detached_current(&arguments).context("无法启动视频导出任务")?;
    Ok(())
}

pub fn run_worker(job_id: &str, start_delay_ms: Option<u64>) -> Result<()> {
    let mut db = db::open()?;
    db.execute(
        "UPDATE export_jobs SET status='running',worker_pid=?2,updated_at=?3 WHERE id=?1",
        params![job_id, std::process::id(), now()],
    )?;
    if let Some(delay) = start_delay_ms {
        thread::sleep(Duration::from_millis(delay));
    }
    if let Err(error) = run(&mut db, job_id) {
        if let Ok(job) = load(&db, job_id) {
            let partial = partial_path(Path::new(&job.output_path));
            if partial.is_file() {
                let _ = fs::remove_file(partial);
            }
        }
        let timestamp = now();
        let _ = db.execute(
            "UPDATE export_jobs SET status='failed',error_message=?2,worker_pid=NULL,updated_at=?3,completed_at=?3 WHERE id=?1 AND status!='cancelled'",
            params![job_id, error.to_string(), timestamp],
        );
        return Err(error);
    }
    Ok(())
}

fn run(db: &mut Connection, job_id: &str) -> Result<()> {
    let job = load(db, job_id)?;
    if job.cancel_requested_at.is_some() {
        finish_cancelled(db, job_id)?;
        return Ok(());
    }
    db.execute(
        "UPDATE export_jobs SET status='running',progress=0.01,worker_pid=?2,updated_at=?3,error_message=NULL WHERE id=?1",
        params![job_id, std::process::id(), now()],
    )?;
    let project = project::load(db, &job.project_id)?;
    let map = timeline::build(&project);
    let source = Path::new(&project.media.source_path);
    let output = PathBuf::from(&job.output_path);
    let partial = partial_path(&output);
    let ffmpeg = tool_path("SIAOCUT_FFMPEG", "ffmpeg");
    let encoder = artifacts::preferred_video_encoder(&ffmpeg)?;
    let has_video = artifacts::has_stream(source, "v:0")?;
    let has_audio = artifacts::has_stream(source, "a:0")?;
    let subtitle_path = if job.burn_subtitles {
        let dir = db::home_dir().join("cache").join("exports");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.ass", job.id));
        let mut export_project = project.clone();
        export_project.subtitle_style = job.subtitle_style.clone();
        fs::write(
            &path,
            export::render(
                &export_project,
                &ExportOptions {
                    format: "ass",
                    language: job.language.as_deref(),
                    subtitle_mode: job.subtitle_mode,
                    include_cuts: false,
                },
            )?,
        )?;
        Some(path)
    } else {
        None
    };

    let mut command = build_command(CommandSpec {
        ffmpeg: &ffmpeg,
        source,
        output: &partial,
        map: &map,
        has_video,
        has_audio,
        subtitle_path: subtitle_path.as_deref(),
        encoder: &encoder,
        canvas_settings: job.canvas_settings,
    })?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().context("无法启动 FFmpeg 视频导出")?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("无法读取 FFmpeg 进度"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("无法读取 FFmpeg 错误"))?;
    let (progress_tx, progress_rx) = mpsc::channel();
    let progress_reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = progress_tx.send(line);
        }
    });
    let error_reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut text);
        text
    });

    let mut last_progress = 0.01;
    let status = loop {
        while let Ok(line) = progress_rx.try_recv() {
            if let Some(value) = line.strip_prefix("out_time_us=")
                && let Ok(microseconds) = value.parse::<f64>()
            {
                let progress =
                    (microseconds / 1_000_000.0 / map.output_duration).clamp(last_progress, 0.99);
                if progress - last_progress >= 0.01 {
                    last_progress = progress;
                    db.execute(
                        "UPDATE export_jobs SET progress=?2,updated_at=?3 WHERE id=?1",
                        params![job_id, progress, now()],
                    )?;
                }
            }
        }
        let cancel_requested: bool = db.query_row(
            "SELECT cancel_requested_at IS NOT NULL FROM export_jobs WHERE id=?1",
            [job_id],
            |row| row.get(0),
        )?;
        if cancel_requested {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        thread::sleep(Duration::from_millis(200));
    };
    let _ = progress_reader.join();
    let stderr = error_reader.join().unwrap_or_default();
    if status.is_none() {
        if partial.is_file() {
            fs::remove_file(&partial)?;
        }
        finish_cancelled(db, job_id)?;
        return Ok(());
    }
    if !status.is_some_and(|status| status.success()) {
        bail!("FFmpeg 视频导出失败：{}", stderr.trim())
    }
    if output.is_file() {
        fs::remove_file(&output)?;
    }
    fs::rename(&partial, &output)?;
    let manifest_path = output.with_extension("siaocut.json");
    let manifest = json!({
        "apiVersion": "0.1",
        "projectId": project.id,
        "source": { "path": project.media.source_path, "sha256": project.media.sha256 },
        "output": { "path": output, "sha256": hash_file(&output)?, "bytes": fs::metadata(&output)?.len() },
        "timeline": map,
        "encoder": encoder,
        "burnSubtitles": job.burn_subtitles,
        "language": job.language,
        "bilingual": job.bilingual,
        "subtitleMode": job.subtitle_mode,
        "canvasSettings": job.canvas_settings,
        "subtitleStyle": job.subtitle_style,
        "createdAt": now()
    });
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    let completed_at = now();
    db.execute(
        "UPDATE export_jobs SET status='completed',progress=1,manifest_path=?2,worker_pid=NULL,updated_at=?3,completed_at=?3 WHERE id=?1",
        params![job_id, manifest_path.to_string_lossy(), completed_at],
    )?;
    Ok(())
}

fn build_command(spec: CommandSpec<'_>) -> Result<Command> {
    let CommandSpec {
        ffmpeg,
        source,
        output,
        map,
        has_video,
        has_audio,
        subtitle_path,
        encoder,
        canvas_settings,
    } = spec;
    if !has_video && !has_audio {
        bail!("媒体中没有可导出的音视频流")
    }
    let mut command = hidden_command(ffmpeg);
    command.args(["-y", "-hide_banner", "-loglevel", "error"]);
    let video_input;
    let audio_input;
    if has_video && has_audio {
        command.arg("-i").arg(source);
        video_input = 0;
        audio_input = 0;
    } else if has_video {
        command
            .arg("-i")
            .arg(source)
            .args(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo"]);
        video_input = 0;
        audio_input = 1;
    } else {
        command.arg("-i").arg(source).args([
            "-f",
            "lavfi",
            "-i",
            "color=c=0x101414:s=1280x720:r=30",
        ]);
        video_input = 1;
        audio_input = 0;
    }

    let mut filters = Vec::new();
    let mut concat_inputs = String::new();
    for (index, range) in map.kept_ranges.iter().enumerate() {
        filters.push(format!(
            "[{video_input}:v]trim=start={:.6}:end={:.6},setpts=PTS-STARTPTS[v{index}]",
            range.source_start, range.source_end
        ));
        filters.push(format!(
            "[{audio_input}:a]atrim=start={:.6}:end={:.6},asetpts=PTS-STARTPTS,afade=t=in:st=0:d=0.03,afade=t=out:st={:.6}:d=0.03[a{index}]",
            range.source_start,
            range.source_end,
            (range.source_end - range.source_start - 0.03).max(0.0)
        ));
        concat_inputs.push_str(&format!("[v{index}][a{index}]"));
    }
    filters.push(format!(
        "{concat_inputs}concat=n={}:v=1:a=1[vcat][acat]",
        map.kept_ranges.len()
    ));
    canvas::append_transform(
        &mut filters,
        "vcat",
        "vcanvas",
        canvas_settings,
        CanvasTarget::Export,
    );
    let video_label = if let Some(path) = subtitle_path {
        filters.push(format!(
            "[vcanvas]subtitles=filename='{}'[vout]",
            escape_filter_path(path)
        ));
        "[vout]"
    } else {
        "[vcanvas]"
    };
    command
        .args(["-filter_complex", &filters.join(";")])
        .args(["-map", video_label, "-map", "[acat]"])
        .args(artifacts::video_encoder_args(encoder))
        .args([
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
        ])
        .arg(output);
    Ok(command)
}

fn escape_filter_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

fn partial_path(output: &Path) -> PathBuf {
    let stem = output
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("siaocut-export");
    output.with_file_name(format!("{stem}.part.mp4"))
}

fn finish_cancelled(db: &Connection, job_id: &str) -> Result<()> {
    let timestamp = now();
    db.execute(
        "UPDATE export_jobs SET status='cancelled',worker_pid=NULL,updated_at=?2,completed_at=?2 WHERE id=?1",
        params![job_id, timestamp],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cuts, export,
        model::{SubtitleMode, SubtitlePosition, SubtitleStylePreset, TimelineMap, TimelineRange},
        project,
    };
    use rusqlite::params;
    use std::{fs, process::Command};
    use tempfile::tempdir;

    #[test]
    fn video_export_job_keeps_its_subtitle_style_snapshot() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("style-snapshot.wav");
        fs::write(&media, b"audio").unwrap();
        let mut database = crate::db::open_at(&temp.path().join("style.db")).unwrap();
        let created = project::create(&mut database, &media, None).unwrap();
        project::add_segment(
            &mut database,
            &created.id,
            0.0,
            1.0,
            "字幕快照".into(),
            None,
        )
        .unwrap();
        crate::subtitle_style::set(&mut database, &created.id, "emphasis", "center").unwrap();
        let job = create(
            &mut database,
            &created.id,
            ExportRequest {
                output: &temp.path().join("snapshot.mp4"),
                burn_subtitles: true,
                language: None,
                subtitle_mode: SubtitleMode::Source,
                start_delay_ms: Some(60_000),
                job_id: Some("x-style-snapshot".into()),
            },
        )
        .unwrap();
        assert_eq!(job.subtitle_style.preset, SubtitleStylePreset::Emphasis);
        assert_eq!(job.subtitle_style.position, SubtitlePosition::Center);

        crate::subtitle_style::set(&mut database, &created.id, "compact", "bottom").unwrap();
        let reloaded = load(&database, &job.id).unwrap();
        assert_eq!(
            reloaded.subtitle_style.preset,
            SubtitleStylePreset::Emphasis
        );
        assert_eq!(reloaded.subtitle_style.position, SubtitlePosition::Center);
    }

    #[test]
    fn subtitle_style_real_media_burns_deterministic_ass() {
        let evidence_path = std::env::var_os("SIAOCUT_PHASE8_MEDIA_EVIDENCE").map(PathBuf::from);
        if !crate::media::command_available("ffmpeg") || !crate::media::command_available("ffprobe")
        {
            assert!(
                evidence_path.is_none(),
                "FFmpeg and FFprobe are required for Phase 8 evidence"
            );
            return;
        }
        let temp = tempdir().unwrap();
        let source = temp.path().join("subtitle-style-source.mp4");
        let generated = Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=0x20272f:s=640x360:r=30",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=stereo",
                "-t",
                "2",
                "-c:v",
                "mpeg4",
                "-q:v",
                "3",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(generated.success());
        let source_hash = hash_file(&source).unwrap();

        let mut database = crate::db::open_at(&temp.path().join("styled-media.db")).unwrap();
        let created = project::create(&mut database, &source, None).unwrap();
        project::add_segment(
            &mut database,
            &created.id,
            0.1,
            1.8,
            "SiaoCut 字幕预览".into(),
            None,
        )
        .unwrap();
        crate::subtitle_style::set(&mut database, &created.id, "emphasis", "bottom").unwrap();
        let styled = project::load(&database, &created.id).unwrap();
        let subtitle_path = temp.path().join("styled.ass");
        let ass = export::render(
            &styled,
            &export::ExportOptions {
                format: "ass",
                language: None,
                subtitle_mode: SubtitleMode::Source,
                include_cuts: false,
            },
        )
        .unwrap();
        assert!(ass.contains("Style: Primary,Microsoft YaHei UI,60"));
        assert!(ass.contains(",4,2,2,80,80,108,1"));
        fs::write(&subtitle_path, &ass).unwrap();

        let output = temp.path().join("styled-output.mp4");
        let status = build_command(CommandSpec {
            ffmpeg: "ffmpeg",
            source: &source,
            output: &output,
            map: &styled.timeline,
            has_video: true,
            has_audio: true,
            subtitle_path: Some(&subtitle_path),
            encoder: "mpeg4",
            canvas_settings: styled.canvas_settings,
        })
        .unwrap()
        .status()
        .unwrap();
        assert!(status.success());
        assert!(output.is_file());
        assert_eq!(hash_file(&source).unwrap(), source_hash);

        if let Some(evidence_path) = evidence_path {
            let evidence = json!({
                "date": "2026-07-18",
                "status": "passed",
                "fixture": {"durationSeconds": 2, "video": "640x360@30", "audio": "silent stereo@48000"},
                "style": styled.subtitle_style,
                "ass": {"playResolution": "1920x1080", "primaryStyle": "60px", "outline": "4px", "safeMargin": "10%"},
                "burnedVideo": {"generated": true, "bytes": fs::metadata(&output).unwrap().len()},
                "sourceHashUnchanged": true
            });
            fs::write(evidence_path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        }
    }

    #[test]
    fn ffmpeg_export_uses_timeline_ranges_and_progress_protocol() {
        let map = TimelineMap {
            source_duration: 8.0,
            output_duration: 7.0,
            kept_ranges: vec![
                TimelineRange {
                    source_start: 0.0,
                    source_end: 2.0,
                    output_start: 0.0,
                    output_end: 2.0,
                },
                TimelineRange {
                    source_start: 3.0,
                    source_end: 8.0,
                    output_start: 2.0,
                    output_end: 7.0,
                },
            ],
            cuts: Vec::new(),
        };
        let command = build_command(CommandSpec {
            ffmpeg: "ffmpeg",
            source: Path::new("source.mp4"),
            output: Path::new("output.part.mp4"),
            map: &map,
            has_video: true,
            has_audio: true,
            subtitle_path: None,
            encoder: "mpeg4",
            canvas_settings: Default::default(),
        })
        .unwrap();
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(arguments.contains("trim=start=0.000000:end=2.000000"));
        assert!(arguments.contains("trim=start=3.000000:end=8.000000"));
        assert!(arguments.contains("concat=n=2:v=1:a=1"));
        assert!(arguments.contains("[vcat]null[vcanvas]"));
        assert!(arguments.contains("afade=t=in:st=0:d=0.03"));
        assert!(arguments.contains("afade=t=out:st=1.970000:d=0.03"));
        assert!(arguments.contains("-progress pipe:1"));
    }

    #[test]
    fn ffmpeg_export_applies_vertical_canvas_after_timeline_concat() {
        let map = TimelineMap {
            source_duration: 2.0,
            output_duration: 2.0,
            kept_ranges: vec![TimelineRange {
                source_start: 0.0,
                source_end: 2.0,
                output_start: 0.0,
                output_end: 2.0,
            }],
            cuts: Vec::new(),
        };
        let command = build_command(CommandSpec {
            ffmpeg: "ffmpeg",
            source: Path::new("source.mp4"),
            output: Path::new("output.part.mp4"),
            map: &map,
            has_video: true,
            has_audio: true,
            subtitle_path: None,
            encoder: "mpeg4",
            canvas_settings: crate::model::CanvasSettings {
                aspect_ratio: crate::model::CanvasAspectRatio::Vertical,
                framing: crate::model::CanvasFraming::CoverCenter,
            },
        })
        .unwrap();
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(arguments.contains("concat=n=1:v=1:a=1[vcat][acat]"));
        assert!(arguments.contains("[vcat]scale=1080:1920"));
        assert!(arguments.contains("crop=1080:1920"));
        assert!(arguments.contains("format=yuv420p[vcanvas]"));
    }

    #[test]
    fn word_range_cut_real_media_matches_timeline_subtitles_and_audio_seam() {
        let evidence_path = std::env::var_os("SIAOCUT_PHASE2_EVIDENCE").map(PathBuf::from);
        if !crate::media::command_available("ffmpeg") || !crate::media::command_available("ffprobe")
        {
            assert!(
                evidence_path.is_none(),
                "FFmpeg and FFprobe are required for Phase 2 evidence"
            );
            return;
        }
        let temp = tempdir().unwrap();
        let source = temp.path().join("word-cut-source.mp4");
        let generated = Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
            ])
            .arg("testsrc2=size=640x360:rate=30")
            .args(["-f", "lavfi", "-i"])
            .arg("sine=frequency=440:sample_rate=48000")
            .args([
                "-t",
                "3",
                "-c:v",
                "mpeg4",
                "-q:v",
                "3",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(generated.success());

        let mut db = crate::db::open_at(&temp.path().join("word-cut-media.db")).unwrap();
        let project = project::create(&mut db, &source, Some("Word cut media".into())).unwrap();
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
            ("media-w1", 0.2, 0.7, "hello", 0),
            ("media-w2", 1.0, 1.5, "brave", 1),
            ("media-w3", 2.0, 2.7, "world", 2),
        ] {
            db.execute(
                "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![id, &project.id, &segment.id, start, end, text, ordinal],
            )
            .unwrap();
        }
        let cut = cuts::create_word_range(
            &mut db,
            &project.id,
            &segment.id,
            "media-w2",
            "media-w2",
            200,
        )
        .unwrap();
        let preview = cuts::preview(&db, &project.id, &cut.id).unwrap();
        cuts::set_status(&mut db, &project.id, &cut.id, "applied").unwrap();
        let project = project::load(&db, &project.id).unwrap();
        assert!((project.timeline.output_duration - 2.1).abs() < 0.01);

        let subtitle_path = temp.path().join("word-cut.srt");
        let subtitle = export::render(
            &project,
            &export::ExportOptions {
                format: "srt",
                language: None,
                subtitle_mode: SubtitleMode::Source,
                include_cuts: false,
            },
        )
        .unwrap();
        assert!(subtitle.contains("hello"));
        assert!(subtitle.contains("world"));
        assert!(!subtitle.contains("brave"));
        fs::write(&subtitle_path, &subtitle).unwrap();

        let output = temp.path().join("word-cut-output.mp4");
        let status = build_command(CommandSpec {
            ffmpeg: "ffmpeg",
            source: &source,
            output: &output,
            map: &project.timeline,
            has_video: true,
            has_audio: true,
            subtitle_path: Some(&subtitle_path),
            encoder: "mpeg4",
            canvas_settings: Default::default(),
        })
        .unwrap()
        .status()
        .unwrap();
        assert!(status.success());

        let duration_output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=nw=1:nk=1",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(duration_output.status.success());
        let duration = String::from_utf8(duration_output.stdout)
            .unwrap()
            .trim()
            .parse::<f64>()
            .unwrap();
        assert!(
            (duration - 2.1).abs() <= 0.08,
            "unexpected output duration: {duration}"
        );

        let decoded = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(&output)
            .args(["-vn", "-ac", "1", "-ar", "48000", "-f", "f32le", "-"])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        let samples = decoded
            .stdout
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        let join_sample = (0.8 * 48_000.0) as usize;
        let seam_radius = 144usize;
        let seam = &samples[join_sample - seam_radius..join_sample + seam_radius];
        let seam_peak = seam
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0_f32, f32::max);
        let seam_max_delta = seam
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0_f32, f32::max);
        assert!(seam_peak < 0.04, "audio seam peak is too high: {seam_peak}");
        assert!(
            seam_max_delta < 0.02,
            "audio seam delta is too high: {seam_max_delta}"
        );

        if let Some(evidence_path) = evidence_path {
            let evidence = json!({
                "date": "2026-07-17",
                "status": "passed",
                "fixture": {"durationSeconds": 3.0, "video": "640x360@30", "audio": "440Hz@48000"},
                "wordRange": {"selected": "brave", "selectedStart": 1.0, "selectedEnd": 1.5, "paddingMs": 200, "cutStart": cut.start, "cutEnd": cut.end},
                "preview": preview,
                "timelineOutputDuration": project.timeline.output_duration,
                "ffprobeOutputDuration": duration,
                "subtitleContains": ["hello", "world"],
                "subtitleExcludes": ["brave"],
                "audioSeam": {"windowMilliseconds": 6, "peak": seam_peak, "maxAdjacentDelta": seam_max_delta, "fadeMilliseconds": 30}
            });
            fs::write(evidence_path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        }
    }

    #[test]
    fn cut_suggestion_real_media_applies_and_restores_reviewed_range() {
        let evidence_path = std::env::var_os("SIAOCUT_PHASE3_MEDIA_EVIDENCE").map(PathBuf::from);
        if !crate::media::command_available("ffmpeg") || !crate::media::command_available("ffprobe")
        {
            assert!(
                evidence_path.is_none(),
                "FFmpeg and FFprobe are required for Phase 3 media evidence"
            );
            return;
        }
        let temp = tempdir().unwrap();
        let source = temp.path().join("suggestion-source.mp4");
        let generated = Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
            ])
            .arg("testsrc2=size=640x360:rate=30")
            .args(["-f", "lavfi", "-i"])
            .arg("sine=frequency=550:sample_rate=48000")
            .args([
                "-t",
                "4",
                "-c:v",
                "mpeg4",
                "-q:v",
                "3",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(generated.success());
        let source_hash = hash_file(&source).unwrap();

        let mut db = crate::db::open_at(&temp.path().join("suggestion-media.db")).unwrap();
        let project = project::create(&mut db, &source, Some("Suggestion media".into())).unwrap();
        let segment = project::add_segment(
            &mut db,
            &project.id,
            0.2,
            3.2,
            "we um can start".into(),
            None,
        )
        .unwrap();
        for (id, start, end, text, ordinal) in [
            ("suggest-w1", 0.2, 0.6, "we", 0),
            ("suggest-w2", 1.0, 1.2, "um", 1),
            ("suggest-w3", 1.6, 2.0, "can", 2),
            ("suggest-w4", 2.4, 3.2, "start", 3),
        ] {
            db.execute(
                "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![id, &project.id, &segment.id, start, end, text, ordinal],
            )
            .unwrap();
        }

        let suggestions = cuts::detect(&mut db, &project.id).unwrap();
        assert_eq!(suggestions.len(), 1);
        let suggestion = &suggestions[0];
        assert_eq!(suggestion.status, "proposed");
        assert_eq!(
            suggestion.suggestion.as_ref().unwrap().suggestion_type,
            "standalone_filler"
        );
        assert!(
            (project::load(&db, &project.id)
                .unwrap()
                .timeline
                .output_duration
                - 4.0)
                .abs()
                < 0.02
        );
        let preview = cuts::preview(&db, &project.id, &suggestion.id).unwrap();
        cuts::set_status(&mut db, &project.id, &suggestion.id, "applied").unwrap();
        let applied = project::load(&db, &project.id).unwrap();
        assert!((applied.timeline.output_duration - 3.6).abs() < 0.02);

        let subtitle_path = temp.path().join("suggestion.srt");
        let applied_subtitle = export::render(
            &applied,
            &export::ExportOptions {
                format: "srt",
                language: None,
                subtitle_mode: SubtitleMode::Source,
                include_cuts: false,
            },
        )
        .unwrap();
        assert!(applied_subtitle.contains("we"));
        assert!(applied_subtitle.contains("can start"));
        assert!(!applied_subtitle.contains("um"));
        fs::write(&subtitle_path, &applied_subtitle).unwrap();

        let output = temp.path().join("suggestion-output.mp4");
        let status = build_command(CommandSpec {
            ffmpeg: "ffmpeg",
            source: &source,
            output: &output,
            map: &applied.timeline,
            has_video: true,
            has_audio: true,
            subtitle_path: Some(&subtitle_path),
            encoder: "mpeg4",
            canvas_settings: Default::default(),
        })
        .unwrap()
        .status()
        .unwrap();
        assert!(status.success());
        let duration_output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=nw=1:nk=1",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(duration_output.status.success());
        let output_duration = String::from_utf8(duration_output.stdout)
            .unwrap()
            .trim()
            .parse::<f64>()
            .unwrap();
        assert!((output_duration - 3.6).abs() <= 0.08);
        assert_eq!(hash_file(&source).unwrap(), source_hash);

        cuts::set_status(&mut db, &project.id, &suggestion.id, "restored").unwrap();
        let restored = project::load(&db, &project.id).unwrap();
        assert!((restored.timeline.output_duration - 4.0).abs() < 0.02);
        let restored_subtitle = export::render(
            &restored,
            &export::ExportOptions {
                format: "srt",
                language: None,
                subtitle_mode: SubtitleMode::Source,
                include_cuts: false,
            },
        )
        .unwrap();
        assert!(restored_subtitle.contains("we um can start"));

        if let Some(evidence_path) = evidence_path {
            let evidence = json!({
                "date": "2026-07-17",
                "status": "passed",
                "fixture": {"durationSeconds": 4.0, "video": "640x360@30", "audio": "550Hz@48000"},
                "suggestion": {
                    "type": suggestion.suggestion.as_ref().unwrap().suggestion_type,
                    "confidence": suggestion.suggestion.as_ref().unwrap().confidence,
                    "selected": "um",
                    "cutStart": suggestion.start,
                    "cutEnd": suggestion.end,
                    "statusBeforeReview": "proposed"
                },
                "preview": preview,
                "timelineBeforeReview": 4.0,
                "timelineAfterApply": applied.timeline.output_duration,
                "ffprobeOutputDuration": output_duration,
                "appliedSubtitleExcludes": ["um"],
                "timelineAfterRestore": restored.timeline.output_duration,
                "restoredSubtitleContains": ["we um can start"],
                "sourceHashUnchanged": true
            });
            fs::write(evidence_path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        }
    }
}
