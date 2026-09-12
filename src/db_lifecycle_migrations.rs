//! Additive command journals; called by the database migration runner in its upgrade transaction.
use anyhow::Result;
use rusqlite::Transaction;

pub(super) fn migration_37_transcription_commands(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE transcription_commands(mutation_id TEXT PRIMARY KEY, request_json TEXT NOT NULL, job_id TEXT NOT NULL REFERENCES transcription_jobs(id) ON DELETE CASCADE, created_at TEXT NOT NULL);")?;
    Ok(())
}

pub(super) fn migration_38_project_commands(tx: &Transaction<'_>) -> Result<()> {
    // No project FK: deletion receipts must survive deletion of their target.
    tx.execute_batch(
        "CREATE TABLE project_commands (
        mutation_id TEXT PRIMARY KEY,
        request_json TEXT NOT NULL,
        response_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    )",
    )?;
    Ok(())
}
