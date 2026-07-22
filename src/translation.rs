use crate::{
    model::{Glossary, GlossaryEntry, Project, Segment, TranslationSegment},
    util::now,
};
use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub fn source_hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub fn load_glossary(db: &Connection, project_id: &str) -> Result<Glossary> {
    let (version, updated_at): (i64, String) = db
        .query_row(
            "SELECT current_version,updated_at FROM project_glossaries WHERE project_id=?1",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .unwrap_or((0, String::new()));
    let entries = db
        .prepare(
            "SELECT language,source,target FROM glossary_entries
             WHERE project_id=?1 AND version=?2 ORDER BY language,ordinal",
        )?
        .query_map(params![project_id, version], |row| {
            Ok(GlossaryEntry {
                language: row.get(0)?,
                source: row.get(1)?,
                target: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Glossary {
        version: version.max(0) as u32,
        updated_at,
        entries,
    })
}

pub fn replace_language(
    db: &mut Connection,
    project_id: &str,
    language: &str,
    expected_version: u32,
    replacements: Vec<(String, String)>,
) -> Result<Glossary> {
    validate_language(language)?;
    if replacements.len() > 200 {
        bail!("glossary_invalid: 单个目标语言最多保存 200 个术语")
    }
    let mut normalized = BTreeMap::new();
    for (source, target) in replacements {
        let source = source.trim();
        let target = target.trim();
        if source.is_empty() || target.is_empty() {
            bail!("glossary_invalid: 术语原文和目标文本不能为空")
        }
        if source.chars().count() > 200 || target.chars().count() > 200 {
            bail!("glossary_invalid: 单个术语不得超过 200 个字符")
        }
        if normalized
            .insert(source.to_owned(), target.to_owned())
            .is_some()
        {
            bail!("glossary_invalid: 同一目标语言不能包含重复术语：{source}")
        }
    }

    let timestamp = now();
    let tx = db.transaction()?;
    let current: i64 = tx
        .query_row(
            "SELECT current_version FROM project_glossaries WHERE project_id=?1",
            [project_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("项目不存在或术语表尚未初始化：{project_id}"))?;
    if current != i64::from(expected_version) {
        bail!("glossary_version_conflict: 术语表版本已变化，当前版本为 {current}，请刷新后重试")
    }
    let next = current + 1;
    tx.execute(
        "INSERT INTO glossary_entries(project_id,version,language,source,target,ordinal)
         SELECT project_id,?3,language,source,target,ordinal FROM glossary_entries
         WHERE project_id=?1 AND version=?2 AND language<>?4",
        params![project_id, current, next, language],
    )?;
    for (ordinal, (source, target)) in normalized.into_iter().enumerate() {
        tx.execute(
            "INSERT INTO glossary_entries(project_id,version,language,source,target,ordinal)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![project_id, next, language, source, target, ordinal as i64],
        )?;
    }
    tx.execute(
        "UPDATE project_glossaries SET current_version=?2,updated_at=?3 WHERE project_id=?1",
        params![project_id, next, &timestamp],
    )?;
    tx.execute(
        "INSERT INTO glossary_versions(project_id,version,created_at) VALUES(?1,?2,?3)",
        params![project_id, next, &timestamp],
    )?;
    tx.execute(
        "UPDATE translation_segments SET status='stale',updated_at=?3
         WHERE project_id=?1 AND language=?2",
        params![project_id, language, &timestamp],
    )?;
    tx.execute(
        "UPDATE translations SET status='stale',updated_at=?3
         WHERE project_id=?1 AND language=?2",
        params![project_id, language, &timestamp],
    )?;
    tx.execute(
        "UPDATE projects SET updated_at=?2 WHERE id=?1",
        params![project_id, &timestamp],
    )?;
    tx.commit()?;
    load_glossary(db, project_id)
}

pub fn restore(
    db: &mut Connection,
    project_id: &str,
    restore_version: u32,
    expected_version: u32,
) -> Result<Glossary> {
    let timestamp = now();
    let tx = db.transaction()?;
    let current: i64 = tx
        .query_row(
            "SELECT current_version FROM project_glossaries WHERE project_id=?1",
            [project_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("项目不存在或术语表尚未初始化：{project_id}"))?;
    if current != i64::from(expected_version) {
        bail!("glossary_version_conflict: 术语表版本已变化，请刷新后重试")
    }
    let restore_version = i64::from(restore_version);
    if restore_version > current {
        bail!("glossary_version_not_found: 术语表历史版本不存在：{restore_version}")
    }
    let known = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM glossary_versions WHERE project_id=?1 AND version=?2)",
        params![project_id, restore_version],
        |row| row.get::<_, bool>(0),
    )?;
    if !known {
        bail!("glossary_version_not_found: 术语表历史版本不存在：{restore_version}")
    }
    let next = current + 1;
    tx.execute(
        "INSERT INTO glossary_entries(project_id,version,language,source,target,ordinal)
         SELECT project_id,?3,language,source,target,ordinal FROM glossary_entries
         WHERE project_id=?1 AND version=?2",
        params![project_id, restore_version, next],
    )?;
    tx.execute(
        "UPDATE project_glossaries SET current_version=?2,updated_at=?3 WHERE project_id=?1",
        params![project_id, next, &timestamp],
    )?;
    tx.execute(
        "INSERT INTO glossary_versions(project_id,version,created_at) VALUES(?1,?2,?3)",
        params![project_id, next, &timestamp],
    )?;
    tx.execute(
        "UPDATE translation_segments SET status='stale',updated_at=?2 WHERE project_id=?1",
        params![project_id, &timestamp],
    )?;
    tx.execute(
        "UPDATE translations SET status='stale',updated_at=?2 WHERE project_id=?1",
        params![project_id, &timestamp],
    )?;
    tx.execute(
        "UPDATE projects SET updated_at=?2 WHERE id=?1",
        params![project_id, &timestamp],
    )?;
    tx.commit()?;
    load_glossary(db, project_id)
}

pub fn effective_segment_status(
    source: &Segment,
    translated: &TranslationSegment,
    language: &str,
) -> String {
    if translated.source_hash != source_hash(&source.text) || translated.status == "stale" {
        return "stale".to_owned();
    }
    if translation_quality_failed(source, &translated.text, language) {
        return "quality_failed".to_owned();
    }
    "current".to_owned()
}

pub fn target_segment_ids(project: &Project, language: &str) -> Vec<String> {
    let translated = project.translations.get(language);
    project
        .transcript
        .segments
        .iter()
        .filter(|source| {
            translated
                .and_then(|translation| {
                    translation
                        .segments
                        .iter()
                        .find(|item| item.segment_id == source.id)
                })
                .is_none_or(|item| effective_segment_status(source, item, language) != "current")
        })
        .map(|segment| segment.id.clone())
        .collect()
}

pub fn task_segment_ids(db: &Connection, task_id: &str) -> Result<Vec<String>> {
    let selected = db
        .prepare("SELECT segment_id FROM task_segments WHERE task_id=?1 ORDER BY ordinal")?
        .query_map([task_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !selected.is_empty() {
        return Ok(selected);
    }
    let project_id: String = db.query_row(
        "SELECT project_id FROM tasks WHERE id=?1",
        [task_id],
        |row| row.get(0),
    )?;
    Ok(db
        .prepare("SELECT id FROM segments WHERE project_id=?1 ORDER BY start_seconds,id")?
        .query_map([project_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn invalidate_segments(
    tx: &Transaction<'_>,
    project_id: &str,
    segment_ids: &[&str],
) -> Result<usize> {
    if segment_ids.is_empty() {
        return Ok(0);
    }
    let timestamp = now();
    let mut changed = 0;
    for segment_id in segment_ids {
        changed += tx.execute(
            "UPDATE translation_segments SET status='stale',updated_at=?3
             WHERE project_id=?1 AND segment_id=?2 AND status!='stale'",
            params![project_id, segment_id, &timestamp],
        )?;
    }
    tx.execute(
        "UPDATE translations SET status='stale',updated_at=?2
         WHERE project_id=?1 AND EXISTS (
             SELECT 1 FROM translation_segments s
             WHERE s.project_id=translations.project_id
               AND s.language=translations.language AND s.status!='current'
         )",
        params![project_id, &timestamp],
    )?;
    Ok(changed)
}

pub fn refresh_language_status(
    tx: &Transaction<'_>,
    project_id: &str,
    language: &str,
    glossary_version: u32,
) -> Result<()> {
    let segment_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM segments WHERE project_id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    let current_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM translation_segments
         WHERE project_id=?1 AND language=?2 AND status='current'",
        params![project_id, language],
        |row| row.get(0),
    )?;
    let status = if segment_count > 0 && segment_count == current_count {
        "current"
    } else {
        "stale"
    };
    tx.execute(
        "UPDATE translations SET status=?3,updated_at=?4,glossary_version=?5
         WHERE project_id=?1 AND language=?2",
        params![
            project_id,
            language,
            status,
            now(),
            i64::from(glossary_version)
        ],
    )?;
    Ok(())
}

pub fn glossary_entries_for_language(glossary: &Glossary, language: &str) -> Vec<GlossaryEntry> {
    glossary
        .entries
        .iter()
        .filter(|entry| entry.language == language)
        .cloned()
        .collect()
}

fn validate_language(language: &str) -> Result<()> {
    if language.is_empty()
        || language.len() > 16
        || !language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        bail!("glossary_invalid: 目标语言代码无效")
    }
    Ok(())
}

fn translation_quality_failed(source: &Segment, text: &str, language: &str) -> bool {
    if text.trim().is_empty() || source.end <= source.start {
        return true;
    }
    let max_line = text
        .lines()
        .map(|line| {
            line.chars()
                .filter(|character| !character.is_whitespace())
                .count()
        })
        .max()
        .unwrap_or_default();
    let line_count = text.lines().count().max(1);
    let visible = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .count();
    let duration = source.end - source.start;
    max_line > 42
        || (language.to_ascii_lowercase().starts_with("en") && line_count > 2)
        || visible as f64 / duration > 20.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, project, tasks};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn hashes_and_quality_status_are_deterministic() {
        let source = Segment {
            id: "s1".into(),
            start: 0.0,
            end: 2.0,
            text: "你好".into(),
            confidence: None,
        };
        let current = TranslationSegment {
            segment_id: "s1".into(),
            text: "Hello".into(),
            source_hash: source_hash(&source.text),
            status: "current".into(),
            updated_at: "now".into(),
        };
        assert_eq!(effective_segment_status(&source, &current, "en"), "current");
        let mut stale = current.clone();
        stale.source_hash = source_hash("changed");
        assert_eq!(effective_segment_status(&source, &stale, "en"), "stale");
    }

    #[test]
    fn quality_failure_is_targeted_to_the_translated_segment() {
        let source = Segment {
            id: "s1".into(),
            start: 0.0,
            end: 1.0,
            text: "短句".into(),
            confidence: None,
        };
        let translated = TranslationSegment {
            segment_id: "s1".into(),
            text: "x".repeat(60),
            source_hash: source_hash(&source.text),
            status: "current".into(),
            updated_at: "now".into(),
        };
        assert_eq!(
            effective_segment_status(&source, &translated, "en"),
            "quality_failed"
        );
    }

    #[test]
    fn glossary_versions_are_recoverable_and_translate_tasks_select_only_stale_segments() {
        let temp = tempdir().unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let mut database = db::open_at(&temp.path().join("core.db")).unwrap();
        let created = project::create(&mut database, &media, None).unwrap();
        let first =
            project::add_segment(&mut database, &created.id, 0.0, 1.0, "第一段".into(), None)
                .unwrap();
        let second =
            project::add_segment(&mut database, &created.id, 1.2, 2.5, "第二段".into(), None)
                .unwrap();

        let glossary = replace_language(
            &mut database,
            &created.id,
            "en",
            0,
            vec![("本地优先".into(), "local-first".into())],
        )
        .unwrap();
        assert_eq!(glossary.version, 1);
        assert_eq!(glossary.entries[0].target, "local-first");

        let timestamp = now();
        database
            .execute(
                "INSERT INTO translations(project_id,language,status,updated_at,glossary_version) VALUES(?1,'en','stale',?2,1)",
                params![&created.id, &timestamp],
            )
            .unwrap();
        database
            .execute(
                "INSERT INTO translation_segments(project_id,language,segment_id,text,source_hash,status,updated_at) VALUES(?1,'en',?2,'First',?3,'current',?4)",
                params![&created.id, &first.id, source_hash("第一段"), &timestamp],
            )
            .unwrap();

        let task =
            tasks::create(&mut database, &created.id, "translate", Some("en".into())).unwrap();
        assert_eq!(
            task_segment_ids(&database, &task.id).unwrap(),
            vec![second.id]
        );
        let (_, _, payload) = tasks::claim(&mut database, "test-worker", Some(&task.id))
            .unwrap()
            .unwrap();
        assert_eq!(payload["segments"].as_array().unwrap().len(), 1);
        assert_eq!(payload["translationContext"]["glossaryVersion"], 1);
        assert_eq!(
            payload["translationContext"]["glossary"][0]["target"],
            "local-first"
        );
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(!serialized.contains(media.to_string_lossy().as_ref()));

        let restored = restore(&mut database, &created.id, 0, 1).unwrap();
        assert_eq!(restored.version, 2);
        assert!(restored.entries.is_empty());
        assert_eq!(
            project::load(&database, &created.id).unwrap().translations["en"].segments[0].status,
            "stale"
        );
    }
}
