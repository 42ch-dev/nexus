-- V1.48 P3 T1 — composite index to support resolved-finding retention pruning.
--
-- Implements archived/knowledge/novel-findings-maturity.md §5.1 (V1.48 Draft): the prune query
-- (`prune_resolved_findings_older_than`) filters on
--   `status = 'resolved' AND updated_at < ?`
-- which was not covered by any existing index:
--   - idx_findings_work_status         (work_id, status)
--   - idx_findings_creator_status      (creator_id, status)
--   - idx_findings_work_chapter_status (work_id, chapter, status)
--
-- The new composite index lets the prune DELETE seek directly by
-- (status, updated_at) without scanning the full table.
--
-- Retention clock: `updated_at` (epoch seconds) is used as the proxy for
-- "when the finding was resolved" — `update_finding` sets it on every
-- transition, including status → 'resolved'. No new `resolved_at` column is
-- introduced (local DB schema only; no wire-schema change).
--
-- Scope note: this index covers the resolved-prune path. `open` and
-- `wont_fix` rows are never purged by the V1.48 P3 DAO, so no index is
-- added for those statuses.

CREATE INDEX IF NOT EXISTS idx_findings_status_updated_at
    ON findings (status, updated_at);
