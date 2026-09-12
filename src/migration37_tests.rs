use super::*;

#[test]
fn transcription_upgrade_is_backed_up_and_rolls_back_without_losing_old_jobs() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old.db");
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version <= 36)
    {
        let tx = db.transaction().unwrap();
        (migration.apply)(&tx).unwrap();
        tx.execute(
            "INSERT INTO schema_migrations VALUES(?1,'fixture')",
            [migration.version],
        )
        .unwrap();
        tx.commit().unwrap();
    }
    db.execute_batch("INSERT INTO projects(id,title,created_at,updated_at) VALUES('project','preserved','now','now');
        INSERT INTO transcription_jobs(id,project_id,provider_id,endpoint,model_id,status,stage,created_at,updated_at) VALUES('old-job','project','moss_openai','http://127.0.0.1:8000','model','interrupted','interrupted','now','now');
        CREATE TABLE transcription_commands(conflict TEXT);").unwrap();
    drop(db);
    assert!(open_at(&path).is_err());
    let mut db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        36
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM transcription_jobs WHERE id='old-job'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "interrupted"
    );
    let backup = Connection::open(path.with_file_name("old.db.schema-36.bak")).unwrap();
    assert_eq!(
        backup
            .query_row("SELECT title FROM projects", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "preserved"
    );
    db.execute_batch("DROP TABLE transcription_commands")
        .unwrap();
    migrate(&mut db).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM transcription_jobs", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    db.execute(
        "INSERT INTO schema_migrations VALUES(?1,'future')",
        [CURRENT_SCHEMA_VERSION + 1],
    )
    .unwrap();
    drop(db);
    assert!(
        open_at(&path)
            .unwrap_err()
            .to_string()
            .contains("database_version_unsupported")
    );
}
