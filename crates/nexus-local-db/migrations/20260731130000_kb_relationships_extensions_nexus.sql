-- V1.146 P5 T1 — additive migration for kb_relationships extensions.nexus round-trip.
-- Spec: spoke-adapter-architecture.md §2.3 (Q7).
--
-- Mirrors the V1.139 P1 T4 migration that added extensions_nexus_json to
-- kb_key_blocks and the V1.145 P3 T1 migration that added it to
-- narrative_timeline_events. The kb_relationships table retains its existing
-- typed identity columns (world_id, source_entity_id, target_entity_id,
-- relation_type, custom_label, symmetric, confidence, source_anchor_ids,
-- metadata, needs_review, source) for query efficiency and index stability.
-- This migration adds a single nullable TEXT column that carries the full
-- serialized `extensions.nexus` namespace, so unknown keys round-trip when a
-- spoke Relation transits SQLite. Known fields stay authoritative in their
-- typed columns; this column is additive and defaults to NULL for legacy rows
-- (backfilled on next read/write cycle).

ALTER TABLE kb_relationships ADD COLUMN extensions_nexus_json TEXT;
