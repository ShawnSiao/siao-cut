use crate::{db, editing::*, project};
use tempfile::tempdir;

fn fixture(db: &mut rusqlite::Connection, media: &std::path::Path) -> SaveEdit {
    std::fs::write(media, b"test audio").unwrap();
    let p = project::create(db, media, Some("Editing".into())).unwrap();
    let s = project::add_segment(db, &p.id, 0.0, 1.0, "before".into(), None).unwrap();
    let version = project::current_version_id(db, &p.id).unwrap();
    SaveEdit {
        mutation_id: "m1".into(),
        expected_version_id: version.clone(),
        group_id: "g1".into(),
        draft: Draft {
            project_id: p.id,
            session_id: "session".into(),
            segment_id: s.id,
            field: "source".into(),
            base_version_id: version,
            base_text: "before".into(),
            text: "after".into(),
            revision: 1,
        },
    }
}

#[test]
fn glossary_mutation_is_versioned_idempotent_and_recoverable() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("test.db")).unwrap();
    let edit = fixture(&mut db, &temp.path().join("audio.wav"));
    let request = ProjectMutation {
        project_id: edit.draft.project_id.clone(),
        mutation_id: "glossary-once".into(),
        expected_version_id: edit.expected_version_id.clone(),
        operation: ProjectOperation::ReplaceGlossary {
            language: "en".into(),
            expected_glossary_version: 0,
            entries: vec![("字幕".into(), "subtitle".into())],
        },
    };
    let receipt = mutate(&mut db, &request).unwrap();
    assert_ne!(
        receipt["versionId"],
        serde_json::json!(edit.expected_version_id)
    );
    assert_eq!(mutate(&mut db, &request).unwrap(), receipt);
    let mut stale = request;
    stale.mutation_id = "glossary-stale".into();
    assert!(
        mutate(&mut db, &stale)
            .unwrap_err()
            .to_string()
            .contains("editing_version_conflict")
    );
    let restored = project::undo(&mut db, &stale.project_id).unwrap();
    assert!(restored.glossary.entries.is_empty());
    let redone = project::redo(&mut db, &stale.project_id).unwrap();
    assert_eq!(redone.glossary.entries[0].target, "subtitle");
    assert!(redone.glossary.version > restored.glossary.version);
}

#[test]
fn workflow_retry_creates_one_task_and_review_cannot_cross_projects() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("test.db")).unwrap();
    let edit = fixture(&mut db, &temp.path().join("audio.wav"));
    let request = ProjectMutation {
        project_id: edit.draft.project_id.clone(),
        mutation_id: "workflow-once".into(),
        expected_version_id: edit.expected_version_id,
        operation: ProjectOperation::CreateWorkflow {
            workflow_kind: "polish".into(),
            language: None,
            locale: "zh-CN".into(),
        },
    };
    let receipt = mutate(&mut db, &request).unwrap();
    assert_eq!(mutate(&mut db, &request).unwrap(), receipt);
    assert_eq!(
        project::select_tasks(&db, &request.project_id)
            .unwrap()
            .len(),
        1
    );
    let other = fixture(&mut db, &temp.path().join("other.wav"));
    let review = ProjectMutation {
        project_id: other.draft.project_id.clone(),
        mutation_id: "foreign-review".into(),
        expected_version_id: other.expected_version_id.clone(),
        operation: ProjectOperation::ReviewAll {
            task_id: receipt["taskId"].as_str().unwrap().into(),
            action: "accept".into(),
        },
    };
    assert!(
        mutate(&mut db, &review)
            .unwrap_err()
            .to_string()
            .contains("任务不属于当前项目")
    );
    assert_eq!(
        project::current_version_id(&db, &other.draft.project_id).unwrap(),
        other.expected_version_id
    );
}

#[test]
fn repeated_save_is_atomic_and_idempotent_even_after_later_edits() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("test.db")).unwrap();
    let edit = fixture(&mut db, &temp.path().join("audio.wav"));
    journal(&db, &edit.draft).unwrap();
    let receipt = save(&mut db, &edit).unwrap();
    let count = project::load(&db, &edit.draft.project_id)
        .unwrap()
        .versions
        .len();
    assert_eq!(save(&mut db, &edit).unwrap().version_id, receipt.version_id);
    assert_eq!(
        project::load(&db, &edit.draft.project_id)
            .unwrap()
            .versions
            .len(),
        count
    );
    assert!(list(&db, &edit.draft.project_id).unwrap().is_empty());
    journal(&db, &edit.draft).unwrap(); // late journal must not resurrect saved content
    assert!(list(&db, &edit.draft.project_id).unwrap().is_empty());
    project::edit_segment(
        &mut db,
        &edit.draft.project_id,
        &edit.draft.segment_id,
        "external".into(),
    )
    .unwrap();
    assert_eq!(save(&mut db, &edit).unwrap().version_id, receipt.version_id);
    assert_eq!(
        project::load(&db, &edit.draft.project_id)
            .unwrap()
            .transcript
            .segments[0]
            .text,
        "external"
    );
    let mut reused = edit.clone();
    reused.draft.text = "another".into();
    assert!(
        save(&mut db, &reused)
            .unwrap_err()
            .to_string()
            .contains("editing_mutation_reused")
    );
}

#[test]
fn stale_client_does_not_overwrite_and_journal_survives_reopen() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("test.db");
    let mut first = db::open_at(&path).unwrap();
    let edit = fixture(&mut first, &temp.path().join("audio.wav"));
    journal(&first, &edit.draft).unwrap();
    let mut second = db::open_at(&path).unwrap();
    project::edit_segment(
        &mut second,
        &edit.draft.project_id,
        &edit.draft.segment_id,
        "other client".into(),
    )
    .unwrap();
    assert!(
        save(&mut first, &edit)
            .unwrap_err()
            .to_string()
            .contains("editing_version_conflict")
    );
    drop(first);
    drop(second);
    let reopened = db::open_at(&path).unwrap();
    assert_eq!(
        list(&reopened, &edit.draft.project_id).unwrap()[0].text,
        "after"
    );
    let snapshot: String = reopened.query_row("SELECT snapshot_json FROM versions WHERE project_id=?1 ORDER BY history_index DESC LIMIT 1", [&edit.draft.project_id], |row| row.get(0)).unwrap();
    assert!(!snapshot.contains("editing_drafts"));
    assert_eq!(
        project::load(&reopened, &edit.draft.project_id)
            .unwrap()
            .transcript
            .segments[0]
            .text,
        "other client"
    );
}

#[test]
fn old_journal_or_save_cannot_remove_newer_draft() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("test.db")).unwrap();
    let edit = fixture(&mut db, &temp.path().join("audio.wav"));
    let mut newer = edit.draft.clone();
    newer.revision = 2;
    newer.text = "new typing".into();
    journal(&db, &newer).unwrap();
    journal(&db, &edit.draft).unwrap();
    save(&mut db, &edit).unwrap();
    assert_eq!(list(&db, &newer.project_id).unwrap()[0].text, "new typing");
}

#[test]
fn rebased_version_does_not_bypass_field_conflict() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("test.db")).unwrap();
    let mut edit = fixture(&mut db, &temp.path().join("audio.wav"));
    project::edit_segment(
        &mut db,
        &edit.draft.project_id,
        &edit.draft.segment_id,
        "external".into(),
    )
    .unwrap();
    edit.expected_version_id = project::current_version_id(&db, &edit.draft.project_id).unwrap();
    assert!(
        save(&mut db, &edit)
            .unwrap_err()
            .to_string()
            .contains("editing_content_conflict")
    );
}

#[test]
fn grouped_autosaves_undo_once_and_protect_a_recovery_baseline() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("test.db")).unwrap();
    let mut edit = fixture(&mut db, &temp.path().join("audio.wav"));
    let first = save(&mut db, &edit).unwrap();
    let mut recovery = edit.draft.clone();
    recovery.session_id = "another-window".into();
    recovery.base_version_id = Some(first.version_id.clone());
    recovery.revision = 10;
    journal(&db, &recovery).unwrap();
    edit.mutation_id = "m2".into();
    edit.expected_version_id = Some(first.version_id.clone());
    edit.draft.base_text = "after".into();
    edit.draft.text = "after again".into();
    edit.draft.revision = 2;
    save(&mut db, &edit).unwrap();
    let active = project::load(&db, &edit.draft.project_id).unwrap();
    assert!(!active.versions.iter().any(|v| v.id == first.version_id));
    let preserved: String = db
        .query_row(
            "SELECT snapshot_json FROM versions WHERE id=?1",
            [&first.version_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(preserved.contains("after"));
    assert_eq!(
        project::undo(&mut db, &edit.draft.project_id)
            .unwrap()
            .transcript
            .segments[0]
            .text,
        "before"
    );
    assert_eq!(
        project::redo(&mut db, &edit.draft.project_id)
            .unwrap()
            .transcript
            .segments[0]
            .text,
        "after again"
    );
}

#[test]
fn guarded_structure_is_idempotent_and_rolls_back_on_failure() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("test.db")).unwrap();
    let edit = fixture(&mut db, &temp.path().join("audio.wav"));
    let mut mutation = ProjectMutation {
        project_id: edit.draft.project_id.clone(),
        mutation_id: "split-1".into(),
        expected_version_id: edit.expected_version_id,
        operation: ProjectOperation::Split {
            segment_id: edit.draft.segment_id,
            text_offset: 3,
            at: 0.5,
        },
    };
    let result = mutate(&mut db, &mutation).unwrap();
    assert_eq!(mutate(&mut db, &mutation).unwrap(), result);
    assert_eq!(
        project::load(&db, &mutation.project_id)
            .unwrap()
            .transcript
            .segments
            .len(),
        2
    );
    mutation.mutation_id = "stale-undo".into();
    mutation.operation = ProjectOperation::Undo;
    assert!(
        mutate(&mut db, &mutation)
            .unwrap_err()
            .to_string()
            .contains("editing_version_conflict")
    );
    mutation.expected_version_id = project::current_version_id(&db, &mutation.project_id).unwrap();
    mutation.operation = ProjectOperation::Merge {
        first_id: "missing".into(),
        second_id: "also-missing".into(),
    };
    assert!(mutate(&mut db, &mutation).is_err());
    assert!(db.is_autocommit());
    assert_eq!(
        project::current_version_id(&db, &mutation.project_id).unwrap(),
        mutation.expected_version_id
    );
}

#[test]
fn transcription_review_is_owned_version_checked_and_idempotent() {
    let temp = tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("review.db")).unwrap();
    let edit = fixture(&mut db, &temp.path().join("audio.wav"));
    db.execute("INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,status,stage,created_at,updated_at) VALUES('job',?1,'moss_openai','http://127.0.0.1:8000','model','completed','completed','now','now')",[&edit.draft.project_id]).unwrap();
    db.execute("INSERT INTO transcription_runs(id,project_id,job_id,provider_id,model_id,source_sha256,result_sha256,raw_result_path,segment_count,speaker_count,created_at) VALUES('run',?1,'job','moss_openai','model','sha','result','',1,1,'now')",[&edit.draft.project_id]).unwrap();
    db.execute("INSERT INTO transcription_review_items(id,project_id,run_id,severity,kind,message,status,created_at) VALUES('item',?1,'run','warning','timing','review','open','now')",[&edit.draft.project_id]).unwrap();
    let mut request = ProjectMutation {
        project_id: edit.draft.project_id,
        mutation_id: "resolve".into(),
        expected_version_id: edit.expected_version_id,
        operation: ProjectOperation::ResolveTranscriptionReview {
            item_id: "item".into(),
            action: "resolved".into(),
        },
    };
    let other = fixture(&mut db, &temp.path().join("other.wav"));
    let mut wrong_owner: ProjectMutation =
        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
    wrong_owner.project_id = other.draft.project_id;
    wrong_owner.expected_version_id = other.expected_version_id;
    assert!(
        mutate(&mut db, &wrong_owner)
            .unwrap_err()
            .to_string()
            .contains("不属于当前项目")
    );
    let receipt = mutate(&mut db, &request).unwrap();
    assert_eq!(receipt["reviewItem"]["status"], "resolved");
    assert_eq!(mutate(&mut db, &request).unwrap(), receipt);
    request.mutation_id = "stale".into();
    request.expected_version_id = Some("old".into());
    assert!(
        mutate(&mut db, &request)
            .unwrap_err()
            .to_string()
            .contains("editing_version_conflict")
    );
}
