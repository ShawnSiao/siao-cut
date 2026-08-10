use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiError {
    #[error("配置已经在其他位置发生变化")]
    RevisionConflict,
    #[error("{0}")]
    Validation(String),
    #[error("无法读取 AI 服务配置")]
    ConfigurationRead,
    #[error("无法保存 AI 服务配置")]
    ConfigurationWrite,
    #[error("无法读取安全凭据")]
    CredentialRead,
    #[error("无法保存安全凭据")]
    CredentialWrite,
    #[error("无法删除安全凭据")]
    CredentialDelete,
    #[cfg_attr(windows, allow(dead_code))]
    #[error("当前系统不支持安全凭据存储")]
    CredentialUnsupported,
    #[error("未找到对应 AI 服务")]
    ServiceNotFound,
    #[error("尚未保存 API Key")]
    CredentialMissing,
    #[error("API Key 无效或已失效")]
    Unauthorized,
    #[error("当前 API Key 没有访问权限")]
    Forbidden,
    #[error("所选模型不可用")]
    ModelNotFound,
    #[error("请求过于频繁，请稍后重试")]
    RateLimited,
    #[error("连接 AI 服务超时")]
    Timeout,
    #[error("AI 服务暂时不可用")]
    ProviderUnavailable,
    #[error("AI 服务返回了无法识别的内容")]
    InvalidResponse,
    #[allow(dead_code)]
    #[error("AI 服务配置已经改变，请重新确认")]
    ServiceRevisionChanged,
    #[error("发送内容超过单次任务限制")]
    PayloadTooLarge,
    #[allow(dead_code)]
    #[error("AI 任务已取消")]
    Cancelled,
}

impl AiError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::RevisionConflict => "revision_conflict",
            Self::Validation(_) => "validation_failed",
            Self::ConfigurationRead => "configuration_read_failed",
            Self::ConfigurationWrite => "configuration_write_failed",
            Self::CredentialRead => "credential_read_failed",
            Self::CredentialWrite => "credential_write_failed",
            Self::CredentialDelete => "credential_delete_failed",
            Self::CredentialUnsupported => "credential_unsupported",
            Self::ServiceNotFound => "ai_service_not_found",
            Self::CredentialMissing => "credential_missing",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::ModelNotFound => "model_not_found",
            Self::RateLimited => "rate_limited",
            Self::Timeout => "timeout",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::InvalidResponse => "invalid_response",
            Self::ServiceRevisionChanged => "service_revision_changed",
            Self::PayloadTooLarge => "payload_too_large",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::Timeout | Self::ProviderUnavailable
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCommandError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_request_id: Option<String>,
}

impl From<AiError> for AiCommandError {
    fn from(error: AiError) -> Self {
        Self {
            code: error.code(),
            retryable: error.retryable(),
            message: error.to_string(),
            provider_request_id: None,
        }
    }
}

impl AiCommandError {
    pub fn with_request_id(mut self, request_id: Option<String>) -> Self {
        self.provider_request_id = request_id;
        self
    }
}
