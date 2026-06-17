-- V1.50 T-A P1 (S-001) — partial index on `works.schedule_json`.
-- Spec: .mstar/knowledge/specs/novel-writing/cron-staggering.md §4.1.
--
-- The daemon-side cron evaluator runs on a 1-min tick and scans every Work
-- whose `schedule_json` is non-empty (the per-Work cron config blob added in
-- 202606180001). Without an index this is a full-table scan on every tick,
-- which grows linearly with the workspace Work count.
--
-- This partial index keeps the scan bounded to the subset of Works that
-- actually have a cron config. The `WHERE schedule_json IS NOT NULL` clause
-- excludes both NULL (column default — most Works) and the empty-string reset
-- sentinel (`creator works cron set <ref>` with no flags writes the full
-- defaults blob, so empty string is rare; NULL is the common unset state).
-- An empty string is technically non-NULL, so the second predicate
-- (`schedule_json != ''`) catches the explicit reset-to-empty edge case.
--
-- ## EXPLAIN QUERY PLAN verification
--
-- A hermetic test in
-- `crates/nexus-orchestration/tests/cron_supervisor.rs` asserts that the scan
-- query plan uses this index via `EXPLAIN QUERY PLAN` (acceptance criterion
-- §6). The DAO `list_works_with_schedule_json` issues the matching predicate.

CREATE INDEX IF NOT EXISTS idx_works_schedule_json_nonempty
    ON works (schedule_json)
    WHERE schedule_json IS NOT NULL AND schedule_json != '';
