//! Wire DTOs shared by Core contract generation and native desktop transport.
use serde::Serialize;

#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "DesktopRuntimeInfo")]
pub struct RuntimeInfo {
    pub core_path: String,
    pub core_api_version: String,
    pub ffmpeg_configured: bool,
    pub asr_configured: bool,
    pub vad_configured: bool,
    pub vad_timeline_verified: bool,
    pub vad_status: String,
    pub vad_reason_code: Option<String>,
    pub yt_dlp_configured: bool,
    pub asr_backend: String,
    pub asr_device: Option<String>,
    pub available_asr_backends: Vec<String>,
    pub ffmpeg_path: Option<String>,
    pub whisper_path: Option<String>,
    pub yt_dlp_path: Option<String>,
    pub runtime_manifest_path: Option<String>,
    pub default_model_path: String,
    pub default_model_available: bool,
    pub log_directory: Option<String>,
    pub diagnostics_available: bool,
}

#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePolicy {
    pub current_version: String,
    pub enabled: bool,
    pub automatic_check_interval_hours: u8,
    pub disabled_reason: Option<String>,
}

#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    #[ts(type = "number")]
    pub size_bytes: u64,
}

#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        #[ts(type = "number | null")]
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
    Verifying,
}
