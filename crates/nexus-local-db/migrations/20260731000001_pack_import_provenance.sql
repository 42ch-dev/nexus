-- no-transaction
-- V1.146 P3 T3: Expand source_provenance_kind CHECK to include 'pack_import'.
-- Product lock (pack-io-product-behavior.md + plan Interfaces) requires
-- 'pack_import' on import rows, but the T3 import path stamped 'manual' because
-- the CHECK (migration 202606190003) did not include 'pack_import'.
--
-- SQLite does not support ALTER COLUMN to modify CHECK constraints, so we
-- rebuild the table with the updated constraint. Child tables (kb_source_anchors,
-- kb_relationships) FK by table name — not touched. With foreign_keys=OFF during
-- rebuild this is safe.

PRAGMA foreign_keys=OFF;

-- Step 1: Create new table with expanded CHECK
CREATE TABLE kb_key_blocks_new (
    key_block_id              TEXT PRIMARY KEY CHECK (key_block_id LIKE 'kb_%'),
    world_id                  TEXT NOT NULL,
    block_type                TEXT NOT NULL,
    canonical_name            TEXT NOT NULL,
    status                    TEXT NOT NULL DEFAULT 'provisional'
        CHECK (status IN ('provisional', 'confirmed', 'deprecated', 'merged', 'deleted')),
    revision                  INTEGER,
    body_json                 TEXT,
    source_anchor_json        TEXT,
    created_from_command_id   TEXT,
    created_at                TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at                TEXT,
    source_work_id            TEXT,
    source_chapter            INTEGER,
    source_provenance_kind    TEXT
        CHECK (source_provenance_kind IS NULL
            OR source_provenance_kind IN (
                'manual',
                'review_time_extract',
                'finalize_time_extract',
                'cross_chapter_rescan',
                'author_explicit',
                'pack_import'
            )),
    extensions_nexus_json     TEXT,
    FOREIGN KEY (world_id) REFERENCES narrative_worlds (world_id) ON DELETE CASCADE
);

-- Step 2: Copy data with named columns (safe against column-order drift)
INSERT INTO kb_key_blocks_new
    (key_block_id, world_id, block_type, canonical_name, status, revision,
     body_json, source_anchor_json, created_from_command_id, created_at,
     updated_at, source_work_id, source_chapter, source_provenance_kind,
     extensions_nexus_json)
SELECT key_block_id, world_id, block_type, canonical_name, status, revision,
       body_json, source_anchor_json, created_from_command_id, created_at,
       updated_at, source_work_id, source_chapter, source_provenance_kind,
       extensions_nexus_json
FROM kb_key_blocks;

-- Step 3: Drop old table
DROP TABLE kb_key_blocks;

-- Step 4: Rename new table to original name
ALTER TABLE kb_key_blocks_new RENAME TO kb_key_blocks;

-- Step 5: Recreate all indexes (preserving original names)
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
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_source_work_id
    ON kb_key_blocks (source_work_id);

PRAGMA foreign_keys=ON;
PRAGMA foreign_key_check;
