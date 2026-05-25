-- V1.29 KB Extract job queue: tracks work-entry-to-world-KB extraction jobs.
--
-- Each row is an idempotent extraction request: enqueue a work-scope KB entry
-- for structured extraction (via acp.prompt) into a target world's KeyBlocks.
-- SSOT in nexus-local-db; no second in-memory queue.

CREATE TABLE IF NOT EXISTS kb_extract_jobs (
    job_id TEXT PRIMARY KEY CHECK (job_id LIKE 'xj_%'),
    creator_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    work_entry_id TEXT NOT NULL,
    world_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'done', 'failed')),
    error_text TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_kb_extract_jobs_creator
    ON kb_extract_jobs (creator_id);
CREATE INDEX IF NOT EXISTS idx_kb_extract_jobs_status
    ON kb_extract_jobs (status);
CREATE UNIQUE INDEX IF NOT EXISTS idx_kb_extract_jobs_idempotent
    ON kb_extract_jobs (creator_id, work_entry_id, world_id)
    WHERE status NOT IN ('failed');
