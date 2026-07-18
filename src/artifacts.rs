use crate::{
    canvas::{self, CanvasTarget},
    db::home_dir,
    media::{command_available, hash_file, tool_path},
    model::MediaArtifacts,
    project,
    util::{hidden_command, now},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use std::{fs, path::Path, process::Command, thread, time::Duration};

pub fn load(db: &Connection, project_id: &str) -> Result<Option<MediaArtifacts>> {
    db.query_row(
        "SELECT status,proxy_path,waveform_path,thumbnails_json,source_sha256,updated_at,error_message FROM media_artifacts WHERE project_id=?1",
        [project_id],
        |row| {
            let thumbnails: String = row.get(3)?;
            Ok(MediaArtifacts {
                status: row.get(0)?,
                proxy_path: row.get(1)?,
                waveform_path: row.get(2)?,
                thumbnails: serde_json::from_str(&thumbnails).unwrap_or_default(),
                source_sha256: row.get(4)?,
                updated_at: row.get(5)?,
                error_message: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn prepare(db: &mut Connection, project_id: &str) -> Result<MediaArtifacts> {
    let project = project::load(db, project_id)?;
    let source = Path::new(&project.media.source_path);
    if !source.is_file() {
        bail!("媒体文件不存在：{}", source.display())
    }
    let source_hash = hash_file(source)?;
    if source_hash != project.media.sha256 {
        bail!("media_hash_changed: 原片校验值已变化，不能静默生成代理媒体")
    }
    if let Some(existing) = load(db, project_id)?
        && existing.status == "ready"
        && existing.source_sha256 == source_hash
        && existing
            .proxy_path
            .as_deref()
            .is_some_and(|path| Path::new(path).is_file())
    {
        return Ok(existing);
    }

    let ffmpeg = tool_path("SIAOCUT_FFMPEG", "ffmpeg");
    if !command_available(&ffmpeg) {
        bail!("FFmpeg 未配置，无法生成预览资源")
    }
    let artifact_dir = home_dir().join("projects").join(project_id).join("preview");
    fs::create_dir_all(&artifact_dir)?;
    let updated_at = now();
    db.execute(
        "INSERT INTO media_artifacts(project_id,status,source_sha256,updated_at,thumbnails_json) VALUES(?1,'processing',?2,?3,'[]') ON CONFLICT(project_id) DO UPDATE SET status='processing',source_sha256=excluded.source_sha256,updated_at=excluded.updated_at,error_message=NULL",
        params![project_id, &source_hash, &updated_at],
    )?;

    let result = generate(
        &ffmpeg,
        source,
        project.media.duration_seconds.unwrap_or(0.0),
        &artifact_dir,
        project.canvas_settings,
    );
    match result {
        Ok((proxy, waveform, thumbnails)) => {
            let updated_at = now();
            db.execute(
                "UPDATE media_artifacts SET status='ready',proxy_path=?2,waveform_path=?3,thumbnails_json=?4,updated_at=?5,error_message=NULL WHERE project_id=?1",
                params![project_id, proxy.to_string_lossy(), waveform.as_ref().map(|path| path.to_string_lossy().to_string()), serde_json::to_string(&thumbnails.iter().map(|path| path.to_string_lossy().to_string()).collect::<Vec<_>>())?, &updated_at],
            )?;
            load(db, project_id)?.ok_or_else(|| anyhow!("预览资源记录不存在"))
        }
        Err(error) => {
            db.execute(
                "UPDATE media_artifacts SET status='failed',updated_at=?2,error_message=?3 WHERE project_id=?1",
                params![project_id, now(), error.to_string()],
            )?;
            Err(error)
        }
    }
}

fn generate(
    ffmpeg: &str,
    source: &Path,
    duration: f64,
    artifact_dir: &Path,
    canvas_settings: crate::model::CanvasSettings,
) -> Result<(
    std::path::PathBuf,
    Option<std::path::PathBuf>,
    Vec<std::path::PathBuf>,
)> {
    let has_video = has_stream(source, "v:0")?;
    let has_audio = has_stream(source, "a:0")?;
    if !has_video && !has_audio {
        bail!("媒体中没有可预览的音视频流")
    }

    let proxy = artifact_dir.join("proxy.mp4");
    let proxy_partial = artifact_dir.join("proxy.part.mp4");
    let encoder = preferred_video_encoder(ffmpeg)?;
    let mut command = hidden_command(ffmpeg);
    command
        .arg("-y")
        .arg("-hide_banner")
        .args(["-loglevel", "error"]);
    let audio_map = if has_video {
        command.arg("-i").arg(source);
        "0:a?"
    } else {
        command
            .args(["-f", "lavfi", "-i", "color=c=0x101414:s=1280x720:r=30"])
            .arg("-i")
            .arg(source);
        "1:a:0"
    };
    let mut filters = Vec::new();
    canvas::append_transform(
        &mut filters,
        "0:v",
        "vpreview",
        canvas_settings,
        CanvasTarget::Preview,
    );
    command.args(["-filter_complex", &filters.join(";")]).args([
        "-map",
        "[vpreview]",
        "-map",
        audio_map,
    ]);
    if !has_video {
        command.arg("-shortest");
    }
    command.args(video_encoder_args(&encoder));
    if has_audio {
        command.args(["-c:a", "aac", "-b:a", "128k"]);
    }
    command
        .args(["-movflags", "+faststart"])
        .arg(&proxy_partial);
    run(&mut command, "代理视频生成失败")?;
    if proxy.is_file() {
        fs::remove_file(&proxy)?;
    }
    fs::rename(&proxy_partial, &proxy)?;

    let waveform = if has_audio {
        let path = artifact_dir.join("waveform.png");
        let mut waveform_command = hidden_command(ffmpeg);
        waveform_command
            .arg("-y")
            .args(["-hide_banner", "-loglevel", "error"])
            .arg("-i")
            .arg(source)
            .args([
                "-filter_complex",
                "aformat=channel_layouts=mono,showwavespic=s=1600x160:colors=0x65d6a0",
                "-frames:v",
                "1",
            ])
            .arg(&path);
        run(&mut waveform_command, "波形生成失败")?;
        Some(path)
    } else {
        None
    };

    let mut thumbnails = Vec::new();
    if has_video {
        let interval = (duration / 8.0).max(1.0);
        let pattern = artifact_dir.join("thumb-%03d.jpg");
        let mut thumbnail_command = hidden_command(ffmpeg);
        thumbnail_command
            .arg("-y")
            .args(["-hide_banner", "-loglevel", "error"])
            .arg("-i")
            .arg(source)
            .args([
                "-vf",
                &format!("fps=1/{interval:.3},scale=240:-2"),
                "-frames:v",
                "8",
                "-q:v",
                "4",
            ])
            .arg(&pattern);
        if let Err(first_error) = run(&mut thumbnail_command, "关键帧缩略图生成失败") {
            thread::sleep(Duration::from_millis(150));
            run(&mut thumbnail_command, "关键帧缩略图重试失败")
                .with_context(|| format!("首次关键帧生成失败：{first_error}"))?;
        }
        thumbnails = fs::read_dir(artifact_dir)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("thumb-") && name.ends_with(".jpg"))
            })
            .collect();
        thumbnails.sort();
    }
    Ok((proxy, waveform, thumbnails))
}

pub fn has_stream(source: &Path, selector: &str) -> Result<bool> {
    let ffprobe = tool_path("SIAOCUT_FFPROBE", "ffprobe");
    let mut command = hidden_command(&ffprobe);
    let output = command
        .args([
            "-v",
            "error",
            "-select_streams",
            selector,
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
        ])
        .arg(source)
        .output()
        .with_context(|| format!("无法启动 FFprobe：{ffprobe}"))?;
    Ok(output.status.success() && !output.stdout.is_empty())
}

pub fn preferred_video_encoder(ffmpeg: &str) -> Result<String> {
    let mut command = hidden_command(ffmpeg);
    let output = command
        .args(["-hide_banner", "-encoders"])
        .output()
        .with_context(|| format!("无法读取 FFmpeg 编码器：{ffmpeg}"))?;
    let encoders = String::from_utf8_lossy(&output.stdout);
    ["h264_mf", "libx264", "mpeg4"]
        .into_iter()
        .find(|encoder| encoders.split_whitespace().any(|value| value == *encoder))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("FFmpeg 缺少可用的视频编码器"))
}

pub fn video_encoder_args(encoder: &str) -> Vec<&str> {
    match encoder {
        "libx264" => vec!["-c:v", "libx264", "-preset", "veryfast", "-crf", "23"],
        "h264_mf" => vec!["-c:v", "h264_mf", "-b:v", "3M"],
        _ => vec!["-c:v", "mpeg4", "-q:v", "5"],
    }
}

fn run(command: &mut Command, label: &str) -> Result<()> {
    let output = command.output().with_context(|| label.to_owned())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            format!("FFmpeg 退出状态 {}", output.status)
        } else {
            stderr.trim().to_owned()
        };
        bail!("{label}：{detail}")
    }
    Ok(())
}
