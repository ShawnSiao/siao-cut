use crate::{
    model::{AgentPatchItem, AgentPatchSet, Project},
    project,
    util::{new_id, now},
};
use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use std::collections::BTreeSet;

struct ProposedItem {
    segment_id: Option<String>,
    target: String,
    before_text: String,
    after_text: String,
    current_text: String,
    reason: String,
    confidence: Option<f64>,
    status: String,
}

type ReviewItemRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    Option<String>,
);

pub fn stage(
    db: &mut Connection,
    task_id: &str,
    project_id: &str,
    kind: &str,
    language: Option<&str>,
    base_version_id: &str,
    response: &Value,
) -> Result<AgentPatchSet> {
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM agent_patch_sets WHERE task_id=?1)",
        [task_id],
        |row| row.get(0),
    )?;
    if exists {
        bail!("task_patch_already_submitted: 任务已提交过补丁")
    }
    let base_project = load_version_project(db, project_id, base_version_id)?;
    let items = proposed_items(db, project_id, kind, language, &base_project, response)?;
    if items.is_empty() {
        bail!("Agent 响应没有可审阅的修改")
    }
    let patch_set_id = new_id("patch");
    let created_at = now();
    let tx = db.transaction()?;
    tx.execute(
        "INSERT INTO agent_patch_sets(id,task_id,project_id,kind,language,status,base_version_id,created_at) VALUES(?1,?2,?3,?4,?5,'pending_review',?6,?7)",
        params![&patch_set_id, task_id, project_id, kind, language, base_version_id, &created_at],
    )?;
    for (ordinal, item) in items.iter().enumerate() {
        tx.execute(
            "INSERT INTO agent_patch_items(id,patch_set_id,segment_id,target,before_text,after_text,current_text_at_submit,reason,confidence,status,ordinal) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![new_id("pi"), &patch_set_id, &item.segment_id, &item.target, &item.before_text, &item.after_text, &item.current_text, &item.reason, item.confidence, &item.status, ordinal as i64],
        )?;
    }
    tx.execute(
        "UPDATE tasks SET status='review',progress=1,lease_worker=NULL,lease_id=NULL,lease_expires_at=NULL WHERE id=?1",
        [task_id],
    )?;
    tx.execute(
        "UPDATE workflows SET status='needs_review',updated_at=?2 WHERE task_id=?1",
        params![task_id, &created_at],
    )?;
    tx.commit()?;
    load_by_task(db, task_id)
}

fn load_version_project(db: &Connection, project_id: &str, version_id: &str) -> Result<Project> {
    let raw: String = db
        .query_row(
            "SELECT snapshot_json FROM versions WHERE id=?1 AND project_id=?2",
            params![version_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务基线版本不存在：{version_id}"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn proposed_items(
    db: &Connection,
    project_id: &str,
    kind: &str,
    language: Option<&str>,
    base_project: &Project,
    response: &Value,
) -> Result<Vec<ProposedItem>> {
    if kind == "summary" {
        let after = response
            .get("summary")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("摘要任务需要非空 summary"))?;
        let before = db
            .query_row(
                "SELECT text FROM summaries WHERE project_id=?1",
                [project_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_default();
        return Ok(vec![ProposedItem {
            segment_id: None,
            target: "summary".into(),
            before_text: before.clone(),
            after_text: after.trim().to_owned(),
            current_text: before,
            reason: response
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("Agent 摘要建议")
                .to_owned(),
            confidence: response.get("confidence").and_then(Value::as_f64),
            status: "pending".into(),
        }]);
    }

    let raw_items = response
        .get("patches")
        .or_else(|| response.get("segments"))
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| anyhow!("任务响应需要非空 patches 数组"))?;
    let target = match kind {
        "translate" => "translation",
        "cut" => "cut",
        _ => "transcript",
    };
    if target == "translation" && language.is_none() {
        bail!("翻译任务缺少目标语言")
    }
    let uses_patch_shape = response.get("patches").is_some();
    let mut seen = BTreeSet::new();
    let mut proposed = Vec::with_capacity(raw_items.len());
    for item in raw_items {
        let segment_id = item
            .get("segmentId")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("补丁缺少 segmentId"))?;
        if !seen.insert(segment_id.to_owned()) {
            bail!("同一字幕段不能在一个任务中重复提交")
        }
        let base_segment = base_project
            .transcript
            .segments
            .iter()
            .find(|segment| segment.id == segment_id)
            .ok_or_else(|| anyhow!("任务基线中不存在字幕段：{segment_id}"))?;
        let before = if uses_patch_shape {
            item.get("before")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("补丁缺少 before"))?
        } else {
            &base_segment.text
        };
        if before != base_segment.text {
            bail!("patch_before_mismatch: 补丁原文与任务基线不一致")
        }
        let after = item
            .get("after")
            .or_else(|| item.get("text"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("补丁缺少 after"))?;
        if after.trim().is_empty() && target != "cut" {
            bail!("补丁建议文本不能为空")
        }
        let reason = item
            .get("reason")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Agent 建议")
            .to_owned();
        let confidence = item.get("confidence").and_then(Value::as_f64);
        if confidence.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
            bail!("补丁置信度必须在 0 到 1 之间")
        }
        let current_source: String = db
            .query_row(
                "SELECT text FROM segments WHERE id=?1 AND project_id=?2",
                params![segment_id, project_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| anyhow!("当前项目中不存在字幕段：{segment_id}"))?;
        let current_text = if target == "translation" {
            db.query_row(
                "SELECT text FROM translation_segments WHERE project_id=?1 AND language=?2 AND segment_id=?3",
                params![project_id, language, segment_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_default()
        } else {
            current_source.clone()
        };
        proposed.push(ProposedItem {
            segment_id: Some(segment_id.to_owned()),
            target: target.into(),
            before_text: before.to_owned(),
            after_text: after.trim().to_owned(),
            current_text,
            reason,
            confidence,
            status: if current_source == base_segment.text {
                "pending".into()
            } else {
                "conflict".into()
            },
        });
    }
    Ok(proposed)
}

pub fn load_by_task(db: &Connection, task_id: &str) -> Result<AgentPatchSet> {
    let set_id: String = db
        .query_row(
            "SELECT id FROM agent_patch_sets WHERE task_id=?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("任务尚无待审补丁：{task_id}"))?;
    load_set(db, &set_id)
}

pub fn for_project(db: &Connection, project_id: &str) -> Result<Vec<AgentPatchSet>> {
    let ids = db
        .prepare("SELECT id FROM agent_patch_sets WHERE project_id=?1 ORDER BY created_at DESC")?
        .query_map([project_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.into_iter().map(|id| load_set(db, &id)).collect()
}

fn load_set(db: &Connection, set_id: &str) -> Result<AgentPatchSet> {
    let (task_id, project_id, kind, language, status, base_version_id, created_at): (
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
    ) = db.query_row(
        "SELECT task_id,project_id,kind,language,status,base_version_id,created_at FROM agent_patch_sets WHERE id=?1",
        [set_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
    )?;
    let mut items = db
        .prepare("SELECT id,segment_id,target,before_text,after_text,current_text_at_submit,reason,confidence,status FROM agent_patch_items WHERE patch_set_id=?1 ORDER BY ordinal")?
        .query_map([set_id], |row| {
            Ok(AgentPatchItem {
                id: row.get(0)?,
                segment_id: row.get(1)?,
                target: row.get(2)?,
                before_text: row.get(3)?,
                after_text: row.get(4)?,
                current_text: row.get(5)?,
                reason: row.get(6)?,
                confidence: row.get(7)?,
                status: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for item in &mut items {
        let current = match item.target.as_str() {
            "transcript" | "cut" => db
                .query_row(
                    "SELECT text FROM segments WHERE id=?1 AND project_id=?2",
                    params![&item.segment_id, &project_id],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or_default(),
            "translation" => db
                .query_row(
                    "SELECT text FROM translation_segments WHERE project_id=?1 AND language=?2 AND segment_id=?3",
                    params![&project_id, &language, &item.segment_id],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or_default(),
            "summary" => db
                .query_row(
                    "SELECT text FROM summaries WHERE project_id=?1",
                    [&project_id],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or_default(),
            _ => item.current_text.clone(),
        };
        if item.status == "pending" && current != item.current_text {
            item.status = "conflict".into();
        }
        item.current_text = current;
    }
    Ok(AgentPatchSet {
        id: set_id.to_owned(),
        task_id,
        kind,
        language,
        status,
        base_version_id,
        created_at,
        items,
    })
}

pub fn review_item(
    db: &mut Connection,
    patch_item_id: &str,
    action: &str,
) -> Result<(String, AgentPatchSet)> {
    if !["apply", "keep"].contains(&action) {
        bail!("审阅动作必须为 apply 或 keep")
    }
    let row: Option<ReviewItemRow> = db
        .query_row(
            "SELECT ps.id,ps.project_id,ps.language,pi.target,pi.after_text,pi.reason,pi.status,pi.segment_id FROM agent_patch_items pi JOIN agent_patch_sets ps ON ps.id=pi.patch_set_id WHERE pi.id=?1",
            [patch_item_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
        )
        .optional()?;
    let Some((set_id, project_id, language, target, after, reason, status, segment_id)) = row
    else {
        bail!("补丁不存在：{patch_item_id}")
    };
    if !["pending", "conflict"].contains(&status.as_str()) {
        bail!("补丁已经审阅：{patch_item_id}")
    }
    let mut changed_project = false;
    if action == "apply" {
        match target.as_str() {
            "transcript" => {
                let segment_id = segment_id
                    .as_deref()
                    .ok_or_else(|| anyhow!("补丁缺少字幕段"))?;
                db.execute(
                    "UPDATE segments SET text=?2 WHERE id=?1 AND project_id=?3",
                    params![segment_id, &after, &project_id],
                )?;
                db.execute(
                    "UPDATE translations SET status='stale' WHERE project_id=?1",
                    [&project_id],
                )?;
            }
            "translation" => {
                let segment_id = segment_id
                    .as_deref()
                    .ok_or_else(|| anyhow!("补丁缺少字幕段"))?;
                let language = language
                    .as_deref()
                    .ok_or_else(|| anyhow!("补丁缺少目标语言"))?;
                db.execute("INSERT INTO translations(project_id,language,status,updated_at) VALUES(?1,?2,'current',?3) ON CONFLICT(project_id,language) DO UPDATE SET status='current',updated_at=excluded.updated_at",params![&project_id,language,now()])?;
                db.execute("INSERT INTO translation_segments(project_id,language,segment_id,text) VALUES(?1,?2,?3,?4) ON CONFLICT(project_id,language,segment_id) DO UPDATE SET text=excluded.text",params![&project_id,language,segment_id,&after])?;
            }
            "summary" => {
                db.execute("INSERT INTO summaries(project_id,text,updated_at) VALUES(?1,?2,?3) ON CONFLICT(project_id) DO UPDATE SET text=excluded.text,updated_at=excluded.updated_at",params![&project_id,&after,now()])?;
            }
            "cut" => {
                let segment_id = segment_id
                    .as_deref()
                    .ok_or_else(|| anyhow!("补丁缺少字幕段"))?;
                let (start, end): (f64, f64) = db.query_row(
                    "SELECT start_seconds,end_seconds FROM segments WHERE id=?1 AND project_id=?2",
                    params![segment_id, &project_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                db.execute("INSERT INTO edits(id,project_id,kind,status,segment_id,start_seconds,end_seconds,reason,created_at) VALUES(?1,?2,'semantic_cut','applied',?3,?4,?5,?6,?7)",params![new_id("e"),&project_id,segment_id,start,end,&reason,now()])?;
            }
            _ => bail!("未知补丁目标：{target}"),
        }
        changed_project = true;
    }
    db.execute(
        "UPDATE agent_patch_items SET status=?2 WHERE id=?1",
        params![
            patch_item_id,
            if action == "apply" { "applied" } else { "kept" }
        ],
    )?;
    if changed_project {
        project::snapshot(db, &project_id, &format!("应用 Agent 建议：{reason}"))?;
    }
    finalize_if_resolved(db, &set_id)?;
    Ok((project_id, load_set(db, &set_id)?))
}

pub fn review_all(
    db: &mut Connection,
    task_id: &str,
    action: &str,
) -> Result<(String, AgentPatchSet)> {
    let set = load_by_task(db, task_id)?;
    let ids = set
        .items
        .iter()
        .filter(|item| ["pending", "conflict"].contains(&item.status.as_str()))
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        bail!("任务没有待审补丁")
    }
    let mut project_id = String::new();
    for id in ids {
        project_id = review_item(db, &id, action)?.0;
    }
    Ok((project_id, load_by_task(db, task_id)?))
}

fn finalize_if_resolved(db: &Connection, set_id: &str) -> Result<()> {
    let (task_id, project_id): (String, String) = db.query_row(
        "SELECT task_id,project_id FROM agent_patch_sets WHERE id=?1",
        [set_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let unresolved: i64 = db.query_row(
        "SELECT COUNT(*) FROM agent_patch_items WHERE patch_set_id=?1 AND status IN ('pending','conflict')",
        [set_id],
        |row| row.get(0),
    )?;
    let applied: i64 = db.query_row(
        "SELECT COUNT(*) FROM agent_patch_items WHERE patch_set_id=?1 AND status='applied'",
        [set_id],
        |row| row.get(0),
    )?;
    let set_status = if unresolved > 0 {
        if applied > 0 {
            "partially_applied"
        } else {
            "pending_review"
        }
    } else if applied > 0 {
        "applied"
    } else {
        "kept"
    };
    db.execute(
        "UPDATE agent_patch_sets SET status=?2 WHERE id=?1",
        params![set_id, set_status],
    )?;
    if unresolved == 0 {
        let completed_at = now();
        db.execute(
            "UPDATE tasks SET status='done',completed_at=?2 WHERE id=?1",
            params![&task_id, &completed_at],
        )?;
        db.execute(
            "UPDATE workflows SET status='completed',updated_at=?2 WHERE task_id=?1",
            params![&task_id, &completed_at],
        )?;
        db.execute(
            "INSERT INTO task_events(task_id,project_id,kind,progress,message,created_at) VALUES(?1,?2,'completed',1,'补丁审阅完成',?3)",
            params![&task_id, &project_id, &completed_at],
        )?;
    }
    Ok(())
}
