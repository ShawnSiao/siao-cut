//! Automatic workflow setup application service, independent of CLI parsing.
use crate::{agent, auto_workflow, model};
use anyhow::{Result, anyhow, bail};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Debug, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartWorkflow {
    pub profile: String,
    pub media: Option<PathBuf>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub confirm_media_id: Option<String>,
    pub model: PathBuf,
    pub language: Option<String>,
    pub locale: String,
    pub translate: Option<String>,
    pub ai_execution: String,
    pub ai_service_config_id: Option<String>,
    #[ts(type = "number | null")]
    pub ai_service_revision: Option<u64>,
    #[ts(type = "number | null")]
    pub ai_network_revision: Option<u64>,
    pub ai_model_id: Option<String>,
    pub confirm_ai_text_send: bool,
    pub output: PathBuf,
    pub burn_subtitles: bool,
    pub subtitle_mode: String,
    #[ts(type = "number | null")]
    pub start_delay_ms: Option<u64>,
}
pub fn start(database: &mut Connection, arguments: StartWorkflow) -> Result<Value> {
    let StartWorkflow {
        profile,
        media,
        url,
        title,
        confirm_media_id,
        model,
        language,
        locale,
        translate,
        ai_execution,
        ai_service_config_id,
        ai_service_revision,
        ai_network_revision,
        ai_model_id,
        confirm_ai_text_send,
        output,
        burn_subtitles,
        subtitle_mode,
        start_delay_ms,
    } = arguments;
    let input = match (media, url) {
        (Some(media), None) => {
            if confirm_media_id.is_some() {
                bail!("auto_workflow_input_invalid: 本地文件不使用 --confirm-media-id")
            }
            auto_workflow::WorkflowInput::Local { media, title }
        }
        (None, Some(url)) => {
            if title.is_some() {
                bail!("auto_workflow_input_invalid: URL 标题来自预检结果，不使用 --title")
            }
            auto_workflow::WorkflowInput::Url {
                url,
                confirmed_media_id: confirm_media_id.ok_or_else(|| {
                    anyhow!(
                        "auto_workflow_confirmation_required: URL 输入必须提供 --confirm-media-id"
                    )
                })?,
            }
        }
        _ => bail!("auto_workflow_input_invalid: 必须且只能提供 --media 或 --url"),
    };
    let subtitle_mode = model::SubtitleMode::parse(&subtitle_mode).ok_or_else(|| {
        anyhow!(
            "auto_workflow_subtitle_mode_invalid: 字幕模式必须为 source、translated 或 bilingual"
        )
    })?;
    let profile = model::WorkflowProfile::parse(&profile).ok_or_else(|| {
        anyhow!("auto_workflow_profile_invalid: 流程预设必须为 draft、balanced 或 delivery")
    })?;
    let translation_execution = agent::execution::ExecutionTarget::auto_from_cli(
        &ai_execution,
        ai_service_config_id,
        ai_service_revision,
        ai_network_revision,
        ai_model_id,
        confirm_ai_text_send,
        translate.is_some(),
    )?;
    let workflow = auto_workflow::start(
        database,
        auto_workflow::StartRequest {
            input,
            model,
            transcribe_language: language,
            instruction_locale: locale,
            translation_language: translate,
            output,
            burn_subtitles,
            subtitle_mode,
            profile,
            start_delay_ms,
            translation_execution,
        },
    )?;
    Ok(json!({
        "workflowId": workflow.id,
        "workflow": workflow,
        "message": "自动工作流已启动；内容判断阶段仍会暂停等待确认。"
    }))
}
