-- v1.242 IR P4/P5 Task 1: the current-apply lookup gets the indexes its anti-join reads.
--
-- Plan: .mstar/plans/ride-along-apply-ir-hardening.md §Task 1 (I-000439).
--
-- `strategy_applies` carries no current pointer, so every apply and every current-row probe reads
-- the same shape: the strategy's rows whose `apply_id` no other row supersedes. That is two
-- unindexed scans per read — one over the strategy's rows, one over the whole table for each
-- candidate's successor test.
--
-- Two indexes carry it:
--
--   * `(strategy_id, applied_at DESC, apply_id DESC)` serves both the strategy filter and the
--     total order the reads use to pick a winner (`applied_at DESC, apply_id DESC`), so the pick
--     is an index range plus LIMIT 1 rather than a sort of every row.
--   * `(supersedes_apply_id)` serves the correlated `NOT EXISTS`, which is the anti-join's inner
--     side.
--
-- This is a separate migration, not an edit to 20261114000001_strategy_applies.sql: that file is
-- already applied on databases in the field (schema 36 is on `main`), and SQLx verifies a
-- migration's checksum once it is recorded — editing it in place would fail the next boot.

CREATE INDEX IF NOT EXISTS idx_strategy_applies_strategy_current
    ON strategy_applies (strategy_id, applied_at DESC, apply_id DESC);

CREATE INDEX IF NOT EXISTS idx_strategy_applies_supersedes
    ON strategy_applies (supersedes_apply_id);
