use super::*;

#[test]
fn approval_migration_preserves_waiting_workflows_and_rolls_back_on_failure() {
    let temp = tempfile::tempdir().unwrap();
    let mut db = Connection::open(temp.path().join("old.db")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)").unwrap();
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version <= 35)
    {
        let tx = db.transaction().unwrap();
        (migration.apply)(&tx).unwrap();
        tx.execute(
            "INSERT INTO schema_migrations VALUES(?1,'test')",
            [migration.version],
        )
        .unwrap();
        tx.commit().unwrap();
    }
    db.execute_batch("INSERT INTO projects(id,title,created_at,updated_at) VALUES('p','preserve','now','now');
        INSERT INTO tasks(id,project_id,kind,language,status,created_at) VALUES('t','p','translate','zh','queued','now');
        INSERT INTO auto_workflows(id,input_kind,input_value,project_id,model_path,translation_language,output_path,subtitle_mode,status,current_stage,agent_task_id,created_at,updated_at,ai_execution_kind,ai_authorized)
        VALUES('auto','local','private.wav','p','model','zh','out.mp4','bilingual','needs_agent','translate','t','now','now','codex',1);
        INSERT INTO auto_workflow_events(workflow_id,stage,status,progress,message,created_at) VALUES('auto','translate','needs_agent',0.5,'preserved event','now');
        CREATE INDEX ai_send_approvals_project ON projects(id);").unwrap();
    assert!(migrate(&mut db).is_err());
    assert_eq!(
        db.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        35
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM auto_workflows WHERE id='auto'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "needs_agent"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='ai_send_approvals'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP INDEX ai_send_approvals_project")
        .unwrap();
    migrate(&mut db).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT status FROM auto_workflows WHERE id='auto'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "awaiting_authorization"
    );
    assert_eq!(
        db.query_row(
            "SELECT message FROM auto_workflow_events WHERE workflow_id='auto'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "preserved event"
    );
    assert!(
        db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
}
