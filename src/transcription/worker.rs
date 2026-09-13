use super::*;
use std::{thread, time::Duration};

pub(super) fn spawn_worker(job_id: &str, attempt: u32, start_delay_ms: Option<u64>) -> Result<()> {
    let delay = start_delay_ms.unwrap_or(0).to_string();
    let attempt = attempt.to_string();
    let args = ["__transcription_worker", job_id, &delay, &attempt];
    crate::util::spawn_detached_current(&args)?;
    Ok(())
}

pub fn run_worker(job_id: &str, start_delay_ms: Option<u64>, attempt: u32) -> Result<()> {
    let mut database = db::open()?;
    let claimed = database.execute(
        "UPDATE transcription_jobs SET status='running',stage='preparing_audio',worker_pid=?2,updated_at=?3 WHERE id=?1 AND status='queued' AND attempt_count=?4 AND cancel_requested_at IS NULL",
        params![job_id, std::process::id(), now(), attempt],
    )?;
    if claimed == 0 {
        return Ok(());
    }
    if let Some(delay) = start_delay_ms {
        thread::sleep(Duration::from_millis(delay));
    }
    let result = execute_job(&mut database, job_id, attempt);
    match result {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().starts_with("transcription_cancelled") => {
            let timestamp = now();
            database.execute(
                "UPDATE transcription_jobs SET status='cancelled',stage='cancelled',worker_pid=NULL,completed_at=?2,updated_at=?2 WHERE id=?1 AND status IN ('running','finalizing') AND attempt_count=?3",
                params![job_id, timestamp, attempt],
            )?;
            Ok(())
        }
        Err(error) => {
            let timestamp = now();
            database.execute(
                "UPDATE transcription_jobs SET status='failed',stage='failed',error_message=?2,worker_pid=NULL,completed_at=?3,updated_at=?3 WHERE id=?1 AND status IN ('running','finalizing') AND attempt_count=?4 AND cancel_requested_at IS NULL",
                params![job_id, error.to_string(), timestamp, attempt],
            )?;
            Err(error)
        }
    }
}

fn execute_job(db: &mut Connection, job_id: &str, attempt: u32) -> Result<String> {
    let mut job = checked_load(db, job_id, attempt)?;
    if job.attempt_count != attempt {
        bail!("transcription_attempt_superseded: 旧执行代次已失效")
    }
    ensure_not_cancelled(db, &job)?;
    let project_value = project::load(db, &job.project_id)?;
    let source_hash = hash_file(Path::new(&project_value.media.source_path))?;
    if job.source_sha256.as_deref() != Some(&source_hash)
        || source_hash != project_value.media.sha256
    {
        bail!("transcription_source_changed: 素材已变化，请重新定位后启动新任务")
    }
    if let Some((run_id, raw_path, result_sha256)) = prepared_run(db, job_id)? {
        mark_finalizing(db, &job)?;
        let raw = read_verified_result(&raw_path, &result_sha256)?;
        let imported = parsed_segments(&job.provider_id, &raw)?;
        job = checked_load(db, job_id, attempt)?;
        finalize_result(
            db,
            &job,
            &project_value,
            &run_id,
            &raw_path,
            &raw,
            &imported,
            None,
        )?;
        return Ok(run_id);
    }

    let raw_path = result_path(&format!("{job_id}-attempt-{attempt}"));
    if raw_path.is_file()
        && let Ok(raw) = fs::read_to_string(&raw_path)
        && let Ok(imported) = parsed_segments(&job.provider_id, &raw)
    {
        mark_finalizing(db, &job)?;
        job = checked_load(db, job_id, attempt)?;
        let run_id = prepare_result(db, &job, &raw_path, &raw, &imported, None)?;
        finalize_result(
            db,
            &job,
            &project_value,
            &run_id,
            &raw_path,
            &raw,
            &imported,
            None,
        )?;
        return Ok(run_id);
    }
    if raw_path.is_file() {
        let _ = fs::remove_file(&raw_path);
    }

    let cache_dir = crate::db::home_dir().join("cache").join("transcription");
    fs::create_dir_all(&cache_dir)?;
    let wav_path = cache_dir.join(format!("{}-attempt-{attempt}.wav", job.id));
    let _wav_guard = TemporaryFile::new(wav_path.clone());
    extract_audio(Path::new(&project_value.media.source_path), &wav_path)?;
    let input_audio_sha256 = hash_file(&wav_path)?;
    db.execute(
        "UPDATE transcription_jobs SET input_audio_sha256=?2,updated_at=?3 WHERE id=?1 AND status='running' AND attempt_count=?4 AND cancel_requested_at IS NULL",
        params![job_id, input_audio_sha256, now(), attempt],
    )?;
    ensure_not_cancelled(db, &job)?;
    db.execute(
        "UPDATE transcription_jobs SET stage='requesting_model',updated_at=?2 WHERE id=?1 AND attempt_count=?3 AND cancel_requested_at IS NULL",
        params![job_id, now(), attempt],
    )?;
    let raw = request_moss(&job, &wav_path)?;
    ensure_not_cancelled(db, &job)?;
    mark_finalizing(db, &job)?;
    let imported = parsed_segments(&job.provider_id, &raw)?;
    atomic_write_result(&raw_path, raw.as_bytes())?;
    job = checked_load(db, job_id, attempt)?;
    let run_id = prepare_result(db, &job, &raw_path, &raw, &imported, None)?;
    finalize_result(
        db,
        &job,
        &project_value,
        &run_id,
        &raw_path,
        &raw,
        &imported,
        None,
    )?;
    Ok(run_id)
}

fn checked_load(db: &Connection, id: &str, attempt: u32) -> Result<TranscriptionJob> {
    let job = load(db, id)?;
    if job.attempt_count != attempt {
        bail!("transcription_attempt_superseded: 旧执行代次已失效")
    }
    ensure_not_cancelled(db, &job)?;
    Ok(job)
}
