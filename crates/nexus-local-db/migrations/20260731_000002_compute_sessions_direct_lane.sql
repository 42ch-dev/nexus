-- V1.147 P0 — augment compute_sessions for the direct Control Room lane.
-- Preserves existing columns (session_id, entry_id, state_json, created_at)
-- for spoke adapter path backward compatibility.

ALTER TABLE compute_sessions ADD COLUMN run_id TEXT;
ALTER TABLE compute_sessions ADD COLUMN world_id TEXT;
ALTER TABLE compute_sessions ADD COLUMN module_id TEXT;
ALTER TABLE compute_sessions ADD COLUMN module_version TEXT;
ALTER TABLE compute_sessions ADD COLUMN status TEXT NOT NULL DEFAULT 'running';
ALTER TABLE compute_sessions ADD COLUMN proposals_json TEXT;
ALTER TABLE compute_sessions ADD COLUMN error_json TEXT;
ALTER TABLE compute_sessions ADD COLUMN updated_at TEXT;
ALTER TABLE compute_sessions ADD COLUMN accepted_at TEXT;
ALTER TABLE compute_sessions ADD COLUMN invocation_params_json TEXT;

-- run_id is the primary key for the direct lane (replaces session_id).
-- session_id remains for spoke adapter rows.
CREATE UNIQUE INDEX IF NOT EXISTS idx_compute_sessions_run_id ON compute_sessions(run_id)
    WHERE run_id IS NOT NULL;
