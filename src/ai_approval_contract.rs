use crate::agent::execution::ExecutionTarget;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiSendSpec {
    pub project_id: String,
    pub expected_version_id: String,
    pub kind: String,
    pub language: Option<String>,
    pub instruction_locale: String,
    pub task_id: Option<String>,
    pub target: ExecutionTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AiSendPreview {
    pub approval_id: String,
    pub payload_hash: String,
    pub spec: AiSendSpec,
    pub receiver: String,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub receiver_verified: bool,
    pub configuration_revision: String,
    pub segment_count: u32,
    pub character_count: u32,
    pub start_time: f64,
    pub end_time: f64,
    /// Canonical JSON of the exact text task, including glossary/context and constraints.
    pub payload_json: String,
}

#[derive(Debug, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AiApprovalRequest {
    Preview {
        spec: AiSendSpec,
    },
    Execute {
        #[serde(rename = "approvalId")]
        approval_id: String,
    },
}
