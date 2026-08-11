const MAX_SEGMENTS: usize = 60;
const MAX_CHARACTERS: usize = 12_000;
const MAX_SINGLE_SEGMENT_CHARACTERS: usize = 24_000;

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension};

pub fn plan(db: &Connection, kind: &str, segment_ids: &[String]) -> Result<Vec<Vec<String>>> {
    let mut statement = db.prepare("SELECT length(text) FROM segments WHERE id=?1")?;
    let mut character_counts = Vec::with_capacity(segment_ids.len());
    for segment_id in segment_ids {
        let count = statement
            .query_row([segment_id], |row| row.get::<_, i64>(0))
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("agent_batch_incomplete: 字幕段不存在：{segment_id}"))?;
        character_counts.push(count.max(0) as usize);
    }
    split(
        segment_ids,
        &character_counts,
        matches!(kind, "summary" | "speaker_names"),
    )
}

pub fn split(
    segment_ids: &[String],
    character_counts: &[usize],
    atomic: bool,
) -> Result<Vec<Vec<String>>> {
    if segment_ids.len() != character_counts.len() {
        bail!("agent_batch_incomplete: Agent 批次字符统计不完整")
    }
    if character_counts
        .iter()
        .any(|count| *count > MAX_SINGLE_SEGMENT_CHARACTERS)
    {
        bail!("payload_too_large: 单条字幕超过 AI 任务限制")
    }
    if atomic {
        return Ok(vec![segment_ids.to_vec()]);
    }
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut characters = 0;
    for (id, count) in segment_ids.iter().zip(character_counts) {
        if !current.is_empty()
            && (current.len() >= MAX_SEGMENTS || characters + count > MAX_CHARACTERS)
        {
            batches.push(std::mem::take(&mut current));
            characters = 0;
        }
        current.push(id.clone());
        characters += count;
    }
    if !current.is_empty() {
        batches.push(current);
    }
    Ok(batches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_segment_and_character_limits() {
        let ids = (0..61)
            .map(|index| format!("s-{index}"))
            .collect::<Vec<_>>();
        assert_eq!(
            split(&ids, &vec![10; 61], false)
                .unwrap()
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![60, 1]
        );
        let ids = vec!["a".into(), "b".into(), "c".into()];
        assert_eq!(split(&ids, &[8_000, 5_000, 1], false).unwrap().len(), 2);
    }
}
