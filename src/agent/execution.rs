use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionTarget {
    Codex,
    Api {
        service_config_id: String,
        service_revision: u64,
        network_revision: u64,
        model_id: String,
    },
}

impl ExecutionTarget {
    pub fn from_cli(
        execution: &str,
        service_config_id: Option<String>,
        service_revision: Option<u64>,
        network_revision: Option<u64>,
        model_id: Option<String>,
    ) -> Result<Self> {
        if execution == "codex" {
            if service_config_id.is_some()
                || service_revision.is_some()
                || network_revision.is_some()
                || model_id.is_some()
            {
                bail!("invalid_request: 本机 Codex 不能绑定 API 服务参数")
            }
            return Ok(Self::Codex);
        }
        if execution != "api" {
            bail!("invalid_request: 不支持的 AI 执行方式")
        }
        let target = Self::Api {
            service_config_id: service_config_id
                .ok_or_else(|| anyhow!("invalid_request: API 执行缺少服务标识"))?,
            service_revision: service_revision
                .ok_or_else(|| anyhow!("invalid_request: API 执行缺少服务修订号"))?,
            network_revision: network_revision
                .ok_or_else(|| anyhow!("invalid_request: API 执行缺少网络修订号"))?,
            model_id: model_id.ok_or_else(|| anyhow!("invalid_request: API 执行缺少模型"))?,
        };
        target.validate()?;
        Ok(target)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn auto_from_cli(
        execution: &str,
        service_config_id: Option<String>,
        service_revision: Option<u64>,
        network_revision: Option<u64>,
        model_id: Option<String>,
        confirmed: bool,
        translation_enabled: bool,
    ) -> Result<Option<Self>> {
        if execution == "manual" {
            if service_config_id.is_some()
                || service_revision.is_some()
                || network_revision.is_some()
                || model_id.is_some()
                || confirmed
            {
                bail!("invalid_request: 手工翻译不使用 AI 服务参数")
            }
            return Ok(None);
        }
        if !translation_enabled {
            bail!("auto_workflow_translation_required: AI 执行目标只能用于字幕翻译")
        }
        if !confirmed {
            bail!("confirmation_required: 自动工作流发送字幕文本前需要显式确认")
        }
        Self::from_cli(
            execution,
            service_config_id,
            service_revision,
            network_revision,
            model_id,
        )
        .map(Some)
    }

    pub fn from_run(run: &crate::model::AgentRun) -> Result<Self> {
        let target = match run.execution_kind.as_str() {
            "codex" => Self::Codex,
            "api" => Self::Api {
                service_config_id: run
                    .service_config_id
                    .clone()
                    .ok_or_else(|| anyhow!("provider_not_configured: 运行记录缺少 AI 服务"))?,
                service_revision: run
                    .service_revision
                    .ok_or_else(|| anyhow!("service_revision_changed: 运行记录缺少服务修订号"))?,
                network_revision: run
                    .network_revision
                    .ok_or_else(|| anyhow!("service_revision_changed: 运行记录缺少网络修订号"))?,
                model_id: run
                    .model_id
                    .clone()
                    .ok_or_else(|| anyhow!("model_not_found: 运行记录缺少模型"))?,
            },
            _ => bail!("provider_not_supported: 不支持的 AI 执行方式"),
        };
        target.validate()?;
        Ok(target)
    }

    pub fn validate(&self) -> Result<()> {
        if let Self::Api {
            service_config_id,
            model_id,
            ..
        } = self
        {
            if service_config_id.is_empty()
                || service_config_id.len() > 128
                || !service_config_id
                    .chars()
                    .all(|value| value.is_ascii_alphanumeric() || value == '-')
            {
                bail!("validation_failed: AI 服务标识无效")
            }
            if model_id.trim().is_empty()
                || model_id.len() > 200
                || model_id.contains(['\r', '\n', '\0'])
            {
                bail!("validation_failed: 模型名称无效")
            }
        }
        Ok(())
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Api { .. } => "api",
        }
    }
}
