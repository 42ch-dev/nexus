-- V1.76 Track A: World KB relationship extraction gate + provenance.
-- Adds `needs_review` (the lightweight extraction-suggestion gate) and `source`
-- (manual vs extraction provenance) to kb_relationships.
--
-- `needs_review = 1` marks a relationship as a suggested (not yet author-
-- confirmed) edge produced by extraction. The GET graph defaults to excluding
-- suggested rows (needs_review = 0); `?include_suggested=true` surfaces them.
-- Promotion is clearing the flag through the existing patch-relationship route
-- (no separate promotion state machine). Entity-scope-model §5.6 extraction.
--
-- `source` is read-only provenance: 'manual' (author-created via patch route)
-- or 'extraction' (proposed by nexus.llm.extract). Existing rows default to
-- 'manual' / needs_review = 0 (no visible change for existing data).

ALTER TABLE kb_relationships ADD COLUMN needs_review INTEGER NOT NULL DEFAULT 0;
ALTER TABLE kb_relationships ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'
    CHECK (source IN ('manual', 'extraction'));

-- Graph filter index: default WHERE needs_review = 0 per world.
CREATE INDEX IF NOT EXISTS idx_kb_relationships_world_id_needs_review
    ON kb_relationships(world_id, needs_review);

-- Extraction idempotent-upsert lookup index: keyed on
-- (world_id, source_entity_id, target_entity_id, relation_type, source).
CREATE INDEX IF NOT EXISTS idx_kb_relationships_extraction_upsert
    ON kb_relationships(world_id, source_entity_id, target_entity_id, relation_type, source);
