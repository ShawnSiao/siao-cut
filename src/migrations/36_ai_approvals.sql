CREATE TABLE ai_send_approvals (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    base_version_id TEXT NOT NULL,
    preview_json TEXT NOT NULL,
    run_id TEXT UNIQUE REFERENCES agent_runs(id),
    created_at TEXT NOT NULL
);
CREATE INDEX ai_send_approvals_project ON ai_send_approvals(project_id);
CREATE TABLE ai_approval_dispatches (
    approval_id TEXT NOT NULL REFERENCES ai_send_approvals(id) ON DELETE CASCADE,
    batch_id TEXT NOT NULL,
    dispatched_at TEXT NOT NULL,
    PRIMARY KEY(approval_id, batch_id)
);

CREATE TABLE auto_workflows_v36 (
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
    status TEXT NOT NULL CHECK(status IN ('queued','running','needs_agent','awaiting_authorization','needs_review','failed','interrupted','cancelled','completed')),
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

INSERT INTO auto_workflows_v36 SELECT * FROM auto_workflows;

CREATE TABLE auto_workflow_events_v36 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workflow_id TEXT NOT NULL REFERENCES auto_workflows_v36(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    status TEXT NOT NULL,
    progress REAL NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL
);
INSERT INTO auto_workflow_events_v36 SELECT * FROM auto_workflow_events;
DROP TABLE auto_workflow_events;
DROP TABLE auto_workflows;
ALTER TABLE auto_workflows_v36 RENAME TO auto_workflows;
ALTER TABLE auto_workflow_events_v36 RENAME TO auto_workflow_events;
CREATE INDEX idx_auto_workflows_status ON auto_workflows(status,created_at);
CREATE INDEX idx_auto_workflows_project ON auto_workflows(project_id);
CREATE INDEX idx_auto_workflow_events_workflow ON auto_workflow_events(workflow_id,id);

-- Previously selected targets did not disclose the eventual transcript.
UPDATE auto_workflows SET ai_authorized=0
WHERE ai_execution_kind IS NOT NULL AND (agent_task_id IS NULL OR agent_task_id NOT IN (SELECT task_id FROM agent_runs));
UPDATE auto_workflows SET status='awaiting_authorization',ai_authorized=0,worker_pid=NULL
WHERE status='needs_agent' AND ai_execution_kind IS NOT NULL
AND (agent_task_id IS NULL OR agent_task_id NOT IN (SELECT task_id FROM agent_runs));
