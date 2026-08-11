use anyhow::{Result, anyhow};
use rusqlite::{Connection, OptionalExtension};

use crate::model::{AgentRun, AgentRunBatch};

pub fn load(db: &Connection, run_id: &str) -> Result<AgentRun> {
    let mut run = db
        .query_row(
            "SELECT id,task_id,project_id,provider,execution_kind,service_config_id,
                    service_revision,network_revision,provider_id,model_id,
                    provider_request_id,usage_json,retry_count,status,base_version_id,
                    progress,current_batch,batch_count,timeout_seconds,cli_version,
                    auth_mode,codex_thread_id,cancel_requested_at,error_code,error_message,
                    created_at,updated_at,started_at,completed_at,worker_pid,attempt_count
             FROM agent_runs WHERE id=?1",
            [run_id],
            |row| {
                Ok(AgentRun {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    project_id: row.get(2)?,
                    provider: row.get(3)?,
                    execution_kind: row.get(4)?,
                    service_config_id: row.get(5)?,
                    service_revision: optional_u64(row, 6)?,
                    network_revision: optional_u64(row, 7)?,
                    provider_id: row.get(8)?,
                    model_id: row.get(9)?,
                    provider_request_id: row.get(10)?,
                    usage: optional_json(row, 11)?,
                    retry_count: row.get::<_, i64>(12)? as u32,
                    status: row.get(13)?,
                    base_version_id: row.get(14)?,
                    progress: row.get(15)?,
                    current_batch: row.get::<_, i64>(16)? as u32,
                    batch_count: row.get::<_, i64>(17)? as u32,
                    timeout_seconds: row.get::<_, i64>(18)? as u64,
                    cli_version: row.get(19)?,
                    auth_mode: row.get(20)?,
                    codex_thread_id: row.get(21)?,
                    cancel_requested_at: row.get(22)?,
                    error_code: row.get(23)?,
                    error_message: row.get(24)?,
                    created_at: row.get(25)?,
                    updated_at: row.get(26)?,
                    started_at: row.get(27)?,
                    completed_at: row.get(28)?,
                    worker_pid: row.get(29)?,
                    attempt_count: row.get::<_, i64>(30)? as u32,
                    batches: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| anyhow!("agent_run_not_found: Agent 运行记录不存在：{run_id}"))?;
    run.batches = load_batches(db, run_id)?;
    Ok(run)
}

pub fn list(db: &Connection, project_id: Option<&str>) -> Result<Vec<AgentRun>> {
    let ids = db
        .prepare("SELECT id FROM agent_runs WHERE (?1 IS NULL OR project_id=?1) ORDER BY created_at DESC")?
        .query_map([project_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.into_iter().map(|id| load(db, &id)).collect()
}

fn load_batches(db: &Connection, run_id: &str) -> Result<Vec<AgentRunBatch>> {
    Ok(db
        .prepare(
            "SELECT id,ordinal,status,segment_ids_json,codex_thread_id,
                provider_request_id,usage_json,retry_count,error_code,error_message,
                started_at,completed_at,attempt_count
         FROM agent_run_batches WHERE run_id=?1 ORDER BY ordinal",
        )?
        .query_map([run_id], |row| {
            let raw_ids: String = row.get(3)?;
            Ok(AgentRunBatch {
                id: row.get(0)?,
                ordinal: row.get::<_, i64>(1)? as u32,
                status: row.get(2)?,
                segment_ids: serde_json::from_str(&raw_ids).unwrap_or_default(),
                codex_thread_id: row.get(4)?,
                provider_request_id: row.get(5)?,
                usage: optional_json(row, 6)?,
                retry_count: row.get::<_, i64>(7)? as u32,
                error_code: row.get(8)?,
                error_message: row.get(9)?,
                started_at: row.get(10)?,
                completed_at: row.get(11)?,
                attempt_count: row.get::<_, i64>(12)? as u32,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn optional_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    Ok(row.get::<_, Option<i64>>(index)?.map(|value| value as u64))
}

fn optional_json(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<serde_json::Value>> {
    Ok(row
        .get::<_, Option<String>>(index)?
        .and_then(|value| serde_json::from_str(&value).ok()))
}
