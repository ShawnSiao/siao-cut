//! Wire DTOs for journal and versioned desktop editing.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub project_id: String,
    pub session_id: String,
    pub segment_id: String,
    /// `source` or `translation:<language>`.
    pub field: String,
    pub base_version_id: Option<String>,
    pub base_text: String,
    pub text: String,
    pub revision: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveEdit {
    pub mutation_id: String,
    pub expected_version_id: Option<String>,
    pub group_id: String,
    pub draft: Draft,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EditReceipt {
    pub mutation_id: String,
    pub project_id: String,
    pub version_id: String,
    pub segment_id: String,
    pub field: String,
    pub text: String,
    pub changed_domains: Vec<String>,
    #[serde(default)]
    pub history: Option<crate::model::HistoryState>,
    #[serde(default)]
    pub version: Option<crate::model::Version>,
}

#[derive(Debug, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditingRequest {
    Mutate {
        mutation: ProjectMutation,
    },
    Journal {
        draft: Draft,
    },
    List {
        #[serde(rename = "projectId")]
        project_id: String,
    },
    Discard {
        draft: Draft,
    },
    Save {
        edit: SaveEdit,
    },
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMutation {
    pub project_id: String,
    pub mutation_id: String,
    pub expected_version_id: Option<String>,
    pub operation: ProjectOperation,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProjectOperation {
    ResolveTranscriptionReview {
        item_id: String,
        action: String,
    },
    RelinkMedia {
        path: String,
    },
    ReplaceGlossary {
        language: String,
        expected_glossary_version: u32,
        entries: Vec<(String, String)>,
    },
    CreateWorkflow {
        workflow_kind: String,
        language: Option<String>,
        locale: String,
    },
    ImportSubtitle {
        path: String,
        sha256: String,
        preview_version_id: String,
    },
    ReviewPatch {
        patch_item_id: String,
        action: String,
    },
    ReviewAll {
        task_id: String,
        action: String,
    },
    DetectCuts,
    SetCutStatus {
        edit_id: String,
        action: String,
    },
    CreateWordCut {
        segment_id: String,
        from_word_id: String,
        to_word_id: String,
        padding_ms: u32,
    },
    Split {
        segment_id: String,
        text_offset: u32,
        at: f64,
    },
    Merge {
        first_id: String,
        second_id: String,
    },
    Timing {
        segment_id: String,
        start: f64,
        end: f64,
    },
    Offset {
        segment_ids: Vec<String>,
        delta: f64,
    },
    Replace {
        search: String,
        replacement: String,
    },
    Undo,
    Redo,
    Restore {
        version_id: String,
    },
    Canvas {
        aspect_ratio: String,
        framing: String,
    },
    Style {
        preset: String,
        position: String,
        source_font_size: Option<u16>,
        translation_font_size: Option<u16>,
        box_width_percent: Option<u8>,
        box_height_lines: Option<u8>,
    },
    RenameSpeaker {
        speaker_id: String,
        name: String,
    },
    MergeSpeaker {
        from_id: String,
        into_id: String,
    },
    AssignSpeaker {
        segment_id: String,
        speaker_id: String,
    },
}

impl ProjectOperation {
    /// Domains whose read models may change; transport adapters do not infer business effects.
    pub fn changed_domains(&self) -> &'static [&'static str] {
        use ProjectOperation::*;
        match self {
            ResolveTranscriptionReview { .. } => &["transcriptionReview"],
            RelinkMedia { .. } => &["media", "history"],
            ReplaceGlossary { .. } => &["glossary", "translations", "review", "history"],
            CreateWorkflow { .. } => &["review"],
            Canvas { .. } | Style { .. } => &["presentation", "history"],
            RenameSpeaker { .. } | MergeSpeaker { .. } | AssignSpeaker { .. } => {
                &["speakers", "transcriptionReview", "history"]
            }
            DetectCuts | SetCutStatus { .. } | CreateWordCut { .. } => {
                &["edits", "timeline", "history"]
            }
            Replace { .. } => &[
                "transcript",
                "translations",
                "edits",
                "insights",
                "review",
                "history",
            ],
            Split { .. } | Merge { .. } | Timing { .. } | Offset { .. } | ImportSubtitle { .. } => {
                &[
                    "transcript",
                    "translations",
                    "edits",
                    "speakers",
                    "insights",
                    "review",
                    "timeline",
                    "history",
                ]
            }
            ReviewPatch { .. } | ReviewAll { .. } => &[
                "transcript",
                "translations",
                "edits",
                "speakers",
                "insights",
                "review",
                "timeline",
                "history",
            ],
            Undo | Redo | Restore { .. } => &[
                "transcript",
                "translations",
                "glossary",
                "edits",
                "speakers",
                "insights",
                "review",
                "transcriptionReview",
                "timeline",
                "presentation",
                "history",
            ],
        }
    }
}
