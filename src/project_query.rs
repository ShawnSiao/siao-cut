//! Desktop read models. Listing projects never deserializes project snapshots.
use anyhow::{Result, bail};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub segment_count: u32,
    pub duration_seconds: Option<f64>,
    pub version_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPage {
    pub items: Vec<ProjectSummary>,
    pub next_offset: Option<u32>,
    pub total: u32,
}

#[derive(Debug, Deserialize, TS)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProjectQuery {
    List { offset: u32, limit: Option<u32> },
    Show { project_id: String },
    Review { project_id: String },
    History { project_id: String },
    Insights { project_id: String },
}

pub fn list(db: &Connection, offset: u32, limit: Option<u32>) -> Result<ProjectPage> {
    let limit = limit.unwrap_or(50);
    if !(1..=50).contains(&limit) {
        bail!("invalid_request: 项目列表每页必须为 1 到 50 条")
    }
    let items = db
        .prepare(
            "SELECT p.id,p.title,p.created_at,p.updated_at,
            (SELECT COUNT(*) FROM segments s WHERE s.project_id=p.id),m.duration_seconds,
            (SELECT v.id FROM versions v JOIN project_history h ON h.project_id=v.project_id
             WHERE v.project_id=p.id AND v.active_history=1 AND v.history_index=h.cursor_index)
         FROM projects p LEFT JOIN media m ON m.project_id=p.id
         ORDER BY p.updated_at DESC,p.id LIMIT ?1 OFFSET ?2",
        )?
        .query_map(params![limit, offset], |row| {
            Ok(ProjectSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                segment_count: row.get(4)?,
                duration_seconds: row.get(5)?,
                version_id: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let total: u32 = db.query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))?;
    let next = offset.saturating_add(items.len() as u32);
    Ok(ProjectPage {
        items,
        next_offset: (next < total).then_some(next),
        total,
    })
}

pub fn execute(db: &Connection, query: ProjectQuery) -> Result<Value> {
    let tx = db.unchecked_transaction()?;
    let db = &*tx;
    let result = match query {
        ProjectQuery::List { offset, limit } => json!({"projectPage":list(db,offset,limit)?}),
        ProjectQuery::Show { project_id } => {
            json!({"project":crate::project::load_workspace(db,&project_id)?})
        }
        ProjectQuery::Review { project_id } => {
            json!({"projectId":project_id,"versionId":crate::project::current_version_id(db,&project_id)?,"tasks":crate::project::select_tasks(db,&project_id)?,"patchSets":crate::patches::for_project(db,&project_id)?,"projectWorkflows":crate::workflows::for_project(db,&project_id)?})
        }
        ProjectQuery::History { project_id } => {
            let versions = db.prepare("SELECT id,reason,created_at FROM versions WHERE project_id=?1 AND active_history=1 ORDER BY history_index")?
                .query_map([&project_id], |row| Ok(crate::model::Version { id:row.get(0)?,reason:row.get(1)?,created_at:row.get(2)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            json!({"projectId":project_id,"history":crate::project::history_status(db,&project_id)?,"versions":versions})
        }
        ProjectQuery::Insights { project_id } => {
            let segments = crate::project::select_segments(db, &project_id)?;
            let words = crate::project::select_words(db, &project_id)?;
            let (language,duration): (String,Option<f64>) = db.query_row("SELECT p.source_language,m.duration_seconds FROM projects p JOIN media m ON m.project_id=p.id WHERE p.id=?1",[&project_id],|row| Ok((row.get(0)?,row.get(1)?)))?;
            let transcript = crate::model::Transcript {
                source_language: language,
                segments,
                words,
            };
            json!({"projectId":project_id,"versionId":crate::project::current_version_id(db,&project_id)?,
                "subtitleQuality":crate::subtitle_quality::inspect_with_language(&transcript.segments,duration,&transcript.source_language),
                "speechInsights":crate::speech::analyze(&transcript)})
        }
    };
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
#[path = "project_query_tests.rs"]
mod tests;
