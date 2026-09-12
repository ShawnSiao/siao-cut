use crate::{db, project_query};
use rusqlite::params;

#[test]
fn hundred_projects_and_ten_thousand_segments_use_bounded_summary_pages() {
    let temp = tempfile::tempdir().unwrap();
    let mut db = db::open_at(&temp.path().join("performance.db")).unwrap();
    let tx = db.transaction().unwrap();
    for index in 0..100 {
        let id = format!("p-{index:03}");
        tx.execute("INSERT INTO projects(id,title,created_at,updated_at) VALUES(?1,?1,'2026-09-12','2026-09-12')",[&id]).unwrap();
        // Invalid media artifacts/snapshots prove the summary query doesn't call Project::load.
        tx.execute("INSERT INTO media(project_id,source_path,sha256,extension,duration_seconds) VALUES(?1,'missing.wav','hash','wav',30000)",[&id]).unwrap();
    }
    for index in 0..10000 {
        tx.execute("INSERT INTO segments(id,project_id,start_seconds,end_seconds,text) VALUES(?1,'p-000',?2,?3,?4)",params![format!("s-{index}"),index as f64*3.0,index as f64*3.0+2.0,format!("字幕 {index} Test subtitle")]).unwrap();
    }
    tx.execute("UPDATE projects SET subtitle_style_json='invalid-json'", [])
        .unwrap();
    tx.commit().unwrap();
    let started = std::time::Instant::now();
    let first = project_query::list(&db, 0, None).unwrap();
    let elapsed = started.elapsed();
    assert_eq!(first.total, 100);
    assert_eq!(first.items.len(), 50);
    assert_eq!(first.next_offset, Some(50));
    assert_eq!(first.items[0].segment_count, 10000);
    let json = serde_json::to_string(&first).unwrap();
    assert!(!json.contains("transcript"));
    assert!(!json.contains("missing.wav"));
    assert!(json.len() < 20_000);
    let second = project_query::list(&db, 50, None).unwrap();
    assert_eq!(second.items.len(), 50);
    assert_eq!(second.next_offset, None);
    assert!(
        first
            .items
            .iter()
            .all(|a| second.items.iter().all(|b| a.id != b.id))
    );
    assert!(project_query::list(&db, 0, Some(51)).is_err());
    eprintln!(
        "ProjectSummary: 100 projects / 10000 segments, page={} bytes, elapsed={elapsed:?}",
        json.len()
    );
}
