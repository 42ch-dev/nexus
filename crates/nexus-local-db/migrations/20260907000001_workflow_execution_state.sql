-- Workflow Execution State (v1.186 P0, A2)
-- Design: `.mstar/iterations/v1.186/guides/architecture-decisions.md` A2
--
-- Append-only migration. Adds durable run-state columns to
-- `orchestration_sessions` and schedule cutover columns to
-- `creator_schedules`. Pure `ALTER TABLE ... ADD COLUMN` — no table rebuild,
-- no FK-off window needed.
--
-- `execution_version = 0` is a **legacy evidence marker**, not an
-- alternative authoritative success state. v1 rows are authoritative; v0
-- rows stay explicitly legacy/unverified until a conservative reconciliation
-- can establish a terminal from existing typed evidence or the shipped safe
-- join classifier. Corrupt or ambiguous legacy records remain non-replayable;
-- original blobs are preserved.

ALTER TABLE orchestration_sessions ADD COLUMN execution_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE orchestration_sessions ADD COLUMN state_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE orchestration_sessions ADD COLUMN run_state_json BLOB;
ALTER TABLE orchestration_sessions ADD COLUMN run_descriptor_json BLOB;
ALTER TABLE creator_schedules ADD COLUMN execution_policy TEXT NOT NULL DEFAULT 'legacy_inert';
ALTER TABLE creator_schedules ADD COLUMN execution_descriptor_json BLOB;
