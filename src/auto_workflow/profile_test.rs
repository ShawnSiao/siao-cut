use super::*;
use crate::project;
use std::{fs, path::PathBuf};
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, Connection, PathBuf, PathBuf, PathBuf) {
    let temp = tempdir().unwrap();
    let db = db::open_at(&temp.path().join("profile.db")).unwrap();
    let media = temp.path().join("talk.wav");
    let model = temp.path().join("model.bin");
    let output = temp.path().join("out.mp4");
    fs::write(&media, b"audio").unwrap();
    fs::write(&model, b"model").unwrap();
    (temp, db, media, model, output)
}

fn insert_profile(
    db: &mut Connection,
    media: PathBuf,
    model: PathBuf,
    output: PathBuf,
    profile: WorkflowProfile,
) -> AutoWorkflow {
    insert(
        db,
        StartRequest {
            input: WorkflowInput::Local {
                media,
                title: Some("Profile fixture".into()),
            },
            model,
            transcribe_language: Some("auto".into()),
            translation_language: None,
            output,
            burn_subtitles: false,
            subtitle_mode: SubtitleMode::Source,
            profile,
            start_delay_ms: None,
            instruction_locale: "zh-CN".into(),
            translation_execution: None,
        },
    )
    .unwrap()
}

#[test]
fn profiles_use_declared_stage_boundaries() {
    let cases = [
        (WorkflowProfile::Draft, "transcribe", 0.15),
        (WorkflowProfile::Draft, "audit", 0.70),
        (WorkflowProfile::Draft, "export", 0.75),
        (WorkflowProfile::Balanced, "transcribe", 0.15),
        (WorkflowProfile::Balanced, "suggestions", 0.45),
        (WorkflowProfile::Balanced, "translate", 0.50),
        (WorkflowProfile::Balanced, "review", 0.50),
        (WorkflowProfile::Balanced, "audit", 0.75),
        (WorkflowProfile::Balanced, "export", 0.80),
        (WorkflowProfile::Delivery, "transcribe", 0.10),
        (WorkflowProfile::Delivery, "analyze", 0.40),
        (WorkflowProfile::Delivery, "suggestions", 0.55),
        (WorkflowProfile::Delivery, "translate", 0.60),
        (WorkflowProfile::Delivery, "review", 0.60),
        (WorkflowProfile::Delivery, "audit", 0.80),
        (WorkflowProfile::Delivery, "export", 0.85),
    ];
    for (profile, stage, expected) in cases {
        assert_eq!(stage_start(profile, stage), expected, "{profile:?} {stage}");
    }
}

#[test]
fn profiles_route_fixed_stages_and_draft_rejects_translation() {
    let (_temp, mut db, media, model, output) = fixture();
    let invalid = insert(
        &mut db,
        StartRequest {
            input: WorkflowInput::Local {
                media: media.clone(),
                title: None,
            },
            model: model.clone(),
            transcribe_language: Some("auto".into()),
            translation_language: Some("en".into()),
            output: output.clone(),
            burn_subtitles: false,
            subtitle_mode: SubtitleMode::Translated,
            profile: WorkflowProfile::Draft,
            start_delay_ms: None,
            instruction_locale: "zh-CN".into(),
            translation_execution: None,
        },
    )
    .unwrap_err();
    assert!(
        invalid
            .to_string()
            .contains("auto_workflow_profile_invalid")
    );

    let draft = insert_profile(&mut db, media, model, output, WorkflowProfile::Draft);
    run_import(&mut db, &draft).unwrap();
    let imported = load(&db, &draft.id).unwrap();
    let project_id = imported.project_id.clone().unwrap();
    project::add_segment(&mut db, &project_id, 0.0, 1.0, "plain speech".into(), None).unwrap();
    project::snapshot(&db, &project_id, "whisper.cpp 本地转录").unwrap();
    run_transcribe(&mut db, &imported).unwrap();
    let routed = load(&db, &draft.id).unwrap();
    assert_eq!(
        (routed.current_stage.as_str(), routed.progress),
        ("audit", 0.70)
    );
    for table in ["edits", "tasks"] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE project_id=?1");
        assert_eq!(
            db.query_row(&sql, [&project_id], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn delivery_requires_review_and_reuses_its_audio_analysis_job() {
    let (_temp, mut db, media, model, output) = fixture();
    let workflow = insert_profile(&mut db, media, model, output, WorkflowProfile::Delivery);
    run_import(&mut db, &workflow).unwrap();
    let imported = load(&db, &workflow.id).unwrap();
    let project_id = imported.project_id.clone().unwrap();
    project::add_segment(&mut db, &project_id, 0.0, 1.0, "plain speech".into(), None).unwrap();
    project::snapshot(&db, &project_id, "whisper.cpp 本地转录").unwrap();
    run_transcribe(&mut db, &imported).unwrap();
    let transcribed = load(&db, &workflow.id).unwrap();
    assert_eq!(
        (transcribed.current_stage.as_str(), transcribed.progress),
        ("analyze", 0.40)
    );

    db.execute(
        "INSERT INTO audio_analysis_jobs(id,project_id,status,progress,created_at,updated_at,completed_at)
         VALUES('audio-delivery',?1,'completed',1,'now','now','now')",
        [&project_id],
    ).unwrap();
    db.execute(
        "UPDATE auto_workflows SET audio_analysis_job_id='audio-delivery' WHERE id=?1",
        [&workflow.id],
    )
    .unwrap();
    assert!(!poll_audio_analysis(&db, &load(&db, &workflow.id).unwrap()).unwrap());
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM audio_analysis_jobs WHERE project_id=?1",
            [&project_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let suggestions = load(&db, &workflow.id).unwrap();
    assert_eq!(
        (suggestions.current_stage.as_str(), suggestions.progress),
        ("suggestions", 0.55)
    );
    assert!(run_suggestions(&mut db, &suggestions).unwrap());
    let review = load(&db, &workflow.id).unwrap();
    assert_eq!(
        (
            review.status.as_str(),
            review.current_stage.as_str(),
            review.progress
        ),
        ("needs_review", "review", 0.60)
    );
    queue_for_resume(&db, &review, "audit").unwrap();
    let confirmed = load(&db, &workflow.id).unwrap();
    assert_eq!(
        (confirmed.current_stage.as_str(), confirmed.progress),
        ("audit", 0.80)
    );
}

#[test]
fn cancelling_delivery_cancels_the_recorded_audio_child_without_clearing_its_id() {
    let (_temp, mut db, media, model, output) = fixture();
    let workflow = insert_profile(&mut db, media, model, output, WorkflowProfile::Delivery);
    run_import(&mut db, &workflow).unwrap();
    let project_id = load(&db, &workflow.id).unwrap().project_id.unwrap();
    db.execute(
        "INSERT INTO audio_analysis_jobs(id,project_id,status,progress,created_at,updated_at)
         VALUES('audio-cancel',?1,'queued',0,'now','now')",
        [&project_id],
    )
    .unwrap();
    db.execute(
        "UPDATE auto_workflows SET current_stage='analyze',status='running',audio_analysis_job_id='audio-cancel' WHERE id=?1",
        [&workflow.id],
    ).unwrap();

    let cancelled = cancel(&mut db, &workflow.id).unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(
        cancelled.audio_analysis_job_id.as_deref(),
        Some("audio-cancel")
    );
    assert_eq!(
        audio_analysis::load(&db, "audio-cancel").unwrap().status,
        "cancelled"
    );
}
