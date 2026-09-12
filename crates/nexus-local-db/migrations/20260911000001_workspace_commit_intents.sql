-- v1.188 P3: Durable workspace commit intents (recoverable multi-file commit).
-- File bytes and secrets never enter this table — only hashes and stage metadata.

CREATE TABLE IF NOT EXISTS workspace_commit_intents (
    session_id TEXT NOT NULL,
    workspace_root TEXT NOT NULL,
    revision TEXT NOT NULL PRIMARY KEY CHECK (revision LIKE 'rev_%'),
    request_digest TEXT NOT NULL,
    state TEXT NOT NULL CHECK (
        state IN ('applying', 'rolling_back', 'committed', 'rolled_back', 'recovery_conflict')
    ),
    entries_json TEXT NOT NULL,
    error_category TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_commit_intents_session_digest
    ON workspace_commit_intents(session_id, request_digest);

CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_commit_intents_unsettled_root
    ON workspace_commit_intents(workspace_root)
    WHERE state IN ('applying', 'rolling_back', 'recovery_conflict');

ALTER TABLE workspace_sessions ADD COLUMN claimed_by_revision TEXT;
