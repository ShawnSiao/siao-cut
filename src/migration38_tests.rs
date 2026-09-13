use super::*;
#[test]
fn lifecycle_receipt_upgrade_rolls_back_and_keeps_a_readable_backup() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("legacy.db");
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for migration in MIGRATIONS.iter().filter(|m| m.version <= 37) {
        let tx = db.transaction().unwrap();
        (migration.apply)(&tx).unwrap();
        tx.execute(
            "INSERT INTO schema_migrations VALUES(?1,'fixture')",
            [migration.version],
        )
        .unwrap();
        tx.commit().unwrap();
    }
    db.execute_batch("INSERT INTO projects(id,title,created_at,updated_at) VALUES('old','kept','now','now'); CREATE TABLE project_commands(conflict TEXT)").unwrap();
    drop(db);
    assert!(open_at(&path).is_err());
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        37
    );
    let backup = Connection::open(path.with_file_name("legacy.db.schema-37.bak")).unwrap();
    assert_eq!(
        backup
            .query_row("SELECT title FROM projects", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "kept"
    );
    db.execute_batch("DROP TABLE project_commands").unwrap();
    drop(db);
    let db = open_at(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT title FROM projects", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "kept"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM project_commands", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
