-- V1.145 P3 T1 — additive migration for TimelineEvent extensions.nexus round-trip.
-- Spec: spoke-adapter-architecture.md §7.4 (timeline storage reuse).
--
-- Mirrors the V1.139 P1 T4 migration that added extensions_nexus_json to
-- kb_key_blocks. The narrative_timeline_events table retains its existing
-- typed columns (world_id, branch_id, event_type, status, sequence_no, ...)
-- which the V1.143 TimelineEvent<->spoke TimelineEvent conversion seam packs
-- into the spoke type's extensions.nexus. This nullable TEXT column carries
-- the full serialized extensions.nexus namespace so unknown keys round-trip
-- when a spoke TimelineEvent transits SQLite in a future write path. Known
-- fields stay authoritative in their typed columns; additive, defaults NULL
-- for legacy rows (no data migration).

ALTER TABLE narrative_timeline_events ADD COLUMN extensions_nexus_json TEXT;
