use super::{ProviderConfig, ProviderHealth};
use anyhow::Result;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProviderSegment {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text: String,
}

pub(crate) struct ProviderRequest<'a> {
    pub endpoint: &'a str,
    pub model_id: &'a str,
    pub language: Option<&'a str>,
    pub prompt: Option<&'a str>,
    pub hotwords: &'a [String],
    pub audio_path: &'a Path,
}

pub(crate) trait TranscriptionProvider: Sync {
    fn provider_id(&self) -> &'static str;
    fn validate_endpoint(&self, endpoint: &str) -> Result<String>;
    fn health(&self, config: &ProviderConfig) -> Result<ProviderHealth>;
    fn transcribe(&self, request: ProviderRequest<'_>) -> Result<String>;
    fn parse(&self, raw: &str) -> Result<Vec<ProviderSegment>>;
}
