use crate::{db, project, translation, util::now};
use rusqlite::params;
use std::fs;
use tempfile::tempdir;

#[test]
fn manual_translation_edit_tracks_the_current_source_and_is_recoverable() {
    let temp = tempdir().unwrap();
    let media = temp.path().join("talk.wav");
    fs::write(&media, b"audio").unwrap();
    let mut database = db::open_at(&temp.path().join("core.db")).unwrap();
    let created = project::create(&mut database, &media, None).unwrap();
    let segment = project::add_segment(
        &mut database,
        &created.id,
        0.0,
        2.0,
        "原始文本".into(),
        None,
    )
    .unwrap();
    let timestamp = now();
    database.execute(
        "INSERT INTO translations(project_id,language,status,updated_at,glossary_version) VALUES(?1,'en','current',?2,0)",
        params![&created.id, &timestamp],
    ).unwrap();
    database.execute(
        "INSERT INTO translation_segments(project_id,language,segment_id,text,source_hash,status,updated_at) VALUES(?1,'en',?2,'Old translation',?3,'current',?4)",
        params![&created.id, &segment.id, translation::source_hash("原始文本"), &timestamp],
    ).unwrap();

    project::edit_segment(
        &mut database,
        &created.id,
        &segment.id,
        "修正后的原文".into(),
    )
    .unwrap();
    let stale = project::load(&database, &created.id).unwrap();
    assert_eq!(stale.translations["en"].segments[0].status, "stale");
    let expected_version = stale.history.current_version_id.unwrap();

    let updated = translation::edit_segment(
        &mut database,
        &created.id,
        &segment.id,
        "en",
        "Corrected translation".into(),
        &expected_version,
    )
    .unwrap();
    assert_eq!(updated.translations["en"].status, "current");
    assert_eq!(
        updated.translations["en"].segments[0].text,
        "Corrected translation"
    );
    assert_eq!(updated.translations["en"].segments[0].status, "current");
    assert_eq!(
        updated.translations["en"].segments[0].source_hash,
        translation::source_hash("修正后的原文")
    );

    let conflict = translation::edit_segment(
        &mut database,
        &created.id,
        &segment.id,
        "en",
        "Lost update".into(),
        &expected_version,
    )
    .unwrap_err();
    assert!(
        conflict
            .to_string()
            .contains("translation_version_conflict")
    );

    let undone = project::undo(&mut database, &created.id).unwrap();
    assert_eq!(
        undone.translations["en"].segments[0].text,
        "Old translation"
    );
    assert_eq!(undone.translations["en"].segments[0].status, "stale");
}
