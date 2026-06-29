-- V1.74 Track A: World KB typed relationship storage.
-- Adds the kb_relationships table with per-row OCC on revision.

CREATE TABLE IF NOT EXISTS kb_relationships (
    relationship_id TEXT PRIMARY KEY NOT NULL,
    world_id TEXT NOT NULL,
    source_entity_id TEXT NOT NULL,
    target_entity_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    custom_label TEXT,
    symmetric INTEGER NOT NULL DEFAULT 0,
    confidence REAL,
    source_anchor_ids TEXT,
    metadata TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (world_id) REFERENCES narrative_worlds(world_id) ON DELETE CASCADE,
    FOREIGN KEY (source_entity_id) REFERENCES kb_key_blocks(key_block_id) ON DELETE CASCADE,
    FOREIGN KEY (target_entity_id) REFERENCES kb_key_blocks(key_block_id) ON DELETE CASCADE,
    -- Prevent exact-duplicate directed relationships in the same World.
    -- (Symmetric dedup across both directions is enforced by daemon validation;
    -- this constraint guards the storage layer against identical rows.)
    UNIQUE (world_id, source_entity_id, target_entity_id, relation_type)
);

CREATE INDEX IF NOT EXISTS idx_kb_relationships_world_id
    ON kb_relationships(world_id);
CREATE INDEX IF NOT EXISTS idx_kb_relationships_source_entity_id
    ON kb_relationships(source_entity_id);
CREATE INDEX IF NOT EXISTS idx_kb_relationships_target_entity_id
    ON kb_relationships(target_entity_id);
CREATE INDEX IF NOT EXISTS idx_kb_relationships_world_id_relation_type
    ON kb_relationships(world_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_kb_relationships_world_id_source_target
    ON kb_relationships(world_id, source_entity_id, target_entity_id);
