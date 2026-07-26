-- V1.139 P1 T4 — additive migration for extensions.nexus round-trip preservation.
-- Spec: spoke-adapter-architecture.md §2.3 (Q7).
--
-- The kb_key_blocks table retains its existing typed identity columns
-- (world_id, created_from_command_id, source_work_id, source_chapter,
-- source_provenance_kind) for query efficiency and index stability. This
-- migration adds a single nullable TEXT column that carries the full serialized
-- `extensions.nexus` namespace, so unknown keys round-trip when a spoke
-- KnowledgeEntry transits SQLite. Known fields stay authoritative in their
-- typed columns; this column is additive and defaults to NULL for legacy rows
-- (backfilled on next read/write cycle).

ALTER TABLE kb_key_blocks ADD COLUMN extensions_nexus_json TEXT;
