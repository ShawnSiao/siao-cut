use anyhow::Error;
use serde_json::{Value, json};

pub const BACKGROUND_JOB_STATUSES: &[&str] = &[
    "queued",
    "running",
    "finalizing",
    "cancelled",
    "interrupted",
    "failed",
    "completed",
];

pub const TRANSCRIPTION_JOB_STATUSES: &[&str] = &[
    "queued",
    "running",
    "finalizing",
    "awaiting_apply",
    "cancelled",
    "interrupted",
    "failed",
    "completed",
    "discarded",
];

pub const TASK_STATUSES: &[&str] = &[
    "queued",
    "claimed",
    "running",
    "review",
    "failed",
    "interrupted",
    "cancelled",
    "done",
    "completed",
];

pub const WORKFLOW_STATUSES: &[&str] = &[
    "waiting_agent",
    "needs_agent",
    "needs_review",
    "completed",
    "cancelled",
];

pub const AUTO_WORKFLOW_STATUSES: &[&str] = &[
    "queued",
    "running",
    "needs_agent",
    "needs_review",
    "failed",
    "interrupted",
    "cancelled",
    "completed",
];

pub const AUTO_WORKFLOW_STAGES: &[&str] = &[
    "import",
    "transcribe",
    "suggestions",
    "translate",
    "review",
    "audit",
    "export",
    "complete",
];

pub const AGENT_RUN_STATUSES: &[&str] = &[
    "queued",
    "running",
    "submitting",
    "completed",
    "cancelled",
    "interrupted",
    "failed",
];

pub const CORE_ERROR_CODES: &[&str] = &[
    "database_version_unsupported",
    "database_migration_failed",
    "project_version_conflict",
    "task_base_version_mismatch",
    "task_cancel_requested",
    "task_patch_already_submitted",
    "patch_before_mismatch",
    "codex_cli_missing",
    "codex_not_logged_in",
    "agent_run_not_found",
    "agent_run_active",
    "agent_run_not_cancellable",
    "agent_run_not_resumable",
    "agent_run_timeout",
    "agent_run_cancelled",
    "agent_worker_interrupted",
    "agent_output_invalid",
    "agent_batch_incomplete",
    "agent_segment_duplicate",
    "agent_segment_unauthorized",
    "agent_project_version_conflict",
    "core_service_unavailable",
    "core_service_no_response",
    "core_request_timeout",
    "media_hash_changed",
    "disk_space_low",
    "model_hash_mismatch",
    "translation_missing",
    "translation_stale",
    "translation_incomplete",
    "word_range_invalid",
    "word_alignment_stale",
    "history_undo_empty",
    "history_redo_empty",
    "subtitle_segment_not_found",
    "subtitle_split_time_invalid",
    "subtitle_split_text_invalid",
    "subtitle_split_crosses_word",
    "subtitle_merge_same",
    "subtitle_merge_separator_invalid",
    "subtitle_merge_not_adjacent",
    "subtitle_time_invalid",
    "subtitle_time_out_of_bounds",
    "subtitle_timing_unchanged",
    "subtitle_offset_invalid",
    "subtitle_offset_empty",
    "subtitle_offset_out_of_bounds",
    "subtitle_offset_word_out_of_bounds",
    "subtitle_import_format_unsupported",
    "subtitle_import_file_unreadable",
    "subtitle_import_encoding_unsupported",
    "subtitle_import_parse_failed",
    "subtitle_import_confirmation_required",
    "subtitle_import_hash_invalid",
    "subtitle_import_file_changed",
    "subtitle_import_quality_blocked",
    "subtitle_style_snapshot_invalid",
    "subtitle_style_preset_invalid",
    "subtitle_style_position_invalid",
    "subtitle_style_content_changed",
    "subtitle_style_color_invalid",
    "transcription_provider_invalid",
    "transcription_provider_unavailable",
    "transcription_job_not_found",
    "transcription_job_not_cancellable",
    "transcription_job_not_resumable",
    "transcription_response_invalid",
    "transcription_timing_invalid",
    "transcription_import_failed",
    "transcription_cancelled",
    "transcription_interrupted",
    "transcription_active_job_exists",
    "transcription_job_state_invalid",
    "transcription_project_changed",
    "transcription_source_changed",
    "transcription_result_not_ready",
    "transcription_apply_confirmation_required",
    "transcription_apply_version_mismatch",
    "transcription_export_format_invalid",
    "transcription_export_blocked",
    "transcription_export_warning_confirmation_required",
    "source_url_invalid",
    "source_https_required",
    "source_credentials_not_allowed",
    "source_private_network",
    "source_dns_failed",
    "source_tool_not_configured",
    "source_tool_hash_mismatch",
    "source_tool_version_mismatch",
    "source_inspection_failed",
    "source_metadata_invalid",
    "source_playlist_not_allowed",
    "source_auth_not_allowed",
    "source_duration_unknown",
    "source_duration_limit",
    "source_size_limit",
    "source_confirmation_mismatch",
    "source_job_not_found",
    "source_job_not_cancellable",
    "source_job_not_resumable",
    "source_tool_changed",
    "source_preflight_failed",
    "source_redirect_invalid",
    "source_redirect_limit",
    "source_selected_url_invalid",
    "source_download_failed",
    "source_output_invalid",
    "source_media_probe_failed",
    "auto_workflow_not_found",
    "auto_workflow_not_cancellable",
    "auto_workflow_not_resumable",
    "auto_workflow_input_invalid",
    "auto_workflow_model_missing",
    "speaker_package_installed",
    "speaker_package_missing",
    "speaker_package_hash_mismatch",
    "speaker_job_not_found",
    "speaker_job_not_cancellable",
    "speaker_job_not_resumable",
    "speaker_source_missing",
    "speaker_runtime_failed",
    "speaker_audio_prepare_failed",
    "speaker_label_invalid",
    "speaker_not_found",
    "speaker_merge_same",
    "speaker_assignment_invalid",
    "auto_workflow_media_missing",
    "auto_workflow_output_invalid",
    "auto_workflow_confirmation_required",
    "auto_workflow_translation_required",
    "auto_workflow_subtitle_mode_invalid",
    "auto_workflow_review_pending",
    "auto_workflow_state_invalid",
    "auto_workflow_source_failed",
    "auto_workflow_agent_cancelled",
    "auto_workflow_audit_failed",
    "auto_workflow_export_failed",
    "audio_source_missing",
    "audio_job_not_found",
    "audio_job_not_cancellable",
    "audio_job_not_resumable",
    "audio_analysis_unavailable",
    "audio_analysis_failed",
    "audio_analysis_invalid_output",
    "audio_duration_unavailable",
    "job_interrupted",
    "job_failed",
    "invalid_request",
];

pub fn contract() -> Value {
    json!({
        "statusSets": {
            "backgroundJob": BACKGROUND_JOB_STATUSES,
            "transcriptionJob": TRANSCRIPTION_JOB_STATUSES,
            "task": TASK_STATUSES,
            "workflow": WORKFLOW_STATUSES,
            "autoWorkflow": AUTO_WORKFLOW_STATUSES,
            "autoWorkflowStage": AUTO_WORKFLOW_STAGES,
            "agentRun": AGENT_RUN_STATUSES,
        },
        "errorCodes": CORE_ERROR_CODES,
    })
}

pub fn error_code(error: &Error) -> &'static str {
    for cause in error.chain() {
        let message = cause.to_string();
        let candidate = message
            .split_once(':')
            .map_or(message.as_str(), |(prefix, _)| prefix)
            .trim();
        if let Some(code) = CORE_ERROR_CODES
            .iter()
            .copied()
            .find(|code| *code == candidate)
        {
            return code;
        }
    }
    "invalid_request"
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn resolves_only_explicit_error_prefixes_across_context_layers() {
        let error = anyhow!("source_private_network: blocked").context("source inspection failed");
        assert_eq!(error_code(&error), "source_private_network");
        assert_eq!(
            error_code(&anyhow!("mentions disk_space_low in prose")),
            "invalid_request"
        );
        assert_eq!(
            error_code(&anyhow!(
                "transcription_export_blocked: unresolved review item"
            )),
            "transcription_export_blocked"
        );
    }

    #[test]
    fn contract_status_sets_are_unique() {
        for statuses in [
            BACKGROUND_JOB_STATUSES,
            TRANSCRIPTION_JOB_STATUSES,
            TASK_STATUSES,
            WORKFLOW_STATUSES,
            AUTO_WORKFLOW_STATUSES,
            AUTO_WORKFLOW_STAGES,
            CORE_ERROR_CODES,
        ] {
            let unique = statuses
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(unique.len(), statuses.len());
        }
    }
}
