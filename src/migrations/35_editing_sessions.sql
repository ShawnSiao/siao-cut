CREATE TABLE editing_drafts (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    segment_id TEXT NOT NULL,
    field TEXT NOT NULL,
    base_version_id TEXT,
    base_text TEXT NOT NULL,
    text TEXT NOT NULL,
    revision INTEGER NOT NULL,
    discarded INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(project_id, session_id, segment_id, field)
);
CREATE TABLE editing_receipts (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    mutation_id TEXT NOT NULL,
    request_json TEXT NOT NULL,
    response_json TEXT NOT NULL,
    PRIMARY KEY(project_id, mutation_id)
);
ALTER TABLE versions ADD COLUMN edit_group TEXT;
ALTER TABLE versions ADD COLUMN active_history INTEGER NOT NULL DEFAULT 1;
