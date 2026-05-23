-- V1.26 World KB persistence: key blocks and source anchors.
--
-- Workspace-local projections for KbStore read/write paths.
-- Domain semantics owned by nexus-kb; migration/storage
-- mechanics owned by nexus-local-db.

CREATE TABLE IF NOT EXISTS kb_key_blocks (
    key_block_id TEXT PRIMARY KEY CHECK (key_block_id LIKE 'kb_%'),
    world_id TEXT NOT NULL,
    block_type TEXT NOT NULL,
    canonical_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'provisional'
        CHECK (status IN ('provisional', 'confirmed', 'deprecated', 'merged', 'deleted')),
    revision INTEGER,
    body_json TEXT,
    source_anchor_json TEXT,
    created_from_command_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT,
    FOREIGN KEY (world_id) REFERENCES narrative_worlds (world_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_id
    ON kb_key_blocks (world_id);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_status
    ON kb_key_blocks (world_id, status);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_type
    ON kb_key_blocks (world_id, block_type);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_world_canonical_name
    ON kb_key_blocks (world_id, canonical_name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_kb_key_blocks_active_unique
    ON kb_key_blocks (world_id, block_type, canonical_name)
    WHERE status NOT IN ('deleted', 'merged', 'deprecated');

CREATE TABLE IF NOT EXISTS kb_source_anchors (
    key_block_id TEXT NOT NULL,
    anchor_ordinal INTEGER NOT NULL,
    source_anchor_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (key_block_id, anchor_ordinal),
    FOREIGN KEY (key_block_id) REFERENCES kb_key_blocks (key_block_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_kb_source_anchors_key_block_id
    ON kb_source_anchors (key_block_id);
