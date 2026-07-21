use crate::{
    model::Workflow,
    tasks,
    util::{new_id, now},
};
use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};

#[cfg(test)]
pub fn create(
    db: &mut Connection,
    project_id: &str,
    kind: &str,
    language: Option<String>,
) -> Result<Workflow> {
    create_with_locale(db, project_id, kind, language, "zh-CN")
}

pub fn create_with_locale(
    db: &mut Connection,
    project_id: &str,
    kind: &str,
    language: Option<String>,
    instruction_locale: &str,
) -> Result<Workflow> {
    if ![
        "polish",
        "translate",
        "proofread",
        "edit",
        "cut",
        "summary",
        "punctuate",
        "speaker_names",
    ]
    .contains(&kind)
    {
        bail!("工作流类型不受支持")
    }
    if kind == "translate" && language.is_none() {
        bail!("翻译工作流需要 --lang")
    }
    let workflow_id = new_id("wf");
    let task = tasks::create_for_workflow(
        db,
        project_id,
        kind,
        language.clone(),
        Some(&workflow_id),
        instruction_locale,
    )?;
    let created_at = now();
    db.execute(
        "INSERT INTO workflows(id,project_id,kind,language,status,task_id,created_at,updated_at,instruction_locale) VALUES(?1,?2,?3,?4,'waiting_agent',?5,?6,?6,?7)",
        params![&workflow_id, project_id, kind, &language, &task.id, &created_at, instruction_locale],
    )?;
    load(db, &workflow_id)
}

pub fn load(db: &Connection, workflow_id: &str) -> Result<Workflow> {
    db.query_row(
        "SELECT id,kind,language,status,task_id,created_at,updated_at,instruction_locale FROM workflows WHERE id=?1",
        [workflow_id],
        |row| {
            Ok(Workflow {
                id: row.get(0)?,
                kind: row.get(1)?,
                language: row.get(2)?,
                status: row.get(3)?,
                task_id: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                instruction_locale: row.get(7)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("工作流不存在：{workflow_id}"))
}

pub fn for_project(db: &Connection, project_id: &str) -> Result<Vec<Workflow>> {
    db.prepare("SELECT id FROM workflows WHERE project_id=?1 ORDER BY created_at DESC")?
        .query_map([project_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|id| load(db, &id))
        .collect::<Result<Vec<_>>>()
}

pub fn continue_workflow(db: &mut Connection, workflow_id: &str) -> Result<Workflow> {
    let workflow = load(db, workflow_id)?;
    let task_status: String = db.query_row(
        "SELECT status FROM tasks WHERE id=?1",
        [&workflow.task_id],
        |row| row.get(0),
    )?;
    match task_status.as_str() {
        "failed" | "interrupted" => {
            tasks::retry(db, &workflow.task_id)?;
            db.execute(
                "UPDATE workflows SET status='waiting_agent',updated_at=?2 WHERE id=?1",
                params![workflow_id, now()],
            )?;
        }
        "queued" => {}
        "claimed" | "running" => {
            db.execute(
                "UPDATE workflows SET status='running',updated_at=?2 WHERE id=?1",
                params![workflow_id, now()],
            )?;
        }
        "review" => {
            db.execute(
                "UPDATE workflows SET status='needs_review',updated_at=?2 WHERE id=?1",
                params![workflow_id, now()],
            )?;
        }
        "done" => {
            db.execute(
                "UPDATE workflows SET status='completed',updated_at=?2 WHERE id=?1",
                params![workflow_id, now()],
            )?;
        }
        "cancelled" => {
            db.execute(
                "UPDATE workflows SET status='cancelled',updated_at=?2 WHERE id=?1",
                params![workflow_id, now()],
            )?;
        }
        _ => bail!("工作流包含未知任务状态：{task_status}"),
    }
    load(db, workflow_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, patches, project, tasks};
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn workflow_moves_from_agent_to_review_to_complete() {
        let temp = tempdir().unwrap();
        let mut db = db::open_at(&temp.path().join("workflow.db")).unwrap();
        let media = temp.path().join("talk.wav");
        fs::write(&media, b"audio").unwrap();
        let project = project::create(&mut db, &media, None).unwrap();
        let segment =
            project::add_segment(&mut db, &project.id, 0.0, 1.0, "你好".into(), None).unwrap();
        let workflow = create(&mut db, &project.id, "polish", None).unwrap();
        let claim = tasks::claim(&mut db, "workflow-agent", None)
            .unwrap()
            .unwrap();
        let base = claim.2["baseVersionId"].as_str().unwrap();
        tasks::submit(
            &mut db,
            &workflow.task_id,
            "workflow-agent",
            json!({"baseVersionId":base,"patches":[{"segmentId":segment.id,"before":"你好","after":"你好。","reason":"补充句号","confidence":0.99}]}),
        )
        .unwrap();
        assert_eq!(
            continue_workflow(&mut db, &workflow.id).unwrap().status,
            "needs_review"
        );
        patches::review_all(&mut db, &workflow.task_id, "apply").unwrap();
        assert_eq!(
            continue_workflow(&mut db, &workflow.id).unwrap().status,
            "completed"
        );
    }
}
