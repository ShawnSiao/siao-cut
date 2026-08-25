use crate::{
    artifacts,
    canvas::{self, CanvasTarget},
    contracts, db,
    export::{self, ExportOptions},
    media::{hash_file, tool_path},
    model::{ExportJob, SubtitleDelivery, SubtitleMode, TimelineMap},
    project, subtitle_style, timeline,
    util::{hidden_command, new_id, now},
};
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, ErrorCode, OptionalExtension, TransactionBehavior, params};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[cfg(test)]
mod subtitle_delivery_test;

pub struct ExportRequest<'a> {
    pub output: &'a Path,
    pub subtitle_delivery: SubtitleDelivery,
    pub language: Option<String>,
    pub subtitle_mode: SubtitleMode,
    pub allow_stale_translation: bool,
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
    subtitle_delivery: SubtitleDelivery,
    subtitle_language: Option<&'a str>,
    encoder: &'a str,
    canvas_settings: crate::model::CanvasSettings,
}

#[cfg(test)]
thread_local! {
    static FAIL_FIRST_BACKUP_CLEANUPS: std::cell::RefCell<Vec<PathBuf>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static BACKUP_CLEANUP_ATTEMPTS: std::cell::RefCell<Vec<PathBuf>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub fn create(
    db: &mut Connection,
    project_id: &str,
    request: ExportRequest<'_>,
) -> Result<ExportJob> {
    let ExportRequest {
        output,
        subtitle_delivery,
        language,
        subtitle_mode,
        allow_stale_translation,
        start_delay_ms,
        job_id,
    } = request;
    let output_extension = output
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    let expected_extension = if subtitle_delivery == SubtitleDelivery::EmbeddedMkv {
        "mkv"
    } else {
        "mp4"
    };
    if output_extension.as_deref() != Some(expected_extension) {
        bail!("所选字幕交付方式要求使用 .{expected_extension} 视频扩展名")
    }
    let burn_subtitles = subtitle_delivery == SubtitleDelivery::Burned;
    let project = project::load(db, project_id)?;
    let quality_options = ExportOptions {
        format: "ass",
        language: language.as_deref(),
        subtitle_mode,
        include_cuts: false,
        allow_stale_translation,
    };
    let report = export::audit_for_options(&project, &quality_options);
    if report["ready"] != true {
        bail!("导出前审计未通过，请先处理媒体或字幕问题")
    }
    if timeline::build(&project).output_duration <= 0.001 {
        bail!("全部内容都被软剪辑移除，无法导出空视频")
    }
    if subtitle_delivery != SubtitleDelivery::None {
        export::validate_subtitle_mode(&project, &quality_options)?;
    }
    let source = Path::new(&project.media.source_path).canonicalize()?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let output_name = output
        .file_name()
        .ok_or_else(|| anyhow!("视频导出路径缺少文件名"))?;
    let output_absolute = parent.canonicalize()?.join(output_name);
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
    let base_version_id = project
        .history
        .current_version_id
        .as_deref()
        .ok_or_else(|| anyhow!("export_project_version_missing: 项目没有可绑定的当前版本"))?;
    let source_sha256 = hash_file(&source)?;
    if source_sha256 != project.media.sha256 {
        bail!("export_source_changed: 项目原始媒体校验值已变化")
    }
    if let Some(job_id) = job_id.as_deref() {
        validate_job_id(job_id)?;
    }
    let job = ExportJob {
        id: job_id.unwrap_or_else(|| new_id("x")),
        project_id: project_id.to_owned(),
        output_path: output_absolute.to_string_lossy().to_string(),
        status: "queued".into(),
        stage_code: Some("queued".into()),
        progress: 0.0,
        burn_subtitles,
        subtitle_delivery,
        language,
        bilingual: subtitle_mode == SubtitleMode::Bilingual,
        subtitle_mode,
        allow_stale_translation,
        canvas_settings: project.canvas_settings,
        subtitle_style: project.subtitle_style.clone(),
        cancel_requested_at: None,
        error_message: None,
        error_code: None,
        manifest_path: None,
        created_at: created_at.clone(),
        updated_at: created_at.clone(),
        completed_at: None,
        worker_pid: None,
    };
    let inserted = db.execute(
        "INSERT INTO export_jobs(id,project_id,output_path,status,progress,burn_subtitles,language,bilingual,subtitle_mode,canvas_aspect_ratio,canvas_framing,subtitle_style_json,created_at,updated_at,allow_stale_translation,base_version_id,source_sha256,subtitle_delivery) VALUES(?1,?2,?3,'queued',0,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12,?13,?14,?15)",
        params![&job.id, &job.project_id, &job.output_path, job.burn_subtitles, &job.language, job.bilingual, job.subtitle_mode.as_str(), job.canvas_settings.aspect_ratio.as_str(), job.canvas_settings.framing.as_str(), subtitle_style::storage_json(&job.subtitle_style)?, &job.created_at, job.allow_stale_translation, base_version_id, source_sha256, job.subtitle_delivery.as_str()],
    );
    match inserted {
        Ok(_) => {}
        Err(error) if error.sqlite_error_code() == Some(ErrorCode::ConstraintViolation) => {
            bail!("export_target_busy: 同一输出路径已有活动导出任务")
        }
        Err(error) => return Err(error.into()),
    }
    if let Err(error) = spawn_worker(&job.id, start_delay_ms) {
        let failed_at = now();
        db.execute(
            "UPDATE export_jobs
             SET status='failed',error_message=?2,completed_at=?3,updated_at=?3
             WHERE id=?1 AND status='queued' AND worker_pid IS NULL",
            params![&job.id, error.to_string(), failed_at],
        )?;
        return Err(error);
    }
    Ok(job)
}

pub fn load(db: &Connection, job_id: &str) -> Result<ExportJob> {
    db.query_row(
        "SELECT id,project_id,output_path,status,progress,burn_subtitles,language,bilingual,subtitle_mode,canvas_aspect_ratio,canvas_framing,subtitle_style_json,cancel_requested_at,error_message,manifest_path,created_at,updated_at,completed_at,worker_pid,allow_stale_translation,subtitle_delivery FROM export_jobs WHERE id=?1",
        [job_id],
        |row| {
            let status = row.get::<_, String>(3)?;
            let error_message = row.get::<_, Option<String>>(13)?;
            Ok(ExportJob {
                id: row.get(0)?,
                project_id: row.get(1)?,
                output_path: row.get(2)?,
                stage_code: Some(status.clone()),
                status: status.clone(),
                progress: row.get(4)?,
                burn_subtitles: row.get(5)?,
                subtitle_delivery: SubtitleDelivery::parse(&row.get::<_, String>(20)?)
                    .ok_or(rusqlite::Error::InvalidQuery)?,
                language: row.get(6)?,
                bilingual: row.get(7)?,
                subtitle_mode: SubtitleMode::parse(&row.get::<_, String>(8)?)
                    .ok_or(rusqlite::Error::InvalidQuery)?,
                allow_stale_translation: row.get(19)?,
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
                error_code: crate::model::background_error_code(
                    &status,
                    error_message.as_deref(),
                ),
                error_message,
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
    if !["failed", "interrupted", "cancelled"].contains(&job.status.as_str()) {
        bail!("视频导出任务当前状态不能重试：{}", job.status)
    }
    let project = project::load(db, &job.project_id)?;
    let base_version_id = project
        .history
        .current_version_id
        .ok_or_else(|| anyhow!("export_project_version_missing: 项目没有可绑定的当前版本"))?;
    let source_sha256 = hash_file(Path::new(&project.media.source_path))?;
    if source_sha256 != project.media.sha256 {
        bail!("export_source_changed: 项目原始媒体校验值已变化")
    }
    let changed = db.execute(
        "UPDATE export_jobs
         SET status='queued',progress=0,cancel_requested_at=NULL,error_message=NULL,
             completed_at=NULL,worker_pid=NULL,updated_at=?2,
             base_version_id=?3,source_sha256=?4
         WHERE id=?1 AND status IN ('failed','interrupted','cancelled')",
        params![job_id, now(), base_version_id, source_sha256],
    );
    match changed {
        Ok(1) => {}
        Ok(_) => bail!("视频导出任务当前状态不能重试：{}", job.status),
        Err(error) if error.sqlite_error_code() == Some(ErrorCode::ConstraintViolation) => {
            bail!("export_target_busy: 同一输出路径已有活动导出任务")
        }
        Err(error) => return Err(error.into()),
    }
    if let Err(error) = spawn_worker(job_id, None) {
        db.execute(
            "UPDATE export_jobs
             SET status='failed',error_message=?2,completed_at=?3,updated_at=?3
             WHERE id=?1 AND status='queued' AND worker_pid IS NULL",
            params![job_id, error.to_string(), now()],
        )?;
        return Err(error);
    }
    load(db, job_id)
}

pub fn reconcile_interrupted(db: &Connection) -> Result<()> {
    let jobs = db
        .prepare(
            "SELECT id,status,worker_pid,updated_at
             FROM export_jobs WHERE status IN ('queued','running')",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<u32>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, status, worker_pid, updated_at) in jobs {
        let stale = chrono::DateTime::parse_from_rfc3339(&updated_at)
            .map(|time| {
                chrono::Utc::now()
                    .signed_duration_since(time.with_timezone(&chrono::Utc))
                    .num_seconds()
                    >= 5
            })
            .unwrap_or(true);
        let worker_alive = worker_pid.is_some_and(crate::util::process_is_active);
        if stale && !worker_alive {
            db.execute(
                "UPDATE export_jobs
                 SET status='interrupted',error_message='上次导出进程已中断，可以从 App 重新开始。',
                     worker_pid=NULL,updated_at=?2
                 WHERE id=?1 AND status=?3 AND updated_at=?4 AND worker_pid IS ?5",
                params![id, now(), status, updated_at, worker_pid],
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
    let claimed = db.execute(
        "UPDATE export_jobs SET status='running',worker_pid=?2,updated_at=?3 WHERE id=?1 AND status='queued' AND worker_pid IS NULL",
        params![job_id, std::process::id(), now()],
    )?;
    if claimed == 0 {
        bail!("export_job_already_running: 导出任务已被其他工作进程领取")
    }
    if let Some(delay) = start_delay_ms {
        thread::sleep(Duration::from_millis(delay));
    }
    if let Err(error) = run(&mut db, job_id) {
        if let Ok(job) = load(&db, job_id) {
            let output = Path::new(&job.output_path);
            let partial = partial_path(output, job_id);
            if partial.is_file() {
                let _ = fs::remove_file(partial);
            }
            if let Some(target) = sidecar_target(output, job.subtitle_delivery) {
                let partial = staging_path(&target, job_id, "subtitle");
                if partial.is_file() {
                    let _ = fs::remove_file(partial);
                }
            }
        }
        if finish_worker_error(&db, job_id, &error)? {
            return Ok(());
        }
        return Err(error);
    }
    Ok(())
}

fn run(db: &mut Connection, job_id: &str) -> Result<()> {
    let job = load(db, job_id)?;
    let (base_version_id, expected_source_sha256): (String, String) = db
        .query_row(
            "SELECT base_version_id,source_sha256 FROM export_jobs WHERE id=?1",
            [job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("export_binding_missing: 导出任务缺少项目版本或媒体校验绑定")?;
    if job.cancel_requested_at.is_some() {
        finish_cancelled(db, job_id)?;
        return Ok(());
    }
    db.execute(
        "UPDATE export_jobs SET status='running',progress=0.01,worker_pid=?2,updated_at=?3,error_message=NULL WHERE id=?1",
        params![job_id, std::process::id(), now()],
    )?;
    let project = project::load(db, &job.project_id)?;
    if project.history.current_version_id.as_deref() != Some(base_version_id.as_str()) {
        bail!("export_project_changed: 项目在导出排队期间发生变化，请重新创建或重试导出")
    }
    let map = timeline::build(&project);
    let source = Path::new(&project.media.source_path);
    if project.media.sha256 != expected_source_sha256
        || hash_file(source)? != expected_source_sha256
    {
        bail!("export_source_changed: 导出绑定的原始媒体内容发生变化")
    }
    let output = PathBuf::from(&job.output_path);
    let partial = partial_path(&output, job_id);
    let ffmpeg = tool_path("SIAOCUT_FFMPEG", "ffmpeg");
    let encoders = artifacts::available_video_encoders(&ffmpeg)?;
    let has_video = artifacts::has_stream(source, "v:0")?;
    let has_audio = artifacts::has_stream(source, "a:0")?;
    let sidecar_target = sidecar_target(&output, job.subtitle_delivery);
    let sidecar_partial = sidecar_target
        .as_deref()
        .map(|target| staging_path(target, job_id, "subtitle"));
    let subtitle_format = match job.subtitle_delivery {
        SubtitleDelivery::Burned => Some("ass"),
        SubtitleDelivery::EmbeddedMp4 | SubtitleDelivery::EmbeddedMkv => Some("srt"),
        SubtitleDelivery::SidecarSrt => Some("srt"),
        SubtitleDelivery::SidecarVtt => Some("vtt"),
        SubtitleDelivery::None => None,
    };
    let subtitle_path = if let Some(format) = subtitle_format {
        let path = if let Some(path) = sidecar_partial.clone() {
            path
        } else {
            let dir = db::home_dir().join("cache").join("exports");
            fs::create_dir_all(&dir)?;
            dir.join(format!("{}.{}", staging_token(&job.id), format))
        };
        let mut export_project = project.clone();
        export_project.subtitle_style = job.subtitle_style.clone();
        fs::write(
            &path,
            export::render(
                &export_project,
                &ExportOptions {
                    format,
                    language: job.language.as_deref(),
                    subtitle_mode: job.subtitle_mode,
                    include_cuts: false,
                    allow_stale_translation: job.allow_stale_translation,
                },
            )?,
        )?;
        Some(path)
    } else {
        None
    };

    let encoder = encode_with_fallback(&encoders, &partial, |encoder| {
        let command = build_command(CommandSpec {
            ffmpeg: &ffmpeg,
            source,
            output: &partial,
            map: &map,
            has_video,
            has_audio,
            subtitle_path: subtitle_path.as_deref(),
            subtitle_delivery: job.subtitle_delivery,
            subtitle_language: job.language.as_deref().or((!project
                .transcript
                .source_language
                .is_empty())
            .then_some(project.transcript.source_language.as_str())),
            encoder,
            canvas_settings: job.canvas_settings,
        })?;
        run_encode_attempt(db, job_id, &map, command)
    })?;
    let Some(encoder) = encoder else {
        finish_cancelled(db, job_id)?;
        return Ok(());
    };
    let manifest_path = output.with_extension("siaocut.json");
    let manifest_partial = staging_path(&manifest_path, job_id, "manifest");
    let manifest = json!({
        "apiVersion": "0.1",
        "projectId": project.id,
        "source": { "path": project.media.source_path, "sha256": project.media.sha256 },
        "output": { "path": output, "sha256": hash_file(&partial)?, "bytes": fs::metadata(&partial)?.len() },
        "baseVersionId": base_version_id,
        "timeline": map,
        "encoder": encoder,
        "videoEncoding": artifacts::export_video_encoding_manifest(&encoder),
        "burnSubtitles": job.burn_subtitles,
        "subtitleDelivery": job.subtitle_delivery,
        "subtitleSidecar": sidecar_target,
        "language": job.language,
        "bilingual": job.bilingual,
        "subtitleMode": job.subtitle_mode,
        "canvasSettings": job.canvas_settings,
        "subtitleStyle": job.subtitle_style,
        "createdAt": now()
    });
    fs::write(&manifest_partial, serde_json::to_vec_pretty(&manifest)?)?;
    let completed_at = now();
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current_version_id = project::current_version_id(&tx, &job.project_id)?;
    let (status, cancel_requested, recorded_source): (String, bool, String) = tx.query_row(
        "SELECT e.status,e.cancel_requested_at IS NOT NULL,m.sha256
         FROM export_jobs e JOIN media m ON m.project_id=e.project_id
         WHERE e.id=?1",
        [job_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if status != "running" || cancel_requested {
        bail!("export_cancelled: 导出任务在发布结果前已取消")
    }
    if current_version_id.as_deref() != Some(base_version_id.as_str()) {
        bail!("export_project_changed: 导出期间项目发生变化，结果未覆盖目标文件")
    }
    if recorded_source != expected_source_sha256 || hash_file(source)? != expected_source_sha256 {
        bail!("export_source_changed: 导出期间原始媒体发生变化，结果未覆盖目标文件")
    }
    let mut publications = Vec::new();
    publications.push(publish_staged(&partial, &output, job_id)?);
    if let (Some(staged), Some(target)) = (sidecar_partial.as_deref(), sidecar_target.as_deref()) {
        match publish_staged(staged, target, job_id) {
            Ok(published) => publications.push(published),
            Err(error) => return Err(rollback_publications(error, publications)),
        }
    }
    match publish_staged(&manifest_partial, &manifest_path, job_id) {
        Ok(published) => publications.push(published),
        Err(error) => return Err(rollback_publications(error, publications)),
    }
    let updated = tx.execute(
        "UPDATE export_jobs SET status='completed',progress=1,manifest_path=?2,worker_pid=NULL,updated_at=?3,completed_at=?3 WHERE id=?1 AND status='running' AND cancel_requested_at IS NULL",
        params![job_id, manifest_path.to_string_lossy(), completed_at],
    );
    match updated {
        Ok(1) => {}
        Ok(_) => {
            return Err(rollback_publications(
                anyhow!("export_cancelled: 导出任务在发布结果时状态已变化"),
                publications,
            ));
        }
        Err(error) => {
            return Err(rollback_publications(error.into(), publications));
        }
    }
    if let Err(error) = tx.commit() {
        return Err(rollback_publications(error.into(), publications));
    }
    finish_publications_after_commit(publications);
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
        subtitle_delivery,
        subtitle_language,
        encoder,
        canvas_settings,
    } = spec;
    if !has_video && !has_audio {
        bail!("媒体中没有可导出的音视频流")
    }
    let mut command = hidden_command(ffmpeg);
    command.args(["-y", "-hide_banner", "-loglevel", "error"]);
    let (video_input, audio_input, next_input_index) = if has_video && has_audio {
        command.arg("-i").arg(source);
        (0, 0, 1)
    } else if has_video {
        command
            .arg("-i")
            .arg(source)
            .args(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo"]);
        (0, 1, 2)
    } else {
        command.arg("-i").arg(source).args([
            "-f",
            "lavfi",
            "-i",
            "color=c=0x101414:s=1280x720:r=30",
        ]);
        (1, 0, 2)
    };
    let embedded_subtitle_input = if matches!(
        subtitle_delivery,
        SubtitleDelivery::EmbeddedMp4 | SubtitleDelivery::EmbeddedMkv
    ) {
        let path = subtitle_path.ok_or_else(|| anyhow!("内嵌字幕导出缺少字幕暂存文件"))?;
        command.arg("-i").arg(path);
        Some(next_input_index)
    } else {
        None
    };

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
    let video_label = if subtitle_delivery == SubtitleDelivery::Burned {
        let path = subtitle_path.ok_or_else(|| anyhow!("烧录字幕导出缺少字幕暂存文件"))?;
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
        .args(artifacts::export_video_encoder_args(encoder))
        .args(["-c:a", "aac", "-b:a", "160k"]);
    if let Some(input) = embedded_subtitle_input {
        command.args(["-map", &format!("{input}:0")]);
        match subtitle_delivery {
            SubtitleDelivery::EmbeddedMp4 => {
                command.args(["-c:s", "mov_text"]);
            }
            SubtitleDelivery::EmbeddedMkv => {
                command.args(["-c:s", "srt"]);
            }
            _ => unreachable!(),
        }
        command
            .arg("-metadata:s:s:0")
            .arg(format!("language={}", subtitle_language.unwrap_or("und")));
    }
    if output
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("mp4"))
    {
        command.args(["-movflags", "+faststart"]);
    }
    command
        .args(["-progress", "pipe:1", "-nostats"])
        .arg(output);
    Ok(command)
}

#[derive(Debug, PartialEq, Eq)]
enum EncodeAttemptOutcome {
    Succeeded,
    Failed(String),
    Cancelled,
}

fn encode_with_fallback(
    encoders: &[String],
    partial: &Path,
    mut attempt: impl FnMut(&str) -> Result<EncodeAttemptOutcome>,
) -> Result<Option<String>> {
    if encoders.is_empty() {
        bail!("FFmpeg 缺少可用的视频编码器")
    }
    let mut failures = Vec::new();
    for encoder in encoders {
        remove_attempt_partial(partial).with_context(|| {
            format!("无法清理 {encoder} 编码前的暂存文件 {}", partial.display())
        })?;
        let outcome = match attempt(encoder) {
            Ok(outcome) => outcome,
            Err(error) => {
                if let Err(cleanup_error) = remove_attempt_partial(partial) {
                    bail!(
                        "{encoder} 编码异常：{error}；且无法清理暂存文件 {}：{cleanup_error}",
                        partial.display()
                    )
                }
                return Err(error.context(format!("{encoder} 编码异常")));
            }
        };
        match outcome {
            EncodeAttemptOutcome::Succeeded if partial.is_file() => {
                return Ok(Some(encoder.clone()));
            }
            EncodeAttemptOutcome::Succeeded => {
                failures.push(format!("{encoder}: FFmpeg 未生成视频暂存文件"));
            }
            EncodeAttemptOutcome::Failed(detail) => {
                failures.push(format!("{encoder}: {detail}"));
            }
            EncodeAttemptOutcome::Cancelled => {
                remove_attempt_partial(partial).with_context(|| {
                    format!(
                        "取消 {encoder} 编码后无法清理暂存文件 {}",
                        partial.display()
                    )
                })?;
                return Ok(None);
            }
        }
        remove_attempt_partial(partial).with_context(|| {
            format!("{encoder} 编码失败后无法清理暂存文件 {}", partial.display())
        })?;
    }
    bail!(
        "FFmpeg 视频导出失败（已按顺序尝试 {}）：{}",
        encoders.join(" -> "),
        failures.join("；")
    )
}

fn remove_attempt_partial(partial: &Path) -> Result<()> {
    match fs::remove_file(partial) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn run_encode_attempt(
    db: &Connection,
    job_id: &str,
    map: &TimelineMap,
    mut command: Command,
) -> Result<EncodeAttemptOutcome> {
    let cancel_requested: bool = db.query_row(
        "SELECT cancel_requested_at IS NOT NULL FROM export_jobs WHERE id=?1",
        [job_id],
        |row| row.get(0),
    )?;
    if cancel_requested {
        return Ok(EncodeAttemptOutcome::Cancelled);
    }

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

    let mut last_progress = db
        .query_row(
            "SELECT progress FROM export_jobs WHERE id=?1",
            [job_id],
            |row| row.get::<_, f64>(0),
        )?
        .clamp(0.01, 0.99);
    let status: Result<Option<_>> = 'monitor: loop {
        while let Ok(line) = progress_rx.try_recv() {
            if let Some(value) = line.strip_prefix("out_time_us=")
                && let Ok(microseconds) = value.parse::<f64>()
            {
                let progress =
                    (microseconds / 1_000_000.0 / map.output_duration).clamp(last_progress, 0.99);
                if progress - last_progress >= 0.01 {
                    last_progress = progress;
                    if let Err(error) = db.execute(
                        "UPDATE export_jobs SET progress=?2,updated_at=?3 WHERE id=?1",
                        params![job_id, progress, now()],
                    ) {
                        let _ = child.kill();
                        let _ = child.wait();
                        break 'monitor Err(error.into());
                    }
                }
            }
        }
        let cancel_requested: bool = match db.query_row(
            "SELECT cancel_requested_at IS NOT NULL FROM export_jobs WHERE id=?1",
            [job_id],
            |row| row.get(0),
        ) {
            Ok(value) => value,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(error.into());
            }
        };
        if cancel_requested {
            let _ = child.kill();
            let _ = child.wait();
            break Ok(None);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(Some(status)),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(error.into());
            }
        }
        thread::sleep(Duration::from_millis(200));
    };
    let _ = progress_reader.join();
    let stderr = error_reader.join().unwrap_or_default();
    let Some(status) = status? else {
        return Ok(EncodeAttemptOutcome::Cancelled);
    };
    if status.success() {
        Ok(EncodeAttemptOutcome::Succeeded)
    } else {
        let detail = if stderr.trim().is_empty() {
            format!("FFmpeg 退出状态 {status}")
        } else {
            stderr.trim().to_owned()
        };
        Ok(EncodeAttemptOutcome::Failed(detail))
    }
}

fn escape_filter_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

fn staging_token(job_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(job_id.as_bytes());
    let digest = format!("{:x}", hash.finalize());
    digest[..16].to_owned()
}

fn sidecar_target(output: &Path, delivery: SubtitleDelivery) -> Option<PathBuf> {
    match delivery {
        SubtitleDelivery::SidecarSrt => Some(output.with_extension("srt")),
        SubtitleDelivery::SidecarVtt => Some(output.with_extension("vtt")),
        _ => None,
    }
}

fn validate_job_id(job_id: &str) -> Result<()> {
    if job_id.is_empty()
        || job_id.len() > 64
        || !job_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!(
            "export_job_id_invalid: 导出任务 ID 只能包含 1 到 64 个 ASCII 字母、数字、连字符或下划线"
        )
    }
    Ok(())
}

fn staging_path(output: &Path, job_id: &str, kind: &str) -> PathBuf {
    let stem = output
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("siaocut-export");
    let extension = output
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("tmp");
    output.with_file_name(format!(
        "{stem}.siaocut-{}.{}.part.{extension}",
        staging_token(job_id),
        kind
    ))
}

fn partial_path(output: &Path, job_id: &str) -> PathBuf {
    staging_path(output, job_id, "video")
}

struct PublishedFile {
    target: PathBuf,
    backup: Option<PathBuf>,
    committed: bool,
}

impl PublishedFile {
    fn rollback(mut self) -> Result<()> {
        if self.target.is_file() {
            fs::remove_file(&self.target).with_context(|| {
                format!(
                    "export_rollback_failed: 无法移除未提交输出 {}",
                    self.target.display()
                )
            })?;
        }
        if let Some(backup) = self.backup.take()
            && backup.is_file()
        {
            fs::rename(&backup, &self.target).with_context(|| {
                format!(
                    "export_rollback_failed: 旧输出仍保存在 {}，无法恢复到 {}",
                    backup.display(),
                    self.target.display()
                )
            })?;
        }
        Ok(())
    }

    fn finish_after_commit(&mut self) {
        self.committed = true;
        let _ = self.cleanup_committed_backup();
    }

    fn cleanup_committed_backup(&mut self) -> Result<()> {
        let Some(backup) = self.backup.clone() else {
            return Ok(());
        };
        match remove_committed_backup(&backup) {
            Ok(()) => {
                self.backup = None;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.backup = None;
                Ok(())
            }
            Err(error) => Err(anyhow::Error::new(error).context(format!(
                "export_cleanup_failed: 导出已提交，但无法清理旧输出备份 {}",
                backup.display()
            ))),
        }
    }
}

impl Drop for PublishedFile {
    fn drop(&mut self) {
        if self.committed {
            let _ = self.cleanup_committed_backup();
        }
    }
}

fn finish_publications_after_commit(publications: impl IntoIterator<Item = PublishedFile>) {
    let mut publications = publications.into_iter().collect::<Vec<_>>();
    for publication in &mut publications {
        publication.finish_after_commit();
    }
}

fn remove_committed_backup(backup: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    {
        BACKUP_CLEANUP_ATTEMPTS.with(|attempts| {
            attempts.borrow_mut().push(backup.to_path_buf());
        });
        let should_fail = FAIL_FIRST_BACKUP_CLEANUPS.with(|paths| {
            let mut paths = paths.borrow_mut();
            paths
                .iter()
                .position(|path| path == backup)
                .map(|index| paths.swap_remove(index))
                .is_some()
        });
        if should_fail {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected export backup cleanup failure",
            ));
        }
    }
    fs::remove_file(backup)
}

fn publish_staged(staged: &Path, target: &Path, job_id: &str) -> Result<PublishedFile> {
    if !staged.is_file() {
        bail!("export_staging_missing: 导出暂存文件不存在")
    }
    let backup = staging_path(target, job_id, "backup");
    if backup.is_file() {
        restore_file_backup(target, &backup)?;
    }
    let backup = if target.is_file() {
        fs::rename(target, &backup)?;
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(staged, target) {
        if let Some(backup) = backup.as_ref()
            && let Err(rollback_error) = restore_file_backup(target, backup)
        {
            bail!(
                "export_rollback_failed: 发布暂存文件失败：{error}；恢复旧输出失败：{rollback_error}"
            )
        }
        return Err(error.into());
    }
    Ok(PublishedFile {
        target: target.to_path_buf(),
        backup,
        committed: false,
    })
}

fn restore_file_backup(target: &Path, backup: &Path) -> Result<()> {
    if !backup.is_file() {
        return Ok(());
    }
    if target.is_file() {
        fs::remove_file(target).with_context(|| {
            format!(
                "export_rollback_failed: 无法移除未提交输出 {}；旧输出仍保存在 {}",
                target.display(),
                backup.display()
            )
        })?;
    }
    fs::rename(backup, target).with_context(|| {
        format!(
            "export_rollback_failed: 旧输出仍保存在 {}，无法恢复到 {}",
            backup.display(),
            target.display()
        )
    })
}

fn rollback_publications(
    cause: anyhow::Error,
    publications: impl IntoIterator<Item = PublishedFile>,
) -> anyhow::Error {
    let failures = publications
        .into_iter()
        .filter_map(|published| published.rollback().err())
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    if failures.is_empty() {
        cause
    } else {
        anyhow!("export_rollback_failed: {cause}; {}", failures.join("; "))
    }
}

fn finish_cancelled(db: &Connection, job_id: &str) -> Result<()> {
    let timestamp = now();
    db.execute(
        "UPDATE export_jobs
         SET status='cancelled',error_message=NULL,worker_pid=NULL,updated_at=?2,completed_at=?2
         WHERE id=?1",
        params![job_id, timestamp],
    )?;
    Ok(())
}

fn finish_worker_error(db: &Connection, job_id: &str, error: &anyhow::Error) -> Result<bool> {
    let cancel_requested: bool = db.query_row(
        "SELECT cancel_requested_at IS NOT NULL FROM export_jobs WHERE id=?1",
        [job_id],
        |row| row.get(0),
    )?;
    if contracts::error_code(error) == "export_cancelled" || cancel_requested {
        finish_cancelled(db, job_id)?;
        return Ok(true);
    }
    let timestamp = now();
    db.execute(
        "UPDATE export_jobs
         SET status='failed',error_message=?2,worker_pid=NULL,updated_at=?3,completed_at=?3
         WHERE id=?1 AND status='running'",
        params![job_id, error.to_string(), timestamp],
    )?;
    Ok(false)
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
    fn export_staging_paths_are_unique_per_job_and_keep_the_target_extension() {
        let output = Path::new("render.mp4");
        let first = partial_path(output, "export-job-one");
        let second = partial_path(output, "export-job-two");

        assert_ne!(first, second);
        assert_eq!(
            first.extension().and_then(|value| value.to_str()),
            Some("mp4")
        );
        assert!(first.to_string_lossy().contains(".video.part.mp4"));
        assert!(second.to_string_lossy().contains(".video.part.mp4"));
    }

    #[test]
    fn late_export_cancellation_remains_cancelled_in_worker_error_cleanup() {
        let database = Connection::open_in_memory().unwrap();
        database
            .execute_batch(
                "CREATE TABLE export_jobs(
                     id TEXT PRIMARY KEY,
                     status TEXT NOT NULL,
                     cancel_requested_at TEXT,
                     error_message TEXT,
                     worker_pid INTEGER,
                     updated_at TEXT,
                     completed_at TEXT
                 );
                 INSERT INTO export_jobs(
                     id,status,cancel_requested_at,updated_at
                 ) VALUES('late-cancel','running','now','now');",
            )
            .unwrap();

        let handled =
            finish_worker_error(&database, "late-cancel", &anyhow!("ffmpeg exited")).unwrap();

        assert!(handled);
        let (status, error_message): (String, Option<String>) = database
            .query_row(
                "SELECT status,error_message FROM export_jobs WHERE id='late-cancel'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "cancelled");
        assert!(error_message.is_none());
    }

    #[test]
    fn final_export_falls_back_after_encoder_failure_and_records_the_successful_encoder() {
        let temp = tempdir().unwrap();
        let partial = temp.path().join("render.video.part.mp4");
        fs::write(&partial, b"stale-before-first-attempt").unwrap();
        let encoders = vec![
            "h264_mf".to_owned(),
            "libx264".to_owned(),
            "mpeg4".to_owned(),
        ];
        let mut attempted = Vec::new();

        let selected = encode_with_fallback(&encoders, &partial, |encoder| {
            attempted.push(encoder.to_owned());
            assert!(
                !partial.exists(),
                "each encoder must start without the previous attempt's partial"
            );
            match encoder {
                "h264_mf" => {
                    fs::write(&partial, b"failed-hardware-output").unwrap();
                    Ok(EncodeAttemptOutcome::Failed(
                        "hardware encoder initialization failed".into(),
                    ))
                }
                "libx264" => {
                    fs::write(&partial, b"software-success").unwrap();
                    Ok(EncodeAttemptOutcome::Succeeded)
                }
                _ => panic!("fallback continued after the first successful encoder"),
            }
        })
        .unwrap()
        .unwrap();

        let manifest = json!({
            "encoder": selected.clone(),
            "videoEncoding": artifacts::export_video_encoding_manifest(&selected)
        });
        assert_eq!(attempted, vec!["h264_mf".to_owned(), "libx264".to_owned()]);
        assert_eq!(fs::read(&partial).unwrap(), b"software-success");
        assert_eq!(manifest["encoder"], "libx264");
        assert_eq!(manifest["videoEncoding"]["encoder"], "libx264");
        assert_eq!(manifest["videoEncoding"]["rateControl"], "crf");
    }

    #[test]
    fn cancelling_an_encoder_attempt_stops_fallback_and_removes_partial_output() {
        let temp = tempdir().unwrap();
        let partial = temp.path().join("render.video.part.mp4");
        let encoders = vec!["h264_mf".to_owned(), "libx264".to_owned()];
        let mut attempted = Vec::new();

        let selected = encode_with_fallback(&encoders, &partial, |encoder| {
            attempted.push(encoder.to_owned());
            fs::write(&partial, b"cancelled-output").unwrap();
            Ok(EncodeAttemptOutcome::Cancelled)
        })
        .unwrap();

        assert!(selected.is_none());
        assert_eq!(attempted, vec!["h264_mf".to_owned()]);
        assert!(!partial.exists());
    }

    #[test]
    fn rejects_export_job_ids_that_could_escape_the_cache_directory() {
        for job_id in [
            r"..\..\victim",
            "../../victim",
            r"C:\tmp\victim",
            "/tmp/victim",
            "contains space",
            "",
        ] {
            let error = validate_job_id(job_id).unwrap_err().to_string();
            assert!(error.contains("export_job_id_invalid"), "{job_id}: {error}");
        }
        validate_job_id("x-safe_job-123").unwrap();
    }

    #[test]
    fn staged_publication_can_restore_the_previous_target() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("output.mp4");
        let staged = temp.path().join("new-output.mp4");
        fs::write(&target, b"previous").unwrap();
        fs::write(&staged, b"replacement").unwrap();

        let published = publish_staged(&staged, &target, "rollback-job").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"replacement");
        assert!(!staged.exists());
        published.rollback().unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"previous");
    }

    #[test]
    fn committed_export_cleanup_failures_do_not_fail_the_worker_and_retry_both_backups() {
        let temp = tempdir().unwrap();
        let video_target = temp.path().join("output.mp4");
        let video_staged = temp.path().join("new-output.mp4");
        let manifest_target = temp.path().join("output.siaocut.json");
        let manifest_staged = temp.path().join("new-output.siaocut.json");
        fs::write(&video_target, b"previous video").unwrap();
        fs::write(&video_staged, b"committed video").unwrap();
        fs::write(&manifest_target, b"previous manifest").unwrap();
        fs::write(&manifest_staged, b"committed manifest").unwrap();

        let published_video = publish_staged(&video_staged, &video_target, "cleanup-job").unwrap();
        let published_manifest =
            publish_staged(&manifest_staged, &manifest_target, "cleanup-job").unwrap();
        let video_backup = staging_path(&video_target, "cleanup-job", "backup");
        let manifest_backup = staging_path(&manifest_target, "cleanup-job", "backup");
        assert!(video_backup.is_file());
        assert!(manifest_backup.is_file());

        FAIL_FIRST_BACKUP_CLEANUPS.with(|paths| {
            *paths.borrow_mut() = vec![video_backup.clone(), manifest_backup.clone()];
        });
        BACKUP_CLEANUP_ATTEMPTS.with(|attempts| attempts.borrow_mut().clear());

        let mut database = Connection::open_in_memory().unwrap();
        database
            .execute_batch(
                "CREATE TABLE export_jobs(id TEXT PRIMARY KEY, status TEXT NOT NULL);
                 INSERT INTO export_jobs(id,status) VALUES('cleanup-job','running');",
            )
            .unwrap();
        let worker_result: Result<()> = (|| {
            let tx = database.transaction()?;
            tx.execute(
                "UPDATE export_jobs SET status='completed' WHERE id='cleanup-job'",
                [],
            )?;
            tx.commit()?;
            finish_publications_after_commit([published_manifest, published_video]);
            Ok(())
        })();

        assert!(
            worker_result.is_ok(),
            "post-commit backup cleanup must not fail the worker: {worker_result:?}"
        );
        assert_eq!(
            database
                .query_row(
                    "SELECT status FROM export_jobs WHERE id='cleanup-job'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "completed"
        );
        assert_eq!(fs::read(&video_target).unwrap(), b"committed video");
        assert_eq!(fs::read(&manifest_target).unwrap(), b"committed manifest");

        let attempts = BACKUP_CLEANUP_ATTEMPTS.with(|attempts| attempts.borrow().clone());
        assert_eq!(attempts.len(), 4);
        assert!(attempts[..2].contains(&video_backup));
        assert!(attempts[..2].contains(&manifest_backup));
        assert_eq!(
            attempts
                .iter()
                .filter(|attempt| *attempt == &video_backup)
                .count(),
            2
        );
        assert_eq!(
            attempts
                .iter()
                .filter(|attempt| *attempt == &manifest_backup)
                .count(),
            2
        );
        assert!(!video_backup.exists());
        assert!(!manifest_backup.exists());
    }

    #[test]
    fn retry_restores_a_crash_backup_before_starting_a_new_publication() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("output.mp4");
        let staged = temp.path().join("retry-output.mp4");
        let backup = staging_path(&target, "crashed-job", "backup");
        fs::write(&target, b"uncommitted output").unwrap();
        fs::write(&backup, b"previous output").unwrap();
        fs::write(&staged, b"retry output").unwrap();

        let published = publish_staged(&staged, &target, "crashed-job").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"retry output");
        published.rollback().unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"previous output");
        assert!(!backup.exists());
    }

    #[test]
    fn stale_export_binding_never_touches_an_existing_output() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("stale-export.wav");
        let output = temp.path().join("output.mp4");
        fs::write(&media, b"audio").unwrap();
        fs::write(&output, b"existing output").unwrap();
        let mut database = crate::db::open_at(&temp.path().join("stale-export.db")).unwrap();
        let created = project::create(&mut database, &media, None).unwrap();
        let base_version = created.history.current_version_id.as_deref().unwrap();
        database
            .execute(
                "INSERT INTO export_jobs(
                     id,project_id,output_path,status,progress,burn_subtitles,bilingual,
                     created_at,updated_at,base_version_id,source_sha256
                 ) VALUES(
                     'stale-export-job',?1,?2,'running',0,0,0,
                     'now','now',?3,?4
                 )",
                params![
                    &created.id,
                    output.to_string_lossy(),
                    base_version,
                    &created.media.sha256
                ],
            )
            .unwrap();
        project::add_segment(
            &mut database,
            &created.id,
            0.0,
            1.0,
            "newer project content".into(),
            None,
        )
        .unwrap();

        let error = run(&mut database, "stale-export-job")
            .unwrap_err()
            .to_string();

        assert!(error.contains("export_project_changed"));
        assert_eq!(fs::read(&output).unwrap(), b"existing output");
        assert!(!partial_path(&output, "stale-export-job").exists());
    }

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
        crate::subtitle_style::set(&mut database, &created.id, "emphasis", "center", None, None)
            .unwrap();
        let job = create(
            &mut database,
            &created.id,
            ExportRequest {
                output: &temp.path().join("snapshot.mp4"),
                subtitle_delivery: SubtitleDelivery::Burned,
                language: None,
                subtitle_mode: SubtitleMode::Source,
                allow_stale_translation: false,
                start_delay_ms: Some(60_000),
                job_id: Some("x-style-snapshot".into()),
            },
        )
        .unwrap();
        assert_eq!(job.subtitle_style.preset, SubtitleStylePreset::Emphasis);
        assert_eq!(job.subtitle_style.position, SubtitlePosition::Center);

        crate::subtitle_style::set(&mut database, &created.id, "compact", "bottom", None, None)
            .unwrap();
        let reloaded = load(&database, &job.id).unwrap();
        assert_eq!(
            reloaded.subtitle_style.preset,
            SubtitleStylePreset::Emphasis
        );
        assert_eq!(reloaded.subtitle_style.position, SubtitlePosition::Center);
    }

    #[test]
    fn subtitle_style_real_media_burns_wrapped_karaoke_ass() {
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
        let text = "Today I want to explain why we are building a local-first editing workbench.";
        let segment =
            project::add_segment(&mut database, &created.id, 0.1, 1.8, text.into(), None).unwrap();
        database
            .execute(
                "UPDATE projects SET source_language='en' WHERE id=?1",
                [&created.id],
            )
            .unwrap();
        let words = text.split_whitespace().collect::<Vec<_>>();
        let word_duration = 1.7 / words.len() as f64;
        for (ordinal, word) in words.into_iter().enumerate() {
            let start = 0.1 + ordinal as f64 * word_duration;
            database
                .execute(
                    "INSERT INTO words(id,project_id,segment_id,start_seconds,end_seconds,text,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![
                        format!("w{ordinal}"),
                        &created.id,
                        &segment.id,
                        start,
                        start + word_duration * 0.9,
                        word,
                        ordinal as i64
                    ],
                )
                .unwrap();
        }
        crate::subtitle_style::set(&mut database, &created.id, "emphasis", "bottom", None, None)
            .unwrap();
        let styled = project::load(&database, &created.id).unwrap();
        let subtitle_path = temp.path().join("styled.ass");
        let ass = export::render(
            &styled,
            &export::ExportOptions {
                format: "ass",
                language: None,
                subtitle_mode: SubtitleMode::Source,
                include_cuts: false,
                allow_stale_translation: false,
            },
        )
        .unwrap();
        assert!(ass.contains("Style: Primary,Microsoft YaHei UI,46"));
        assert!(ass.contains(",4,2,2,76,76,54,1"));
        assert_eq!(ass.matches("\\N").count(), 1);
        assert!(ass.contains("{\\kf"));
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
            subtitle_delivery: SubtitleDelivery::Burned,
            subtitle_language: None,
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
                "ass": {"playResolution": "1920x1080", "primaryStyle": "60px", "outline": "4px", "safeMargin": "10%", "wrappedLines": 2, "karaoke": true},
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
            subtitle_delivery: SubtitleDelivery::None,
            subtitle_language: None,
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
        assert!(arguments.contains("-c:v mpeg4 -q:v 2 -pix_fmt yuv420p"));
        assert!(arguments.contains("-progress pipe:1"));
    }

    #[test]
    fn final_export_command_does_not_reuse_the_proxy_bitrate_cap() {
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
            subtitle_delivery: SubtitleDelivery::None,
            subtitle_language: None,
            encoder: "h264_mf",
            canvas_settings: Default::default(),
        })
        .unwrap();
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(arguments.contains(
            "-c:v h264_mf -rate_control quality -quality 90 -scenario archive -pix_fmt yuv420p"
        ));
        assert!(!arguments.contains("-b:v 3M"));
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
            subtitle_delivery: SubtitleDelivery::None,
            subtitle_language: None,
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
                allow_stale_translation: false,
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
            subtitle_delivery: SubtitleDelivery::Burned,
            subtitle_language: None,
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
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
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
                allow_stale_translation: false,
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
            subtitle_delivery: SubtitleDelivery::Burned,
            subtitle_language: None,
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
                allow_stale_translation: false,
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
