CREATE TABLE auto_workflows_v32 (
    id TEXT PRIMARY KEY,
    input_kind TEXT NOT NULL CHECK(input_kind IN ('local','url')),
    input_value TEXT NOT NULL,
    title TEXT,
    confirmed_media_id TEXT,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    source_import_id TEXT REFERENCES source_imports(id) ON DELETE SET NULL,
    model_path TEXT NOT NULL,
    transcribe_language TEXT,
    translation_language TEXT,
    output_path TEXT NOT NULL,
    burn_subtitles INTEGER NOT NULL DEFAULT 0,
    subtitle_mode TEXT NOT NULL CHECK(subtitle_mode IN ('source','translated','bilingual')),
    profile TEXT NOT NULL DEFAULT 'balanced' CHECK(profile IN ('draft','balanced','delivery')),
    status TEXT NOT NULL CHECK(status IN ('queued','running','needs_agent','needs_review','failed','interrupted','cancelled','completed')),
    current_stage TEXT NOT NULL CHECK(current_stage IN ('import','transcribe','analyze','suggestions','translate','review','audit','export','complete')),
    progress REAL NOT NULL DEFAULT 0,
    transcript_version_id TEXT REFERENCES versions(id) ON DELETE SET NULL,
    agent_task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    audio_analysis_job_id TEXT REFERENCES audio_analysis_jobs(id) ON DELETE SET NULL,
    export_job_id TEXT REFERENCES export_jobs(id) ON DELETE SET NULL,
    audit_json TEXT,
    cancel_requested_at TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    worker_pid INTEGER,
    attempt_count INTEGER NOT NULL DEFAULT 1,
    instruction_locale TEXT NOT NULL DEFAULT 'zh-CN',
    ai_execution_kind TEXT,
    ai_service_config_id TEXT,
    ai_service_revision INTEGER,
    ai_network_revision INTEGER,
    ai_model_id TEXT,
    ai_authorized INTEGER NOT NULL DEFAULT 0
);

INSERT INTO auto_workflows_v32(
    id,input_kind,input_value,title,confirmed_media_id,project_id,source_import_id,
    model_path,transcribe_language,translation_language,output_path,burn_subtitles,
    subtitle_mode,profile,status,current_stage,progress,transcript_version_id,agent_task_id,
    audio_analysis_job_id,export_job_id,audit_json,cancel_requested_at,error_message,
    created_at,updated_at,completed_at,worker_pid,attempt_count,instruction_locale,
    ai_execution_kind,ai_service_config_id,ai_service_revision,ai_network_revision,
    ai_model_id,ai_authorized
) SELECT
    id,input_kind,input_value,title,confirmed_media_id,project_id,source_import_id,
    model_path,transcribe_language,translation_language,output_path,burn_subtitles,
    subtitle_mode,'balanced',status,current_stage,progress,transcript_version_id,agent_task_id,
    NULL,export_job_id,audit_json,cancel_requested_at,error_message,
    created_at,updated_at,completed_at,worker_pid,attempt_count,instruction_locale,
    ai_execution_kind,ai_service_config_id,ai_service_revision,ai_network_revision,
    ai_model_id,ai_authorized
FROM auto_workflows;

CREATE TABLE auto_workflow_events_v32 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workflow_id TEXT NOT NULL REFERENCES auto_workflows_v32(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    status TEXT NOT NULL,
    progress REAL NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL
);
INSERT INTO auto_workflow_events_v32 SELECT * FROM auto_workflow_events;
DROP TABLE auto_workflow_events;
DROP TABLE auto_workflows;
ALTER TABLE auto_workflows_v32 RENAME TO auto_workflows;
ALTER TABLE auto_workflow_events_v32 RENAME TO auto_workflow_events;
CREATE INDEX idx_auto_workflows_status ON auto_workflows(status,created_at);
CREATE INDEX idx_auto_workflows_project ON auto_workflows(project_id);
CREATE INDEX idx_auto_workflow_events_workflow ON auto_workflow_events(workflow_id,id);
