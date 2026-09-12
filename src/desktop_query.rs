//! Read-only desktop application queries shared with the domain services used by the CLI.
use crate::{
    agent_runner, audio_analysis, auto_workflow, cuts, local_resources, models, project,
    resource_jobs, runtime, source_import, speaker, subtitle_import, transcription, video_export,
};
use anyhow::Result;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

#[derive(Debug, Deserialize, ts_rs::TS)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DesktopQuery {
    Models {
        verify: bool,
    },
    ModelJobs,
    ModelJob {
        job_id: String,
    },
    SourceJobs,
    SourceJob {
        job_id: String,
    },
    AutoWorkflows,
    AutoWorkflow {
        workflow_id: String,
    },
    AudioLatest {
        project_id: String,
    },
    AudioJob {
        job_id: String,
    },
    SpeakerPackage {
        verify: bool,
    },
    SpeakerJobs,
    SpeakerJob {
        job_id: String,
    },
    SpeakerTrack {
        project_id: String,
    },
    TranscriptionHealth,
    LatestTranscription {
        project_id: String,
    },
    TranscriptionReviews {
        project_id: String,
    },
    VideoExports {
        project_id: String,
    },
    VideoExport {
        job_id: String,
    },
    AgentHealth,
    AgentRuns {
        project_id: Option<String>,
    },
    AgentRun {
        run_id: String,
    },
    ResourceStatus,
    ResourcePlan {
        capability: String,
        profile: Option<String>,
    },
    ResourceJob {
        job_id: String,
    },
    ResourceJobs,
    Runtime,
    DeletePreflight {
        project_id: String,
    },
    TranscriptReplacementPreflight {
        project_id: String,
    },
    InspectSubtitle {
        project_id: String,
        path: String,
    },
    PreviewCut {
        project_id: String,
        edit_id: String,
    },
}

pub fn execute(db: &Connection, query: DesktopQuery) -> Result<Value> {
    Ok(match query {
        DesktopQuery::Models { verify } => json!({"models":models::catalog(verify)?}),
        DesktopQuery::ModelJobs => json!({"modelJobs":models::list_jobs(db)?}),
        DesktopQuery::ModelJob { job_id } => json!({"modelJob":models::load_job(db,&job_id)?}),
        DesktopQuery::SourceJobs => json!({"sourceJobs":source_import::list(db)?}),
        DesktopQuery::SourceJob { job_id } => json!({"sourceJob":source_import::load(db,&job_id)?}),
        DesktopQuery::AutoWorkflows => json!({"workflows":auto_workflow::list(db)?}),
        DesktopQuery::AutoWorkflow { workflow_id } => {
            json!({"workflow":auto_workflow::load(db,&workflow_id)?,"events":auto_workflow::events(db,&workflow_id,0)?})
        }
        DesktopQuery::AudioLatest { project_id } => {
            json!({"audioAnalysisJob":audio_analysis::latest(db,&project_id)?})
        }
        DesktopQuery::AudioJob { job_id } => {
            json!({"audioAnalysisJob":audio_analysis::load(db,&job_id)?})
        }
        DesktopQuery::SpeakerPackage { verify } => {
            json!({"speakerPackage":speaker::package_status(verify)?})
        }
        DesktopQuery::SpeakerJobs => json!({"speakerJobs":speaker::list_jobs(db)?}),
        DesktopQuery::SpeakerJob { job_id } => json!({"speakerJob":speaker::load_job(db,&job_id)?}),
        DesktopQuery::SpeakerTrack { project_id } => {
            json!({"speakerTrack":speaker::load_track(db,&project_id)?})
        }
        DesktopQuery::TranscriptionHealth => json!({"providerHealth":transcription::health(db)?}),
        DesktopQuery::LatestTranscription { project_id } => {
            json!({"transcriptionJob":transcription::latest(db,&project_id)?})
        }
        DesktopQuery::TranscriptionReviews { project_id } => {
            json!({"reviewItems":transcription::review_items(db,&project_id,true)?})
        }
        DesktopQuery::VideoExports { project_id } => {
            json!({"jobs":video_export::for_project(db,&project_id)?})
        }
        DesktopQuery::VideoExport { job_id } => json!({"job":video_export::load(db,&job_id)?}),
        DesktopQuery::AgentHealth => json!({"codex":agent_runner::health()}),
        DesktopQuery::AgentRuns { project_id } => {
            json!({"agentRuns":agent_runner::list(db,project_id.as_deref())?})
        }
        DesktopQuery::AgentRun { run_id } => json!({"agentRun":agent_runner::load(db,&run_id)?}),
        DesktopQuery::ResourceStatus => json!({"localResources":local_resources::status()?}),
        DesktopQuery::ResourcePlan {
            capability,
            profile,
        } => json!({"resourcePlan":local_resources::plan(&capability,profile.as_deref())?}),
        DesktopQuery::ResourceJob { job_id } => {
            json!({"resourceJob":resource_jobs::load_job(db,&job_id)?})
        }
        DesktopQuery::ResourceJobs => json!({"resourceJobs":resource_jobs::list_jobs(db)?}),
        DesktopQuery::Runtime => json!({"runtime":runtime::status()?}),
        DesktopQuery::DeletePreflight { project_id } => {
            json!({"deletionPreflight":project::deletion_preflight(db,&project_id)?})
        }
        DesktopQuery::TranscriptReplacementPreflight { project_id } => {
            json!({"transcriptReplacementPreflight":project::transcript_replacement_preflight(db,&project_id)?})
        }
        DesktopQuery::InspectSubtitle { project_id, path } => {
            json!({"subtitleImportPreview":subtitle_import::inspect_file(db,&project_id,Path::new(&path))?})
        }
        DesktopQuery::PreviewCut {
            project_id,
            edit_id,
        } => json!({"preview":cuts::preview(db,&project_id,&edit_id)?}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_and_task_queries_leave_project_data_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_at(&temp.path().join("query.db")).unwrap();
        let media = temp.path().join("audio.wav");
        std::fs::write(&media, b"test audio").unwrap();
        let project = project::create(&mut db, &media, Some("Query".into())).unwrap();
        let baseline = db.total_changes();
        for query in [
            DesktopQuery::ModelJobs,
            DesktopQuery::SourceJobs,
            DesktopQuery::AutoWorkflows,
            DesktopQuery::SpeakerJobs,
            DesktopQuery::ResourceJobs,
            DesktopQuery::AgentRuns {
                project_id: Some(project.id.clone()),
            },
            DesktopQuery::VideoExports {
                project_id: project.id.clone(),
            },
            DesktopQuery::AudioLatest {
                project_id: project.id.clone(),
            },
            DesktopQuery::LatestTranscription {
                project_id: project.id.clone(),
            },
            DesktopQuery::TranscriptionReviews {
                project_id: project.id.clone(),
            },
            DesktopQuery::SpeakerTrack {
                project_id: project.id.clone(),
            },
            DesktopQuery::DeletePreflight {
                project_id: project.id.clone(),
            },
            DesktopQuery::TranscriptReplacementPreflight {
                project_id: project.id.clone(),
            },
        ] {
            let result = execute(&db, query).unwrap();
            assert!(!result.as_object().unwrap().is_empty());
        }
        assert_eq!(db.total_changes(), baseline);
        assert!(
            serde_json::from_value::<DesktopQuery>(
                json!({"action":"video_exports","projectId":project.id,"extra":"ignored?"})
            )
            .is_err()
        );
    }
}
