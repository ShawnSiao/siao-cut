use rusqlite::Connection;

#[test]
fn subtitle_delivery_migration_preserves_legacy_burned_jobs() {
    let database = Connection::open_in_memory().unwrap();
    database
        .execute_batch(
            "CREATE TABLE export_jobs (
                id TEXT PRIMARY KEY,
                burn_subtitles INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO export_jobs(id,burn_subtitles) VALUES('burned',1),('plain',0);",
        )
        .unwrap();

    database
        .execute_batch(include_str!("migrations/33_subtitle_delivery.sql"))
        .unwrap();

    let burned: String = database
        .query_row(
            "SELECT subtitle_delivery FROM export_jobs WHERE id='burned'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let plain: String = database
        .query_row(
            "SELECT subtitle_delivery FROM export_jobs WHERE id='plain'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(burned, "burned");
    assert_eq!(plain, "none");
}
