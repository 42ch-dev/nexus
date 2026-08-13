-- DR-08: Drop legacy daemon `outbox` table.
-- Zero active Rust consumers since V1.159 T3 (confirmed by audit).
-- The cloud-sync `outbox_entries` table (behind `legacy-sync` feature flag)
-- is a separate schema and is NOT affected by this migration.
-- See .mstar/specs/outbox-consolidation.md §6.
DROP TABLE IF EXISTS outbox;
