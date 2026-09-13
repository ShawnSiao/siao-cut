use super::desktop::*;
use super::*;
use tempfile::tempdir;

#[test]
fn result_storage_failure_does_not_leave_a_partial_success() {
    let temp = tempdir().unwrap();
    let blocked = temp.path().join("not-a-directory");
    fs::write(&blocked, b"existing data").unwrap();
    assert!(atomic_write_result(&blocked.join("candidate.json"), b"{}").is_err());
    assert_eq!(fs::read(&blocked).unwrap(), b"existing data");
    assert!(!blocked.join("candidate.json").exists());
}

#[test]
fn start_is_prompt_persistent_deduplicated_and_does_not_require_media_io() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("jobs.db");
    let mut db = db::open_at(&path).unwrap();
    let media = temp.path().join("source.wav");
    fs::write(&media, b"source").unwrap();
    let project = project::create(&mut db, &media, None).unwrap();
    fs::remove_file(media).unwrap();
    let request = TranscriptionCommand::Start {
        mutation_id: "start1".into(),
        project_id: project.id.clone(),
        expected_version_id: project.history.current_version_id.clone().unwrap(),
        model_path: "missing-model.bin".into(),
        language: "zh".into(),
    };
    let result = desktop::execute_impl(&mut db, request.clone(), false).unwrap();
    let id = result["jobSummary"]["jobId"].as_str().unwrap().to_string();
    assert_eq!(result["jobSummary"]["status"], "queued");
    drop(db);
    let mut db = db::open_at(&path).unwrap();
    let repeat = desktop::execute_impl(&mut db, request, false).unwrap();
    assert_eq!(repeat["jobSummary"]["jobId"], id);
    let other = TranscriptionCommand::Start {
        mutation_id: "start2".into(),
        project_id: project.id.clone(),
        expected_version_id: "old".into(),
        model_path: "another.bin".into(),
        language: "en".into(),
    };
    assert_eq!(
        desktop::execute_impl(&mut db, other, false).unwrap()["jobSummary"]["jobId"],
        id
    );
    assert_eq!(list(&db, Some(&project.id)).unwrap().len(), 1);
    assert!(
        project::load(&db, &project.id)
            .unwrap()
            .transcript
            .segments
            .is_empty()
    );
}

#[test]
fn an_old_attempt_cannot_finalize_or_prepare_after_retry() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("jobs.db")).unwrap();
    let media = temp.path().join("source.wav");
    fs::write(&media, b"source").unwrap();
    let project = project::create(&mut db, &media, None).unwrap();
    db.execute("INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,status,stage,base_version_id,source_sha256,created_at,updated_at) VALUES('job',?1,'moss_openai','http://localhost:8000','test','running','requesting_model',?2,?3,?4,?4)", params![project.id, project.history.current_version_id, project.media.sha256, now()]).unwrap();
    assert!(artifact_job_is_recoverable(&db, "job-attempt-1").unwrap());
    assert!(!artifact_job_is_recoverable(&db, "unknown-attempt-1").unwrap());
    let old = load(&db, "job").unwrap();
    cancel(&db, "job").unwrap();
    let next = enqueue_retry(&mut db, "job").unwrap();
    assert_eq!(next.attempt_count, old.attempt_count + 1);
    assert!(
        mark_finalizing(&db, &old)
            .unwrap_err()
            .to_string()
            .contains("attempt_superseded")
    );
    assert!(
        prepare_result(
            &mut db,
            &old,
            &temp.path().join("old.json"),
            "{}",
            &[ImportedSegment {
                id: "segment".into(),
                start: 0.0,
                end: 1.0,
                speaker: "S01".into(),
                text: "text".into()
            }],
            None
        )
        .unwrap_err()
        .to_string()
        .contains("attempt_superseded")
    );
    assert_eq!(load(&db, "job").unwrap().status, "queued");
    assert_eq!(
        db.query_row("SELECT count(*) FROM transcription_runs", [], |row| row
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
}

#[test]
fn multispeaker_start_has_the_same_version_and_replay_boundary() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("moss.db")).unwrap();
    let media = temp.path().join("audio.wav");
    fs::write(&media, b"audio").unwrap();
    let p = project::create(&mut db, &media, None).unwrap();
    let request = TranscriptionCommand::StartMultispeaker {
        mutation_id: "moss-once".into(),
        project_id: p.id.clone(),
        expected_version_id: p.history.current_version_id.clone().unwrap(),
        language: "zh".into(),
        prompt: Some("区分小爱与小艾".into()),
        hotwords: vec!["李雷".into()],
    };
    let mut stale = request.clone();
    if let TranscriptionCommand::StartMultispeaker {
        expected_version_id,
        ..
    } = &mut stale
    {
        *expected_version_id = "old".into();
    }
    assert!(
        desktop::execute_impl(&mut db, stale, false)
            .unwrap_err()
            .to_string()
            .contains("project_version_conflict")
    );
    let first = desktop::execute_impl(&mut db, request.clone(), false).unwrap();
    let repeated = desktop::execute_impl(&mut db, request, false).unwrap();
    assert_eq!(
        first["transcriptionJob"]["id"],
        repeated["transcriptionJob"]["id"]
    );
    assert_eq!(first["transcriptionJob"]["prompt"], "区分小爱与小艾");
    assert_eq!(
        first["transcriptionJob"]["hotwords"],
        serde_json::json!(["李雷"])
    );
    assert_eq!(list(&db, Some(&p.id)).unwrap().len(), 1);
}
