use super::*;
use crate::db;

fn setup() -> (tempfile::TempDir, Connection, AiSendSpec) {
    let temp = tempfile::tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("test.db")).unwrap();
    let media = temp.path().join("private.wav");
    std::fs::write(&media, b"test").unwrap();
    let p = project::create(&mut db, &media, Some("private title".into())).unwrap();
    project::add_segment(&mut db, &p.id, 0., 2., "Actual subtitle 字幕".into(), None).unwrap();
    let spec = AiSendSpec {
        project_id: p.id.clone(),
        expected_version_id: project::current_version_id(&db, &p.id).unwrap().unwrap(),
        kind: "polish".into(),
        language: None,
        instruction_locale: "zh-CN".into(),
        task_id: None,
        target: ExecutionTarget::Codex,
    };
    (temp, db, spec)
}

fn fake_enqueue(
    db: &mut Connection,
    task_id: &str,
    _: ExecutionTarget,
) -> Result<crate::model::AgentRun> {
    let p = project::load(db, &tasks::project_id(db, task_id)?)?;
    let task = p.tasks.iter().find(|t| t.id == task_id).unwrap();
    let run_id = new_id("ar");
    db.execute("INSERT INTO agent_runs(id,task_id,project_id,status,base_version_id,progress,current_batch,batch_count,timeout_seconds,created_at,updated_at,attempt_count) VALUES(?1,?2,?3,'queued',?4,0,0,1,30,?5,?5,1)",
        params![run_id, task_id, p.id, task.base_version_id, now()])?;
    agent_runner::load(db, &run_id)
}

#[test]
fn preview_contains_actual_scope_but_creates_no_task_or_run() {
    let (_temp, mut db, spec) = setup();
    let result = preview(&mut db, spec.clone()).unwrap();
    let payload: Value = serde_json::from_str(&result.payload_json).unwrap();
    assert_eq!(payload["segments"][0]["text"], "Actual subtitle 字幕");
    assert_eq!(result.character_count, 18);
    assert!(!result.receiver_verified);
    for forbidden in [
        "private.wav",
        "private title",
        "projectId",
        "taskId",
        "leaseId",
        "apiKey",
    ] {
        assert!(!result.payload_json.contains(forbidden), "{forbidden}");
    }
    assert!(
        project::load(&db, &spec.project_id)
            .unwrap()
            .tasks
            .is_empty()
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM agent_runs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn repeated_confirmation_after_reopen_returns_the_same_run_and_dispatch_is_fenced() {
    let (temp, mut db, spec) = setup();
    let preview = preview(&mut db, spec.clone()).unwrap();
    let (run, launch) = consume_with(&mut db, &preview.approval_id, fake_enqueue).unwrap();
    assert!(launch);
    drop(db);
    let mut db = db::open_at(&temp.path().join("test.db")).unwrap();
    let (same, launch) = consume_with(&mut db, &preview.approval_id, |_, _, _| {
        panic!("must not enqueue twice")
    })
    .unwrap();
    assert!(!launch);
    assert_eq!(same.id, run.id);
    let project = project::load(&db, &spec.project_id).unwrap();
    let payload = tasks::build_claim_payload(&db, &project, &project.tasks[0]).unwrap();
    assert_eq!(
        payload_for_run(&db, &run, &payload).unwrap(),
        serde_json::from_str::<Value>(&preview.payload_json).unwrap()
    );
    authorize_dispatch(&db, &run, "batch-1").unwrap();
    assert!(
        validate_resume(&db, &run.id)
            .unwrap_err()
            .to_string()
            .contains("ai_dispatch_uncertain")
    );
    assert!(
        authorize_dispatch(&db, &run, "batch-1")
            .unwrap_err()
            .to_string()
            .contains("ai_dispatch_uncertain")
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM agent_runs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn changed_project_or_context_cannot_consume_old_consent() {
    let (_temp, mut db, spec) = setup();
    let approved = preview(&mut db, spec.clone()).unwrap();
    project::add_segment(
        &mut db,
        &spec.project_id,
        3.,
        4.,
        "Another subtitle".into(),
        None,
    )
    .unwrap();
    let error = consume_with(&mut db, &approved.approval_id, |_, _, _| {
        panic!("stale consent must not launch")
    })
    .unwrap_err();
    assert!(error.to_string().contains("ai_approval_stale"));
    assert!(
        project::load(&db, &spec.project_id)
            .unwrap()
            .tasks
            .is_empty()
    );
}

#[test]
fn different_payload_and_configuration_are_rejected_before_dispatch() {
    let (_temp, mut db, spec) = setup();
    let mut approved = preview(&mut db, spec).unwrap();
    let (run, _) = consume_with(&mut db, &approved.approval_id, fake_enqueue).unwrap();
    let mut payload: Value = serde_json::from_str(&approved.payload_json).unwrap();
    payload["segments"][0]["text"] = json!("unapproved text");
    assert!(payload_for_run(&db, &run, &payload).is_err());
    approved.configuration_revision = "different configuration".into();
    db.execute(
        "UPDATE ai_send_approvals SET preview_json=?2 WHERE id=?1",
        params![
            approved.approval_id,
            serde_json::to_string(&approved).unwrap()
        ],
    )
    .unwrap();
    assert!(authorize_dispatch(&db, &run, "batch").is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM ai_approval_dispatches", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn enqueue_failure_rolls_back_task_and_preserves_retryable_approval() {
    let (_temp, mut db, spec) = setup();
    let approved = preview(&mut db, spec.clone()).unwrap();
    assert!(
        consume_with(&mut db, &approved.approval_id, |_, _, _| bail!(
            "simulated disk failure"
        ))
        .is_err()
    );
    assert!(
        project::load(&db, &spec.project_id)
            .unwrap()
            .tasks
            .is_empty()
    );
    assert!(
        consume_with(&mut db, &approved.approval_id, fake_enqueue)
            .unwrap()
            .1
    );
}

#[test]
fn repreview_of_a_waiting_task_uses_the_latest_text_and_replaces_it_only_on_confirmation() {
    let (_temp, mut db, mut spec) = setup();
    let old =
        tasks::create_with_locale(&mut db, &spec.project_id, "polish", None, "zh-CN").unwrap();
    project::add_segment(
        &mut db,
        &spec.project_id,
        3.,
        4.,
        "Text added while awaiting consent".into(),
        None,
    )
    .unwrap();
    spec.expected_version_id = project::current_version_id(&db, &spec.project_id)
        .unwrap()
        .unwrap();
    spec.task_id = Some(old.id.clone());
    let approved = preview(&mut db, spec.clone()).unwrap();
    assert!(
        approved
            .payload_json
            .contains("Text added while awaiting consent")
    );
    assert_eq!(project::load(&db, &spec.project_id).unwrap().tasks.len(), 1);
    let (run, _) = consume_with(&mut db, &approved.approval_id, fake_enqueue).unwrap();
    assert_ne!(run.task_id, old.id);
    let current = project::load(&db, &spec.project_id).unwrap();
    assert_eq!(
        current
            .tasks
            .iter()
            .find(|t| t.id == old.id)
            .unwrap()
            .status,
        "cancelled"
    );
    let task = current.tasks.iter().find(|t| t.id == run.task_id).unwrap();
    let payload = tasks::build_claim_payload(&db, &current, task).unwrap();
    assert_eq!(
        payload_for_run(&db, &run, &payload).unwrap()["segments"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
