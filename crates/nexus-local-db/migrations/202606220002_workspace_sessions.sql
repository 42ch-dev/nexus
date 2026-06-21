-- V1.56 P0: Workspace sessions table (DF-31 full)
-- Persists workspace.open/workspace.commit sessions in SQLite,
-- replacing the V1.55 in-memory WorkspaceSessionManager with 
-- durable DB-backed sessions that survive daemon restarts.
--
-- Columns:
--   session_id       — unique session identifier (ws_<uuid>)
--   workspace_root   — absolute path to workspace creative root
--   relative_path    — relative path within the workspace
--   existed          — whether the target path existed at open time
--   file_hashes_json — JSON object of {relative_path: sha256_hex} for all tracked files
--   created_at       — ISO-8601 creation timestamp
--   expires_at       — ISO-8601 expiry timestamp (created_at + TTL)
--   consumed         — 0 = active, 1 = consumed (committed)
CREATE TABLE IF NOT EXISTS workspace_sessions (
    session_id TEXT PRIMARY KEY CHECK (session_id LIKE 'ws_%'),
    workspace_root TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    existed INTEGER NOT NULL DEFAULT 0,
    file_hashes_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    consumed INTEGER NOT NULL DEFAULT 0 CHECK (consumed IN (0, 1))
);

CREATE INDEX IF NOT EXISTS idx_workspace_sessions_expires_at
    ON workspace_sessions (expires_at);

CREATE INDEX IF NOT EXISTS idx_workspace_sessions_consumed_expires
    ON workspace_sessions (consumed, expires_at);
