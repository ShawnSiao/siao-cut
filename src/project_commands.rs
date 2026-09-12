//! Versioned project lifecycle commands. Receipts survive project deletion.
use crate::{
    project,
    util::{new_id, now},
    write_transaction::WriteTransaction,
};
use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Deserialize, Serialize, ts_rs::TS)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProjectCommand {
    Import {
        mutation_id: String,
        path: String,
    },
    Delete {
        mutation_id: String,
        project_id: String,
        expected_version_id: String,
    },
}
pub fn execute(db: &mut Connection, request: ProjectCommand) -> Result<Value> {
    let mutation_id = match &request {
        ProjectCommand::Import { mutation_id, .. } | ProjectCommand::Delete { mutation_id, .. } => {
            mutation_id
        }
    };
    if mutation_id.is_empty() || mutation_id.len() > 256 || mutation_id.contains('\0') {
        bail!("invalid_request: 项目操作标识无效")
    }
    let raw = serde_json::to_string(&request)?;
    if let Some(receipt) = receipt(db, mutation_id, &raw)? {
        return Ok(receipt);
    }
    // Hash/probe media before acquiring the write lock, then revalidate its identity in the transaction.
    let prepared = match &request {
        ProjectCommand::Import { path, .. } => {
            Some(project::prepare_media(std::path::Path::new(path), None)?)
        }
        _ => None,
    };
    let mut tx = WriteTransaction::begin(db)?;
    if let Some(receipt) = receipt(&tx, mutation_id, &raw)? {
        return Ok(receipt);
    }
    let result = match &request {
        ProjectCommand::Import { .. } => {
            let id = new_id("p");
            project::insert_prepared_with_id_in_transaction(&tx, prepared.as_ref().unwrap(), &id)?;
            json!({"projectId": id, "project": project::load_workspace(&tx, &id)?, "message": "已创建项目。"})
        }
        ProjectCommand::Delete {
            project_id,
            expected_version_id,
            ..
        } => {
            project::delete_at_version(&mut tx, project_id, expected_version_id)?;
            json!({"projectId": project_id, "deleted": true, "message": "项目已删除。"})
        }
    };
    // Import receipts retain only the ID, never a second copy of media paths or project content.
    let saved = match &request {
        ProjectCommand::Import { .. } => {
            json!({"projectId": result["projectId"], "imported": true})
        }
        ProjectCommand::Delete { .. } => result.clone(),
    };
    tx.execute(
        "INSERT INTO project_commands VALUES(?1,?2,?3,?4)",
        params![mutation_id, raw, serde_json::to_string(&saved)?, now()],
    )?;
    tx.commit()?;
    Ok(result)
}
fn receipt(db: &Connection, id: &str, raw: &str) -> Result<Option<Value>> {
    let previous: Option<(String, String)> = db
        .query_row(
            "SELECT request_json,response_json FROM project_commands WHERE mutation_id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((old, response)) = previous else {
        return Ok(None);
    };
    if old != raw {
        bail!("editing_mutation_reused: 项目操作标识已经用于其他请求")
    }
    let mut response: Value = serde_json::from_str(&response)?;
    if response["imported"] == true {
        let id = response["projectId"].as_str().unwrap();
        // A replay cannot recreate a project that was subsequently deleted.
        let project = project::load_workspace(db, id)?;
        response["project"] = serde_json::to_value(project)?;
    }
    Ok(Some(response))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_and_delete_replay_without_duplicate_project_or_missing_target_error() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("音频.wav");
        std::fs::write(&path, b"audio").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("test.db")).unwrap();
        let import = || ProjectCommand::Import {
            mutation_id: "import-once".into(),
            path: path.to_string_lossy().into(),
        };
        let first = execute(&mut db, import()).unwrap();
        let second = execute(&mut db, import()).unwrap();
        assert_eq!(first["projectId"], second["projectId"]);
        let id = first["projectId"].as_str().unwrap();
        let version = project::current_version_id(&db, id).unwrap().unwrap();
        assert!(
            execute(
                &mut db,
                ProjectCommand::Delete {
                    mutation_id: "delete-stale".into(),
                    project_id: id.into(),
                    expected_version_id: "old".into()
                }
            )
            .is_err()
        );
        let delete = || ProjectCommand::Delete {
            mutation_id: "delete-once".into(),
            project_id: id.into(),
            expected_version_id: version.clone(),
        };
        let first_delete = execute(&mut db, delete()).unwrap();
        assert_eq!(first_delete, execute(&mut db, delete()).unwrap());
        assert!(execute(&mut db, import()).is_err());
        let count: i64 = db
            .query_row("SELECT count(*) FROM projects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(
            execute(
                &mut db,
                ProjectCommand::Delete {
                    mutation_id: "delete-once".into(),
                    project_id: "other".into(),
                    expected_version_id: version
                }
            )
            .unwrap_err()
            .to_string()
            .contains("editing_mutation_reused")
        );
    }
}
