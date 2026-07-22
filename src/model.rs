use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum CanvasAspectRatio {
    #[default]
    #[serde(rename = "source")]
    Source,
    #[serde(rename = "9:16")]
    Vertical,
}

impl CanvasAspectRatio {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "source" => Some(Self::Source),
            "9:16" => Some(Self::Vertical),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Vertical => "9:16",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CanvasFraming {
    #[default]
    ContainBlur,
    CoverCenter,
}

impl CanvasFraming {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "contain-blur" => Some(Self::ContainBlur),
            "cover-center" => Some(Self::CoverCenter),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ContainBlur => "contain-blur",
            Self::CoverCenter => "cover-center",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CanvasSettings {
    pub aspect_ratio: CanvasAspectRatio,
    pub framing: CanvasFraming,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleMode {
    #[default]
    Source,
    Translated,
    Bilingual,
}

impl SubtitleMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "source" => Some(Self::Source),
            "translated" => Some(Self::Translated),
            "bilingual" => Some(Self::Bilingual),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Translated => "translated",
            Self::Bilingual => "bilingual",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SubtitleStylePreset {
    Compact,
    #[default]
    Standard,
    Emphasis,
}

impl SubtitleStylePreset {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "compact" => Some(Self::Compact),
            "standard" => Some(Self::Standard),
            "emphasis" => Some(Self::Emphasis),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SubtitlePosition {
    #[default]
    Bottom,
    Center,
}

impl SubtitlePosition {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "bottom" => Some(Self::Bottom),
            "center" => Some(Self::Center),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleStyle {
    pub preset: SubtitleStylePreset,
    pub position: SubtitlePosition,
    pub font_family: String,
    pub bold: bool,
    pub font_size: u16,
    pub secondary_font_size: u16,
    pub primary_color: String,
    pub secondary_color: String,
    pub outline_color: String,
    pub outline_width: u8,
    pub shadow_depth: u8,
    pub safe_margin_percent: u8,
}

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self {
            preset: SubtitleStylePreset::Standard,
            position: SubtitlePosition::Bottom,
            font_family: "Microsoft YaHei UI".to_owned(),
            bold: true,
            font_size: 52,
            secondary_font_size: 40,
            primary_color: "#F2F4F5".to_owned(),
            secondary_color: "#B5BEC6".to_owned(),
            outline_color: "#080A0D".to_owned(),
            outline_width: 3,
            shadow_depth: 1,
            safe_margin_percent: 8,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub confidence: Option<f64>,
}

pub fn reconcile_source_language<'a>(
    reported: &str,
    texts: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut latin_letters = 0usize;
    let mut han_characters = 0usize;
    for character in texts.into_iter().flat_map(str::chars) {
        if character.is_ascii_alphabetic() {
            latin_letters += 1;
        } else if matches!(
            character as u32,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        ) {
            han_characters += 1;
        }
    }
    let normalized = reported.trim().to_ascii_lowercase();
    if normalized.starts_with("zh")
        && latin_letters >= 40
        && latin_letters >= han_characters.saturating_mul(4).saturating_add(20)
    {
        "en".to_owned()
    } else if normalized.starts_with("en")
        && han_characters >= 20
        && han_characters >= latin_letters.saturating_mul(2)
    {
        "zh".to_owned()
    } else {
        normalized
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub id: String,
    pub segment_id: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub confidence: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleQualityStatus {
    #[default]
    Good,
    Warning,
    Error,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleIssueSeverity {
    Warning,
    Error,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleIssueKind {
    EmptyText,
    InvalidTiming,
    OutOfBounds,
    Overlap,
    DurationTooLong,
    LineTooLong,
    TooManyLines,
    ReadingSpeedHigh,
    GapTooShort,
}

impl SubtitleIssueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmptyText => "empty_text",
            Self::InvalidTiming => "invalid_timing",
            Self::OutOfBounds => "out_of_bounds",
            Self::Overlap => "overlap",
            Self::DurationTooLong => "duration_too_long",
            Self::LineTooLong => "line_too_long",
            Self::TooManyLines => "too_many_lines",
            Self::ReadingSpeedHigh => "reading_speed_high",
            Self::GapTooShort => "gap_too_short",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitleQualityThresholds {
    pub max_duration_seconds: f64,
    pub max_line_characters: usize,
    pub max_characters_per_second: f64,
    pub min_gap_seconds: f64,
    pub max_lines: usize,
}

impl Default for SubtitleQualityThresholds {
    fn default() -> Self {
        Self {
            max_duration_seconds: 8.0,
            max_line_characters: 42,
            max_characters_per_second: 20.0,
            min_gap_seconds: 0.12,
            max_lines: 2,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleQualityIssue {
    pub id: String,
    pub kind: SubtitleIssueKind,
    pub severity: SubtitleIssueSeverity,
    pub segment_id: String,
    pub related_segment_id: Option<String>,
    pub start: f64,
    pub end: f64,
    pub message: String,
    pub measured_value: Option<f64>,
    pub threshold: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleQualityReport {
    pub status: SubtitleQualityStatus,
    pub status_label: String,
    pub issue_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub thresholds: SubtitleQualityThresholds,
    pub issues: Vec<SubtitleQualityIssue>,
}

impl Default for SubtitleQualityReport {
    fn default() -> Self {
        Self {
            status: SubtitleQualityStatus::Good,
            status_label: "未发现字幕问题".to_owned(),
            issue_count: 0,
            error_count: 0,
            warning_count: 0,
            thresholds: SubtitleQualityThresholds::default(),
            issues: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeechInsightStatus {
    #[default]
    InsufficientEvidence,
    Ready,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeechPauseSeverity {
    Pause,
    LongPause,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeechEvidenceKind {
    Filler,
    LowConfidence,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpeechInsightThresholds {
    pub pause_seconds: f64,
    pub long_pause_seconds: f64,
    pub low_confidence: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeechPause {
    pub start: f64,
    pub end: f64,
    pub duration: f64,
    pub previous_word_id: String,
    pub next_word_id: String,
    pub severity: SpeechPauseSeverity,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeechEvidence {
    pub kind: SpeechEvidenceKind,
    pub word_id: String,
    pub segment_id: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub confidence: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpeechInsights {
    pub status: SpeechInsightStatus,
    pub analyzer_version: String,
    pub thresholds: SpeechInsightThresholds,
    pub span_duration_seconds: f64,
    pub spoken_duration_seconds: f64,
    pub token_count: usize,
    pub tokens_per_minute: f64,
    pub pause_count: usize,
    pub long_pause_count: usize,
    pub total_pause_duration_seconds: f64,
    pub filler_count: usize,
    pub low_confidence_count: usize,
    pub pauses: Vec<SpeechPause>,
    pub evidence: Vec<SpeechEvidence>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub source_path: String,
    pub sha256: String,
    pub extension: String,
    pub duration_seconds: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaArtifacts {
    pub status: String,
    pub proxy_path: Option<String>,
    pub waveform_path: Option<String>,
    pub thumbnails: Vec<String>,
    pub source_sha256: String,
    pub updated_at: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TimelineRange {
    pub source_start: f64,
    pub source_end: f64,
    pub output_start: f64,
    pub output_end: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TimelineCut {
    pub edit_ids: Vec<String>,
    pub source_start: f64,
    pub source_end: f64,
    pub output_at: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TimelineMap {
    pub source_duration: f64,
    pub output_duration: f64,
    pub kept_ranges: Vec<TimelineRange>,
    pub cuts: Vec<TimelineCut>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportJob {
    pub id: String,
    pub project_id: String,
    pub output_path: String,
    pub status: String,
    pub stage_code: Option<String>,
    pub progress: f64,
    pub burn_subtitles: bool,
    pub language: Option<String>,
    pub bilingual: bool,
    pub subtitle_mode: SubtitleMode,
    #[serde(default)]
    pub allow_stale_translation: bool,
    pub canvas_settings: CanvasSettings,
    pub subtitle_style: SubtitleStyle,
    pub cancel_requested_at: Option<String>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub manifest_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub worker_pid: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub status: String,
    pub updated_at: String,
    #[serde(default)]
    pub glossary_version: u32,
    pub segments: Vec<TranslationSegment>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSegment {
    pub segment_id: String,
    pub text: String,
    #[serde(default)]
    pub source_hash: String,
    #[serde(default = "default_stale_translation_status")]
    pub status: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_stale_translation_status() -> String {
    "stale".to_owned()
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Glossary {
    pub version: u32,
    pub updated_at: String,
    pub entries: Vec<GlossaryEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryEntry {
    pub language: String,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub segment_id: String,
    pub start: f64,
    pub end: f64,
    pub reason: String,
    pub created_at: String,
    #[serde(default)]
    pub cut_range: Option<CutRange>,
    #[serde(default)]
    pub suggestion: Option<CutSuggestion>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CutRange {
    pub from_word_id: String,
    pub to_word_id: String,
    pub selected_start: f64,
    pub selected_end: f64,
    pub padding_ms: u32,
    pub transcript_hash: String,
    pub stale: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CutSuggestion {
    pub suggestion_type: String,
    pub confidence: f64,
    pub detector_version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub kind: String,
    pub language: Option<String>,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub lease: Option<Lease>,
    #[serde(default)]
    pub last_activity: Option<TaskActivity>,
    pub base_version_id: Option<String>,
    pub progress: f64,
    pub error_message: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    pub attempt_count: i64,
    pub cancel_requested_at: Option<String>,
    #[serde(default)]
    pub workflow_id: Option<String>,
    #[serde(default = "default_instruction_locale")]
    pub instruction_locale: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPatchItem {
    pub id: String,
    pub segment_id: Option<String>,
    pub target: String,
    pub before_text: String,
    pub after_text: String,
    pub current_text: String,
    pub reason: String,
    pub confidence: Option<f64>,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentPatchSet {
    pub id: String,
    pub task_id: String,
    pub kind: String,
    pub language: Option<String>,
    pub status: String,
    pub base_version_id: String,
    pub created_at: String,
    pub items: Vec<AgentPatchItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunBatch {
    pub id: String,
    pub ordinal: u32,
    pub status: String,
    pub segment_ids: Vec<String>,
    pub codex_thread_id: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub attempt_count: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    pub id: String,
    pub task_id: String,
    pub project_id: String,
    pub provider: String,
    pub status: String,
    pub base_version_id: String,
    pub progress: f64,
    pub current_batch: u32,
    pub batch_count: u32,
    pub timeout_seconds: u64,
    pub cli_version: Option<String>,
    pub auth_mode: Option<String>,
    pub codex_thread_id: Option<String>,
    pub cancel_requested_at: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub worker_pid: Option<u32>,
    pub attempt_count: u32,
    pub batches: Vec<AgentRunBatch>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub kind: String,
    pub language: Option<String>,
    pub status: String,
    pub task_id: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default = "default_instruction_locale")]
    pub instruction_locale: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoWorkflow {
    pub id: String,
    pub input_kind: String,
    pub input_value: String,
    pub title: Option<String>,
    pub confirmed_media_id: Option<String>,
    pub project_id: Option<String>,
    pub source_import_id: Option<String>,
    pub model_path: String,
    pub transcribe_language: Option<String>,
    pub translation_language: Option<String>,
    pub output_path: String,
    pub burn_subtitles: bool,
    pub subtitle_mode: SubtitleMode,
    pub status: String,
    pub current_stage: String,
    pub progress: f64,
    pub transcript_version_id: Option<String>,
    pub agent_task_id: Option<String>,
    pub export_job_id: Option<String>,
    pub audit: Option<serde_json::Value>,
    pub cancel_requested_at: Option<String>,
    pub error_message: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub worker_pid: Option<u32>,
    pub attempt_count: u32,
    #[serde(default = "default_instruction_locale")]
    pub instruction_locale: String,
}

pub fn background_error_code(status: &str, error_message: Option<&str>) -> Option<String> {
    if status == "interrupted" {
        return Some("job_interrupted".to_owned());
    }
    if let Some(prefix) = error_message
        .and_then(|message| message.split_once(':'))
        .map(|(value, _)| value.trim())
        && crate::contracts::CORE_ERROR_CODES.contains(&prefix)
    {
        return Some(prefix.to_owned());
    }
    (status == "failed").then(|| "job_failed".to_owned())
}

fn default_instruction_locale() -> String {
    "zh-CN".to_owned()
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoWorkflowEvent {
    pub id: i64,
    pub workflow_id: String,
    pub stage: String,
    pub status: String,
    pub progress: f64,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub id: i64,
    pub task_id: String,
    pub project_id: String,
    pub kind: String,
    pub progress: Option<f64>,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskActivity {
    pub kind: String,
    pub progress: Option<f64>,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    pub worker: String,
    pub id: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub current_version_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub canvas_settings: CanvasSettings,
    #[serde(default)]
    pub subtitle_style: SubtitleStyle,
    pub media: Media,
    #[serde(default)]
    pub media_artifacts: Option<MediaArtifacts>,
    #[serde(default)]
    pub timeline: TimelineMap,
    pub transcript: Transcript,
    #[serde(default)]
    pub subtitle_quality: SubtitleQualityReport,
    #[serde(default)]
    pub speech_insights: SpeechInsights,
    pub translations: BTreeMap<String, Translation>,
    #[serde(default)]
    pub glossary: Glossary,
    pub edits: Vec<Edit>,
    pub tasks: Vec<Task>,
    pub versions: Vec<Version>,
    #[serde(default)]
    pub history: HistoryState,
    #[serde(default)]
    pub patch_sets: Vec<AgentPatchSet>,
    #[serde(default)]
    pub workflows: Vec<Workflow>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub source_language: String,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub words: Vec<Word>,
}

#[cfg(test)]
mod tests {
    use super::{SubtitleQualityThresholds, background_error_code, reconcile_source_language};
    use serde_json::json;

    #[test]
    fn legacy_subtitle_thresholds_keep_existing_values_and_default_max_lines() {
        let thresholds: SubtitleQualityThresholds = serde_json::from_value(json!({
            "maxDurationSeconds": 6.5,
            "maxLineCharacters": 36,
            "maxCharactersPerSecond": 18.0,
            "minGapSeconds": 0.2
        }))
        .unwrap();

        assert_eq!(thresholds.max_duration_seconds, 6.5);
        assert_eq!(thresholds.max_line_characters, 36);
        assert_eq!(thresholds.max_characters_per_second, 18.0);
        assert_eq!(thresholds.min_gap_seconds, 0.2);
        assert_eq!(thresholds.max_lines, 2);
    }

    #[test]
    fn background_error_codes_preserve_machine_prefixes_and_hide_raw_messages() {
        assert_eq!(
            background_error_code("failed", Some("source_download_failed: download failed")),
            Some("source_download_failed".to_owned())
        );
        assert_eq!(
            background_error_code("failed", Some("无法启动外部进程")),
            Some("job_failed".to_owned())
        );
        assert_eq!(
            background_error_code("interrupted", Some("human readable detail")),
            Some("job_interrupted".to_owned())
        );
        assert_eq!(background_error_code("running", None), None);
    }

    #[test]
    fn transcript_language_reconciliation_requires_strong_script_evidence() {
        assert_eq!(
            reconcile_source_language(
                "ZH",
                [
                    "What racing reveals about working with artificial intelligence and how teams use data to improve performance."
                ],
            ),
            "en"
        );
        assert_eq!(
            reconcile_source_language(
                "en",
                ["今天讨论本地视频剪辑、字幕校对以及模型运行环境是否稳定可靠。"]
            ),
            "zh"
        );
        assert_eq!(reconcile_source_language("zh", ["OpenAI"]), "zh");
    }
}
