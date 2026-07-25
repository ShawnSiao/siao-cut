use crate::{
    canvas::{self, CanvasTarget},
    db::home_dir,
    media::{command_available, hash_file, tool_path},
    model::MediaArtifacts,
    project,
    util::{hidden_command, now},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

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
    let base_version_id = project
        .history
        .current_version_id
        .clone()
        .ok_or_else(|| anyhow!("preview_project_version_missing: 项目没有可绑定的当前版本"))?;
    let source = Path::new(&project.media.source_path);
    if !source.is_file() {
        bail!("媒体文件不存在：{}", source.display())
    }
    let source_hash = hash_file(source)?;
    if source_hash != project.media.sha256 {
        bail!("media_hash_changed: 原片校验值已变化，不能静默生成代理媒体")
    }
    let project_dir = home_dir().join("projects").join(project_id);
    fs::create_dir_all(&project_dir)?;
    let artifact_dir = project_dir.join("preview");
    recover_preview_publication(db, project_id, &project_dir, &artifact_dir)?;
    if let Some(existing) = load(db, project_id)?
        && existing.status == "ready"
        && existing.source_sha256 == source_hash
        && db
            .query_row(
                "SELECT base_version_id=?2 FROM media_artifacts WHERE project_id=?1",
                params![project_id, &base_version_id],
                |row| row.get::<_, bool>(0),
            )
            .unwrap_or(false)
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
    let generation_id = crate::util::new_id("preview");
    let staging_dir = project_dir.join(format!("preview.staging-{generation_id}"));
    fs::create_dir_all(&staging_dir)?;
    let updated_at = now();
    db.execute(
        "INSERT INTO media_artifacts(project_id,status,source_sha256,updated_at,thumbnails_json,base_version_id,generation_id,owner_pid) VALUES(?1,'processing',?2,?3,'[]',?4,?5,?6) ON CONFLICT(project_id) DO UPDATE SET status='processing',source_sha256=excluded.source_sha256,updated_at=excluded.updated_at,error_message=NULL,base_version_id=excluded.base_version_id,generation_id=excluded.generation_id,owner_pid=excluded.owner_pid",
        params![project_id, &source_hash, &updated_at, &base_version_id, &generation_id, std::process::id()],
    )?;

    let result = generate(
        &ffmpeg,
        source,
        project.media.duration_seconds.unwrap_or(0.0),
        &staging_dir,
        project.canvas_settings,
    );
    match result {
        Ok((proxy, waveform, thumbnails)) => {
            let publication = (|| -> Result<MediaArtifacts> {
                let updated_at = now();
                let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if project::current_version_id(&tx, project_id)?.as_deref()
                    != Some(base_version_id.as_str())
                    || hash_file(source)? != source_hash
                {
                    tx.execute(
                        "UPDATE media_artifacts SET status='stale',updated_at=?2,error_message='preview_project_changed: 预览生成期间项目或媒体发生变化。',owner_pid=NULL WHERE project_id=?1 AND generation_id=?3",
                        params![project_id, &updated_at, &generation_id],
                    )?;
                    tx.commit()?;
                    bail!("preview_project_changed: 预览生成期间项目或媒体发生变化，结果未发布")
                }
                let owned_generation: bool = tx.query_row(
                    "SELECT generation_id=?2 FROM media_artifacts WHERE project_id=?1",
                    params![project_id, &generation_id],
                    |row| row.get(0),
                )?;
                if !owned_generation {
                    bail!("preview_generation_superseded: 已有更新的预览生成任务")
                }
                let published = publish_directory(&staging_dir, &artifact_dir, &generation_id)?;
                let final_proxy = artifact_dir.join(
                    proxy
                        .file_name()
                        .ok_or_else(|| anyhow!("预览代理文件名无效"))?,
                );
                let final_waveform = waveform.as_ref().map(|path| {
                    artifact_dir.join(path.file_name().expect("waveform has a file name"))
                });
                let final_thumbnails = thumbnails
                    .iter()
                    .map(|path| {
                        artifact_dir.join(path.file_name().expect("thumbnail has a file name"))
                    })
                    .collect::<Vec<_>>();
                let changed = tx.execute(
                    "UPDATE media_artifacts SET status='ready',proxy_path=?2,waveform_path=?3,thumbnails_json=?4,updated_at=?5,error_message=NULL,owner_pid=NULL WHERE project_id=?1 AND generation_id=?6 AND base_version_id=?7 AND status='processing' AND owner_pid=?8",
                    params![project_id, final_proxy.to_string_lossy(), final_waveform.as_ref().map(|path| path.to_string_lossy().to_string()), serde_json::to_string(&final_thumbnails.iter().map(|path| path.to_string_lossy().to_string()).collect::<Vec<_>>())?, &updated_at, &generation_id, &base_version_id, std::process::id()],
                );
                match changed {
                    Ok(1) => {}
                    Ok(_) => {
                        return Err(rollback_preview_publication(
                            anyhow!("preview_generation_superseded: 已有更新的预览生成任务"),
                            published,
                        ));
                    }
                    Err(error) => {
                        return Err(rollback_preview_publication(error.into(), published));
                    }
                }
                if let Err(error) = tx.commit() {
                    return Err(rollback_preview_publication(error.into(), published));
                }
                published.finish()?;
                load(db, project_id)?.ok_or_else(|| anyhow!("预览资源记录不存在"))
            })();
            if let Err(error) = &publication {
                let _ = fs::remove_dir_all(&staging_dir);
                let _ = db.execute(
                    "UPDATE media_artifacts
                     SET status='failed',updated_at=?2,error_message=?3,owner_pid=NULL
                     WHERE project_id=?1 AND generation_id=?4 AND status='processing'",
                    params![project_id, now(), error.to_string(), &generation_id],
                );
            }
            publication
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_dir);
            db.execute(
                "UPDATE media_artifacts SET status='failed',updated_at=?2,error_message=?3,owner_pid=NULL WHERE project_id=?1 AND generation_id=?4",
                params![project_id, now(), error.to_string(), &generation_id],
            )?;
            Err(error)
        }
    }
}

struct PublishedDirectory {
    target: PathBuf,
    backup: Option<PathBuf>,
}

impl PublishedDirectory {
    fn rollback(self) -> Result<()> {
        if self.target.is_dir() {
            fs::remove_dir_all(&self.target).with_context(|| {
                format!(
                    "preview_rollback_failed: 无法移除未提交预览目录 {}",
                    self.target.display()
                )
            })?;
        }
        if let Some(backup) = self.backup
            && backup.is_dir()
        {
            fs::rename(&backup, &self.target).with_context(|| {
                format!(
                    "preview_rollback_failed: 旧预览仍保存在 {}，无法恢复到 {}",
                    backup.display(),
                    self.target.display()
                )
            })?;
        }
        Ok(())
    }

    fn finish(self) -> Result<()> {
        if let Some(backup) = self.backup
            && backup.is_dir()
        {
            fs::remove_dir_all(&backup).with_context(|| {
                format!(
                    "preview_cleanup_failed: 预览已提交，但无法清理旧预览备份 {}",
                    backup.display()
                )
            })?;
        }
        Ok(())
    }
}

fn publish_directory(
    staged: &Path,
    target: &Path,
    generation_id: &str,
) -> Result<PublishedDirectory> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("预览目录缺少父目录"))?;
    let backup = preview_backup_path(parent, generation_id);
    if backup.is_dir() {
        restore_directory_backup(target, &backup)?;
    }
    let backup = if target.is_dir() {
        fs::rename(target, &backup)?;
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(staged, target) {
        if let Some(backup) = backup.as_ref()
            && let Err(rollback_error) = restore_directory_backup(target, backup)
        {
            bail!(
                "preview_rollback_failed: 发布暂存目录失败：{error}；恢复旧预览失败：{rollback_error}"
            )
        }
        return Err(error.into());
    }
    Ok(PublishedDirectory {
        target: target.to_path_buf(),
        backup,
    })
}

fn preview_backup_path(project_dir: &Path, generation_id: &str) -> PathBuf {
    project_dir.join(format!("preview.backup-{generation_id}"))
}

fn restore_directory_backup(target: &Path, backup: &Path) -> Result<()> {
    if !backup.is_dir() {
        return Ok(());
    }
    if target.is_dir() {
        fs::remove_dir_all(target).with_context(|| {
            format!(
                "preview_rollback_failed: 无法移除未提交预览 {}；旧预览仍保存在 {}",
                target.display(),
                backup.display()
            )
        })?;
    }
    fs::rename(backup, target).with_context(|| {
        format!(
            "preview_rollback_failed: 旧预览仍保存在 {}，无法恢复到 {}",
            backup.display(),
            target.display()
        )
    })
}

fn rollback_preview_publication(
    cause: anyhow::Error,
    published: PublishedDirectory,
) -> anyhow::Error {
    match published.rollback() {
        Ok(()) => cause,
        Err(rollback_error) => {
            anyhow!("preview_rollback_failed: {cause}; {rollback_error}")
        }
    }
}

fn recover_preview_publication(
    db: &Connection,
    project_id: &str,
    project_dir: &Path,
    artifact_dir: &Path,
) -> Result<()> {
    let state = db
        .query_row(
            "SELECT status,generation_id,owner_pid,proxy_path
             FROM media_artifacts WHERE project_id=?1",
            [project_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<u32>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((status, Some(generation_id), owner_pid, proxy_path)) = state else {
        return Ok(());
    };
    if status == "processing"
        && owner_pid
            .is_some_and(|pid| pid == std::process::id() || crate::util::process_is_active(pid))
    {
        bail!("preview_generation_busy: 当前项目已有预览生成任务")
    }
    let backup = preview_backup_path(project_dir, &generation_id);
    let staging = project_dir.join(format!("preview.staging-{generation_id}"));
    let mut ready = status == "ready";
    if backup.is_dir() {
        if ready
            && artifact_dir.is_dir()
            && proxy_path
                .as_deref()
                .is_some_and(|path| Path::new(path).is_file())
        {
            fs::remove_dir_all(&backup).with_context(|| {
                format!(
                    "preview_cleanup_failed: 无法清理已提交预览的备份 {}",
                    backup.display()
                )
            })?;
        } else {
            restore_directory_backup(artifact_dir, &backup)?;
            if ready {
                db.execute(
                    "UPDATE media_artifacts
                     SET status='stale',updated_at=?2,
                         error_message='preview_interrupted: 已提交预览缺失，已恢复旧备份并等待重新生成。',
                         owner_pid=NULL
                     WHERE project_id=?1 AND generation_id=?3 AND status='ready'",
                    params![project_id, now(), generation_id],
                )?;
                ready = false;
            }
        }
    }
    if !ready {
        if staging.is_dir() {
            fs::remove_dir_all(&staging)?;
        }
        if status == "processing" {
            db.execute(
                "UPDATE media_artifacts
                 SET status='stale',updated_at=?2,
                     error_message='preview_interrupted: 上次预览生成在发布前中断。',
                     owner_pid=NULL
                 WHERE project_id=?1 AND generation_id=?3 AND status='processing'",
                params![project_id, now(), generation_id],
            )?;
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn preview_directory_publication_can_restore_the_previous_generation() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("preview");
        let staged = temp.path().join("preview.staging-new");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&staged).unwrap();
        fs::write(target.join("proxy.mp4"), b"old").unwrap();
        fs::write(staged.join("proxy.mp4"), b"new").unwrap();

        let published = publish_directory(&staged, &target, "new").unwrap();
        assert_eq!(fs::read(target.join("proxy.mp4")).unwrap(), b"new");
        published.rollback().unwrap();

        assert_eq!(fs::read(target.join("proxy.mp4")).unwrap(), b"old");
        assert!(!staged.exists());
    }

    #[test]
    fn preview_recovery_restores_a_crash_backup_and_marks_processing_stale() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("source.wav");
        fs::write(&media, b"audio").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("preview-recovery.db")).unwrap();
        let project = crate::project::create(&mut db, &media, None).unwrap();
        db.execute(
            "INSERT INTO media_artifacts(
                 project_id,status,source_sha256,updated_at,base_version_id,generation_id
             ) VALUES(?1,'processing',?2,'now',?3,'crashed')",
            params![
                &project.id,
                &project.media.sha256,
                project.history.current_version_id.as_deref().unwrap()
            ],
        )
        .unwrap();
        let project_dir = temp.path().join("project");
        let target = project_dir.join("preview");
        let backup = preview_backup_path(&project_dir, "crashed");
        let staging = project_dir.join("preview.staging-crashed");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(target.join("proxy.mp4"), b"uncommitted").unwrap();
        fs::write(backup.join("proxy.mp4"), b"previous").unwrap();
        fs::write(staging.join("proxy.part.mp4"), b"partial").unwrap();

        recover_preview_publication(&db, &project.id, &project_dir, &target).unwrap();

        assert_eq!(fs::read(target.join("proxy.mp4")).unwrap(), b"previous");
        assert!(!backup.exists());
        assert!(!staging.exists());
        let status: String = db
            .query_row(
                "SELECT status FROM media_artifacts WHERE project_id=?1",
                [&project.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "stale");
    }

    #[test]
    fn ready_preview_recovery_keeps_the_only_valid_backup_when_target_is_missing() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("source.wav");
        fs::write(&media, b"audio").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("ready-preview-recovery.db")).unwrap();
        let project = crate::project::create(&mut db, &media, None).unwrap();
        let project_dir = temp.path().join("project");
        let target = project_dir.join("preview");
        let proxy = target.join("proxy.mp4");
        db.execute(
            "INSERT INTO media_artifacts(
                 project_id,status,proxy_path,source_sha256,updated_at,base_version_id,generation_id
             ) VALUES(?1,'ready',?2,?3,'now',?4,'committed')",
            params![
                &project.id,
                proxy.to_string_lossy(),
                &project.media.sha256,
                project.history.current_version_id.as_deref().unwrap()
            ],
        )
        .unwrap();
        let backup = preview_backup_path(&project_dir, "committed");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("proxy.mp4"), b"only valid preview").unwrap();

        recover_preview_publication(&db, &project.id, &project_dir, &target).unwrap();

        assert_eq!(
            fs::read(target.join("proxy.mp4")).unwrap(),
            b"only valid preview"
        );
        assert!(!backup.exists());
        let status: String = db
            .query_row(
                "SELECT status FROM media_artifacts WHERE project_id=?1",
                [&project.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "stale");
    }

    #[test]
    fn preview_recovery_never_rolls_back_an_active_generation() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("source.wav");
        fs::write(&media, b"audio").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("preview-busy.db")).unwrap();
        let project = crate::project::create(&mut db, &media, None).unwrap();
        db.execute(
            "INSERT INTO media_artifacts(
                 project_id,status,source_sha256,updated_at,base_version_id,generation_id,owner_pid
             ) VALUES(?1,'processing',?2,'now',?3,'active',?4)",
            params![
                &project.id,
                &project.media.sha256,
                project.history.current_version_id.as_deref().unwrap(),
                std::process::id()
            ],
        )
        .unwrap();
        let project_dir = temp.path().join("project");
        let target = project_dir.join("preview");
        let backup = preview_backup_path(&project_dir, "active");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(target.join("proxy.mp4"), b"active-new").unwrap();
        fs::write(backup.join("proxy.mp4"), b"active-old").unwrap();

        let error = recover_preview_publication(&db, &project.id, &project_dir, &target)
            .unwrap_err()
            .to_string();

        assert!(error.contains("preview_generation_busy"));
        assert_eq!(fs::read(target.join("proxy.mp4")).unwrap(), b"active-new");
        assert_eq!(fs::read(backup.join("proxy.mp4")).unwrap(), b"active-old");
    }
}
