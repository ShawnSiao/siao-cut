//! Shared export application services used by CLI and guarded desktop commands.
use crate::{
    export::{self, ExportOptions},
    project, transcription,
};
use anyhow::{Result, bail};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::path::Path;
pub fn transcript(
    db: &Connection,
    project_id: &str,
    output: &Path,
    options: &ExportOptions<'_>,
) -> Result<Value> {
    let project = project::load(db, project_id)?;
    export::validate_subtitle_mode(&project, options)?;
    let report = export::audit_for_options(&project, options);
    if report["ready"] != true {
        bail!("导出前审计未通过，请先处理无效字幕或媒体问题")
    }
    std::fs::write(output, export::render(&project, options)?)?;
    Ok(
        json!({"projectId":project_id,"output":output,"format":options.format,"subtitleMode":options.subtitle_mode,"audit":report}),
    )
}
pub fn structured(
    db: &Connection,
    project_id: &str,
    output: &Path,
    format: &str,
    include_speaker_labels: bool,
    confirm_warnings: bool,
) -> Result<Value> {
    let (text, audit) = transcription::render_structured_export(
        db,
        project_id,
        format,
        include_speaker_labels,
        confirm_warnings,
    )?;
    std::fs::write(output, text)?;
    Ok(
        json!({"projectId":project_id,"output":output,"format":format,"audit":audit,"message":"结构化多人转写已导出；JSON/Markdown 保留说话人证据。"}),
    )
}
