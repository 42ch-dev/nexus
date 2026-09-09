-- Workflow Chain Origin (v1.186 P2 T3, A3)
-- Design: `.mstar/iterations/v1.186/guides/architecture-decisions.md` A3
--
-- Append-only migration. Adds internal provenance for idempotent auto-chain:
-- `source_run_id` records the terminal source run (session id) that created a
-- child schedule. It is internal provenance, not a new public API.
--
-- The partial unique index makes duplicate terminal callbacks and restart
-- reconciliation load the already-created child instead of minting a second
-- schedule. `source_run_id` is NULL for rows created outside the terminal
-- settlement path (public adds, cron, boot-recovery auto-chain).

ALTER TABLE creator_schedules ADD COLUMN source_run_id TEXT;
CREATE UNIQUE INDEX creator_schedules_by_source_run
  ON creator_schedules(source_run_id)
  WHERE source_run_id IS NOT NULL;
