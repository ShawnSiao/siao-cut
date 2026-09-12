//! Frozen-version desktop exports. Registration and command receipts commit before worker launch.
use crate::{
    export,
    model::{SubtitleDelivery, SubtitleMode},
    project, video_export,
    write_transaction::WriteTransaction,
};
use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Debug, Deserialize, Serialize, ts_rs::TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ExportOperation {
    Transcript {
        format: String,
        output: PathBuf,
        language: Option<String>,
        subtitle_mode: SubtitleMode,
        allow_stale_translation: bool,
    },
    Structured {
        format: String,
        output: PathBuf,
        include_speaker_labels: bool,
        confirm_warnings: bool,
    },
    Video {
        output: PathBuf,
        language: Option<String>,
        subtitle_mode: SubtitleMode,
        subtitle_delivery: SubtitleDelivery,
        allow_stale_translation: bool,
    },
}
#[derive(Debug, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportCommand {
    pub mutation_id: String,
    pub project_id: String,
    pub expected_version_id: String,
    pub operation: ExportOperation,
}
pub fn execute(db: &mut Connection, request: ExportCommand) -> Result<Value> {
    if request.mutation_id.is_empty() || request.mutation_id.len() > 256 {
        bail!("invalid_request: 导出操作标识无效")
    }
    let raw = serde_json::to_string(&request)?;
    let mut tx = WriteTransaction::begin(db)?;
    let previous: Option<(String, String)> = tx
        .query_row(
            "SELECT request_json,response_json FROM project_commands WHERE mutation_id=?1",
            [&request.mutation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((old, response)) = previous {
        if old != raw {
            bail!("editing_mutation_reused: 导出标识已用于其他请求")
        }
        let mut response: Value = serde_json::from_str(&response)?;
        if let Some(job_id) = response["jobId"].as_str() {
            response["job"] = serde_json::to_value(video_export::load(&tx, job_id)?)?;
        }
        return Ok(response);
    }
    if project::current_version_id(&tx, &request.project_id)?.as_deref()
        != Some(&request.expected_version_id)
    {
        bail!("editing_version_conflict: 项目在导出确认后发生变化，请重新核对")
    }
    let mut job = None;
    let result = match request.operation {
        ExportOperation::Transcript {
            format,
            output,
            language,
            subtitle_mode,
            allow_stale_translation,
        } => {
            let options = export::ExportOptions {
                format: &format,
                language: language.as_deref(),
                subtitle_mode,
                include_cuts: false,
                allow_stale_translation,
            };
            crate::export_delivery::transcript(&tx, &request.project_id, &output, &options)?
        }
        ExportOperation::Structured {
            format,
            output,
            include_speaker_labels,
            confirm_warnings,
        } => crate::export_delivery::structured(
            &tx,
            &request.project_id,
            &output,
            &format,
            include_speaker_labels,
            confirm_warnings,
        )?,
        ExportOperation::Video {
            output,
            language,
            subtitle_mode,
            subtitle_delivery,
            allow_stale_translation,
        } => {
            let next = video_export::register(
                &mut tx,
                &request.project_id,
                video_export::ExportRequest {
                    output: &output,
                    subtitle_delivery,
                    language,
                    subtitle_mode,
                    allow_stale_translation,
                    start_delay_ms: None,
                    job_id: None,
                },
            )?;
            let response = json!({"projectId":request.project_id,"jobId":next.id});
            job = Some(next);
            response
        }
    };
    tx.execute(
        "INSERT INTO project_commands VALUES(?1,?2,?3,?4)",
        params![
            request.mutation_id,
            raw,
            serde_json::to_string(&result)?,
            crate::util::now()
        ],
    )?;
    tx.commit()?;
    let mut result = result;
    if let Some(job) = job {
        video_export::launch(db, &job, None)?;
        result["job"] = serde_json::to_value(job)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_replay_keeps_original_file_and_rejects_stale_new_intents() {
        let temp = tempfile::tempdir().unwrap();
        let media = temp.path().join("audio.wav");
        std::fs::write(&media, b"audio").unwrap();
        let mut db = crate::db::open_at(&temp.path().join("test.db")).unwrap();
        let p = project::create(&mut db, &media, None).unwrap();
        let version = project::current_version_id(&db, &p.id).unwrap().unwrap();
        let output = temp.path().join("transcript.json");
        let command = |mutation: &str| ExportCommand {
            mutation_id: mutation.into(),
            project_id: p.id.clone(),
            expected_version_id: version.clone(),
            operation: ExportOperation::Structured {
                format: "json".into(),
                output: output.clone(),
                include_speaker_labels: true,
                confirm_warnings: false,
            },
        };
        let receipt = execute(&mut db, command("once")).unwrap();
        let text = std::fs::read_to_string(&output).unwrap();
        project::add_segment(&mut db, &p.id, 0.0, 1.0, "new content".into(), None).unwrap();
        assert_eq!(execute(&mut db, command("once")).unwrap(), receipt);
        assert_eq!(std::fs::read_to_string(&output).unwrap(), text);
        assert!(
            execute(&mut db, command("new-stale"))
                .unwrap_err()
                .to_string()
                .contains("editing_version_conflict")
        );
        assert_eq!(std::fs::read_to_string(&output).unwrap(), text);
        let mut reused = command("once");
        reused.expected_version_id = "different".into();
        assert!(
            execute(&mut db, reused)
                .unwrap_err()
                .to_string()
                .contains("editing_mutation_reused")
        );
    }
}
