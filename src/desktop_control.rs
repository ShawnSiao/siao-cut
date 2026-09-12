//! Typed lifecycle and resource operations. CLI and desktop share these application services.
use crate::{
    agent_runner, artifacts, audio_analysis, auto_workflow, local_resources, models, project,
    resource_jobs, source_import, speaker, tasks, transcription, video_export,
};
use anyhow::Result;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Debug, Deserialize, ts_rs::TS)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DesktopControl {
    MediaPrepare {
        project_id: String,
    },
    AutoContinue {
        workflow_id: String,
    },
    AutoCancel {
        workflow_id: String,
    },
    VideoRetry {
        job_id: String,
    },
    AgentResume {
        run_id: String,
        #[ts(type = "number | null")]
        start_delay_ms: Option<u64>,
    },
    ModelInstall {
        model_id: String,
    },
    ModelCancel {
        job_id: String,
    },
    ModelRemove {
        model_id: String,
    },
    SourceInspect {
        url: String,
        browser: Option<String>,
    },
    SourceStart {
        url: String,
        confirm_media_id: String,
        #[ts(type = "number | null")]
        start_delay_ms: Option<u64>,
        browser: Option<String>,
    },
    SourceCancel {
        job_id: String,
    },
    SourceResume {
        job_id: String,
    },
    AudioStart {
        project_id: String,
        #[ts(type = "number | null")]
        start_delay_ms: Option<u64>,
    },
    AudioCancel {
        job_id: String,
    },
    AudioResume {
        job_id: String,
        #[ts(type = "number | null")]
        start_delay_ms: Option<u64>,
    },
    SpeakerInstall,
    SpeakerCancel {
        job_id: String,
    },
    SpeakerResume {
        job_id: String,
    },
    SpeakerAnalyze {
        project_id: String,
    },
    ResourceCheckUpdates {
        capability: Option<String>,
    },
    ResourceConfigure {
        root: std::path::PathBuf,
    },
    ResourceMigrate {
        root: std::path::PathBuf,
    },
    ResourceInstall {
        capability: String,
        profile: Option<String>,
    },
    ResourceUpdate {
        capability: String,
        profile: Option<String>,
    },
    ResourceCancel {
        job_id: String,
    },
    ResourceResume {
        job_id: String,
    },
    ResourceRepair {
        capability: String,
    },
    ResourceRollback {
        capability: String,
    },
    ResourceRemove {
        capability: String,
    },
    ResourceCleanup,
    AgentCancel {
        run_id: String,
    },
    TaskRetry {
        task_id: String,
    },
    TaskCancel {
        task_id: String,
    },
    VideoCancel {
        job_id: String,
    },
    TranscriptionConfigure {
        endpoint: String,
        model: String,
    },
}
pub fn execute(database: &mut Connection, request: DesktopControl) -> Result<Value> {
    match request {
        DesktopControl::MediaPrepare { project_id } => {
            let artifacts = artifacts::prepare(database, &project_id)?;
            Ok(json!({
                "projectId":project_id,
                "artifacts":artifacts,
                "project":project::load(database,&project_id)?,
                "message":"预览资源已生成；原片未修改。"
            }))
        }
        DesktopControl::AutoContinue { workflow_id } => {
            let workflow = auto_workflow::continue_workflow(database, &workflow_id)?;
            Ok(json!({
                "workflowId": workflow.id,
                "workflow": workflow,
                "message": "自动工作流已显式继续。"
            }))
        }
        DesktopControl::AutoCancel { workflow_id } => {
            let workflow = auto_workflow::cancel(database, &workflow_id)?;
            Ok(json!({
                "workflowId": workflow.id,
                "workflow": workflow,
                "message": "自动工作流已取消。"
            }))
        }
        DesktopControl::VideoRetry { job_id } => {
            let job = video_export::retry(database, &job_id)?;
            Ok(json!({
                "projectId":job.project_id,
                "jobId":job.id,
                "job":job,
                "message":"视频导出已重新开始。"
            }))
        }
        DesktopControl::AgentResume {
            run_id,
            start_delay_ms,
        } => {
            let run = agent_runner::resume(database, &run_id, start_delay_ms)?;
            Ok(json!({
                "projectId": run.project_id,
                "taskId": run.task_id,
                "agentRunId": run.id,
                "agentRun": run,
                "message": "AI 辅助已重新排队。"
            }))
        }
        DesktopControl::ModelInstall { model_id } => {
            let job = models::create_download(database, &model_id)?;
            Ok(json!({
                "jobId": job.id,
                "modelJob": job,
                "message": "模型下载已开始；只会访问界面显示的模型来源。"
            }))
        }
        DesktopControl::ModelCancel { job_id } => {
            let job = models::cancel(database, &job_id)?;
            Ok(json!({
                "jobId":job.id,
                "modelJob":job,
                "message":"已请求取消模型下载；已下载部分可用于以后继续。"
            }))
        }
        DesktopControl::ModelRemove { model_id } => {
            models::remove(database, &model_id)?;
            Ok(json!({
                "modelId":model_id,
                "message":"模型已从本机移除；项目和原始媒体未受影响。"
            }))
        }
        DesktopControl::SourceInspect { url, browser } => {
            let source = match browser.as_deref() {
                Some(browser) => source_import::inspect_with_browser(&url, browser)?,
                None => source_import::inspect(&url)?,
            };
            Ok(json!({
                "source": source,
                "message": if browser.is_some() {
                    "已使用浏览器登录态读取单视频信息；确认前不会下载或创建项目。"
                } else {
                    "已读取公开单视频信息；确认前不会下载或创建项目。"
                }
            }))
        }
        DesktopControl::SourceStart {
            url,
            confirm_media_id,
            start_delay_ms,
            browser,
        } => {
            let job = match browser.as_deref() {
                Some(browser) => source_import::start_with_browser(
                    database,
                    &url,
                    &confirm_media_id,
                    start_delay_ms,
                    browser,
                )?,
                None => source_import::start(database, &url, &confirm_media_id, start_delay_ms)?,
            };
            Ok(json!({
                "sourceJobId": job.id,
                "sourceJob": job,
                "message": "已确认视频信息并开始后台下载；项目将在完整校验成功后创建。"
            }))
        }
        DesktopControl::SourceCancel { job_id } => {
            let job = source_import::cancel(database, &job_id)?;
            Ok(json!({
                "sourceJobId": job.id,
                "sourceJob": job,
                "message": "已请求取消 URL 导入；部分下载保留到显式继续。"
            }))
        }
        DesktopControl::SourceResume { job_id } => {
            let job = source_import::resume(database, &job_id)?;
            Ok(json!({
                "sourceJobId": job.id,
                "sourceJob": job,
                "message": "URL 导入已显式继续，将复用已下载部分。"
            }))
        }
        DesktopControl::AudioStart {
            project_id,
            start_delay_ms,
        } => {
            let job = audio_analysis::start(database, &project_id, start_delay_ms)?;
            Ok(json!({
                "audioAnalysisJob": job,
                "message": "已开始本地音频质量分析；媒体不会上传。"
            }))
        }
        DesktopControl::AudioCancel { job_id } => Ok(json!({
            "audioAnalysisJob": audio_analysis::cancel(database, &job_id)?,
            "message": "已请求取消本地音频分析。"
        })),
        DesktopControl::AudioResume {
            job_id,
            start_delay_ms,
        } => Ok(json!({
            "audioAnalysisJob": audio_analysis::resume(database, &job_id, start_delay_ms)?,
            "message": "已显式继续本地音频分析。"
        })),
        DesktopControl::SpeakerInstall => {
            let job = speaker::create_install(database)?;
            Ok(json!({
                "speakerJob": job,
                "message": "说话人模型包已进入本机下载队列；完成前不会启用分析。"
            }))
        }
        DesktopControl::SpeakerCancel { job_id } => Ok(json!({
            "speakerJob": speaker::cancel(database, &job_id)?,
            "message": "说话人任务已取消；字幕、剪辑和原片未修改。"
        })),
        DesktopControl::SpeakerResume { job_id } => Ok(json!({
            "speakerJob": speaker::resume(database, &job_id)?,
            "message": "说话人任务已显式继续。"
        })),
        DesktopControl::SpeakerAnalyze { project_id } => Ok(json!({
            "projectId": project_id,
            "speakerJob": speaker::create_analysis(database, &project_id)?,
            "message": "本地说话人分析已开始；结果只进入待审阅说话人轨。"
        })),
        DesktopControl::ResourceCheckUpdates { capability } => Ok(json!({
            "localResources": local_resources::status()?,
            "resourceUpdateCheck": local_resources::check_updates(capability.as_deref())?,
            "message": "已检查本地组件更新。"
        })),
        DesktopControl::ResourceConfigure { root } => Ok(json!({
            "localResources": local_resources::configure(database, &root)?,
            "message": "本地资源保存位置已设置。"
        })),
        DesktopControl::ResourceMigrate { root } => {
            let migration = local_resources::migrate(database, &root)?;
            Ok(json!({
                "localResources": migration.status.clone(),
                "resourceMigration": migration,
                "message": "本地资源保存位置已更改。"
            }))
        }
        DesktopControl::ResourceInstall {
            capability,
            profile,
        } => Ok(json!({
            "resourceJob": resource_jobs::create_install(database, &capability, profile.as_deref())?,
            "message": "正在准备所需的本地资源。"
        })),
        DesktopControl::ResourceUpdate {
            capability,
            profile,
        } => Ok(json!({
            "resourceJob": resource_jobs::create_install(database, &capability, profile.as_deref())?,
            "message": "正在更新所需的本地资源。"
        })),
        DesktopControl::ResourceCancel { job_id } => Ok(json!({
            "resourceJob": resource_jobs::cancel(database, &job_id)?,
            "message": "正在取消本地资源准备。"
        })),
        DesktopControl::ResourceResume { job_id } => Ok(json!({
            "resourceJob": resource_jobs::resume(database, &job_id)?,
            "message": "已继续准备本地资源。"
        })),
        DesktopControl::ResourceRepair { capability } => Ok(json!({
            "resourceJob": resource_jobs::repair(database, &capability)?,
            "message": "正在修复本地资源。"
        })),
        DesktopControl::ResourceRollback { capability } => {
            let rollback = local_resources::rollback(database, &capability)?;
            Ok(json!({
                "localResources": rollback.status.clone(),
                "resourceRollback": rollback,
                "message": "已恢复上一可用版本。"
            }))
        }
        DesktopControl::ResourceRemove { capability } => {
            resource_jobs::remove(database, &capability)?;
            Ok(json!({
                "localResources": local_resources::status()?,
                "message": "已移除所选本地能力。"
            }))
        }
        DesktopControl::ResourceCleanup => Ok(json!({
            "resourceCleanup": local_resources::cleanup(database)?,
            "localResources": local_resources::status()?,
            "message": "已清理不再使用的本地资源文件。"
        })),
        DesktopControl::AgentCancel { run_id } => {
            let run = agent_runner::cancel(database, &run_id)?;
            Ok(json!({
                "projectId": run.project_id,
                "taskId": run.task_id,
                "agentRunId": run.id,
                "agentRun": run,
                "message": "AI 辅助已取消；项目内容未自动修改。"
            }))
        }
        DesktopControl::TaskRetry { task_id } => {
            let project_id = tasks::project_id(database, &task_id)?;
            let task = tasks::retry(database, &task_id)?;
            Ok(
                json!({"projectId":project_id,"taskId":task.id,"task":task,"message":"任务已重新排队。"}),
            )
        }
        DesktopControl::TaskCancel { task_id } => {
            let project_id = tasks::project_id(database, &task_id)?;
            let task = tasks::cancel(database, &task_id)?;
            Ok(
                json!({"projectId":project_id,"taskId":task.id,"task":task,"message":"任务已取消。"}),
            )
        }
        DesktopControl::VideoCancel { job_id } => {
            let job = video_export::cancel(database, &job_id)?;
            Ok(json!({
                "projectId":job.project_id,
                "jobId":job.id,
                "job":job,
                "message":"已请求取消视频导出。"
            }))
        }
        DesktopControl::TranscriptionConfigure { endpoint, model } => Ok(json!({
            "config": transcription::configure(database, &endpoint, &model)?,
            "message": "MOSS 本机服务配置已保存；不会发送 API 密钥或连接远程地址。"
        })),
    }
}
