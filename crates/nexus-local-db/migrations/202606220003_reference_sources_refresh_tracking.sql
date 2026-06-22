-- V1.58 P1: reference body refresh tracking (DF-44).
-- Adds refresh lifecycle columns to reference_sources for the DF-44
-- refreshable scan pipeline (P1 ships capability + DB migration;
-- P3 ships CLI subcommand + cross-cut E2E tests).
--
-- Columns:
--   last_refreshed_at  ISO-8601 timestamp of last successful refresh (nullable)
--   refresh_policy     Enum: on_change | scheduled | offline (default offline)
--   refresh_status     Enum: fresh | stale | refreshing | error (nullable)

ALTER TABLE reference_sources ADD COLUMN last_refreshed_at TEXT;
ALTER TABLE reference_sources ADD COLUMN refresh_policy TEXT NOT NULL DEFAULT 'offline';
ALTER TABLE reference_sources ADD COLUMN refresh_status TEXT;

-- Partial index: only non-offline sources participate in refresh scheduling.
-- The refresh-scheduler daemon hook queries this index to find candidates.
CREATE INDEX IF NOT EXISTS idx_reference_sources_refresh_policy
    ON reference_sources(refresh_policy)
    WHERE refresh_policy != 'offline';

-- Index on refresh_status for quick filtering of stale/error sources.
CREATE INDEX IF NOT EXISTS idx_reference_sources_refresh_status
    ON reference_sources(refresh_status);
