//! Desktop application entry point. Transport decoding stays separate from domain execution.
use crate::{ai_approval, editing, project_query, subtitle_workbench, transcription};
use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Debug, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum DesktopRequest {
    #[serde(rename = "project_command")]
    ProjectCommand {
        request: crate::project_commands::ProjectCommand,
    },
    #[serde(rename = "desktop_control")]
    Control {
        request: crate::desktop_control::DesktopControl,
    },
    #[serde(rename = "desktop_query")]
    Query {
        request: crate::desktop_query::DesktopQuery,
    },
    #[serde(rename = "project_query")]
    ProjectQuery {
        request: project_query::ProjectQuery,
    },
    #[serde(rename = "ai_approval")]
    AiApproval {
        request: ai_approval::AiApprovalRequest,
    },
    #[serde(rename = "transcription_job")]
    TranscriptionJob {
        request: transcription::desktop::TranscriptionCommand,
    },
    #[serde(rename = "editing")]
    Editing { request: editing::EditingRequest },
    #[serde(rename = "transcript_offset")]
    TranscriptOffset {
        #[serde(rename = "projectId")]
        project_id: String,
        #[serde(rename = "segmentIds")]
        segment_ids: Vec<String>,
        delta: f64,
    },
    #[serde(rename = "transcription_start")]
    TranscriptionStart {
        #[serde(rename = "projectId")]
        project_id: String,
        language: String,
        #[serde(default)]
        #[ts(optional)]
        prompt: Option<String>,
        hotwords: Vec<String>,
    },
}

fn validate_desktop_request_text(label: &str, value: &str, max_chars: usize) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > max_chars || value.contains('\0') {
        bail!("invalid_request: Desktop 结构化请求中的{label}无效")
    }
    Ok(())
}

pub fn execute(database: &mut rusqlite::Connection, request: DesktopRequest) -> Result<Value> {
    match request {
        DesktopRequest::ProjectCommand { request } => {
            crate::project_commands::execute(database, request)
        }
        DesktopRequest::Control { request } => crate::desktop_control::execute(database, request),
        DesktopRequest::Query { request } => crate::desktop_query::execute(database, request),
        DesktopRequest::ProjectQuery { request } => project_query::execute(database, request),
        DesktopRequest::AiApproval { request } => ai_approval::execute(database, request),
        DesktopRequest::Editing { request } => editing::execute(database, request),
        DesktopRequest::TranscriptOffset {
            project_id,
            segment_ids,
            delta,
        } => {
            validate_desktop_request_text("项目 ID", &project_id, 256)?;
            if segment_ids.is_empty()
                || segment_ids.len() > 1000
                || segment_ids
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != segment_ids.len()
            {
                bail!("invalid_request: Desktop 批量偏移请求无效")
            }
            for segment_id in &segment_ids {
                validate_desktop_request_text("字幕段 ID", segment_id, 256)?;
            }
            let result = subtitle_workbench::offset(database, &project_id, &segment_ids, delta)?;
            Ok(json!({
                "projectId": project_id,
                "structureEdit": result,
                "message": "选中字幕与对应词级证据已批量偏移。"
            }))
        }
        DesktopRequest::TranscriptionJob { request } => {
            transcription::desktop::execute(database, request)
        }
        DesktopRequest::TranscriptionStart {
            project_id,
            language,
            prompt,
            hotwords,
        } => {
            validate_desktop_request_text("项目 ID", &project_id, 256)?;
            if !["auto", "en", "zh"].contains(&language.as_str()) || hotwords.len() > 512 {
                bail!("invalid_request: Desktop 多人转写请求无效")
            }
            if let Some(value) = prompt.as_deref() {
                validate_desktop_request_text("Prompt", value, 1200)?;
            }
            for hotword in &hotwords {
                validate_desktop_request_text("热词", hotword, 200)?;
            }
            let job = transcription::start(
                database,
                &project_id,
                Some(&language),
                prompt.as_deref(),
                &hotwords,
                None,
            )?;
            Ok(json!({
                "transcriptionJob": job,
                "message": "多人长音频转写已进入后台队列；不会静默回退到快速转写。"
            }))
        }
    }
}
