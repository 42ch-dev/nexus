-- V1.147 P0 fix wave — branch snapshot + list index for the direct lane.
--
-- Adds the branch snapshot columns consumed by the run handler / Accept
-- (F-002/F-003): `branch_id` (branch the run was scoped to; defaults to the
-- World root branch at run time) and `timeline_head_event_id` (the world's
-- timeline head at run time). Accept appends timeline events to the
-- SNAPSHOTTED branch, not the current fork head.
--
-- Also adds the composite index for the direct lane's world-scoped lookup
-- shape (S-1): `list_runs` always filters by `world_id` (owned-world scope),
-- so `(world_id, run_id)` narrows the scan to one world. It does NOT back
-- the newest-first list cursor ordering (`ORDER BY created_at DESC,
-- run_id DESC`) — covering that ordering would need
-- `(world_id, created_at DESC, run_id DESC)` and is deferred to the P3 TTL
-- work (qc1 N-2 / qc3 N-2; comment-honesty only, no new migration).

ALTER TABLE compute_sessions ADD COLUMN branch_id TEXT;
ALTER TABLE compute_sessions ADD COLUMN timeline_head_event_id TEXT;

CREATE INDEX IF NOT EXISTS idx_compute_sessions_world_run
    ON compute_sessions (world_id, run_id);
