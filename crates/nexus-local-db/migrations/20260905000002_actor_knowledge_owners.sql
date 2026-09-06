-- no-transaction
-- v1.184 P1 Task 1: canonical KnowledgeEntry owner scopes on kb_key_blocks.
-- (14-digit version: sqlx orders numerically and the July/August rebuilds use
-- 14-digit versions up to 20260815000001; a 12-digit 202609050002 would run
-- BEFORE the pack_import rebuild/modules_json and lose these columns.)
-- Spec: .mstar/iterations/v1.184/specs/actor-knowledge-view.md §2.
--
-- Adds the closed owner union (world | character | actor_world_binding) and
-- the World-only creator_only marker. SQLite cannot ALTER constraints, so the
-- table is rebuilt with the same kb_key_blocks_new swap pattern as
-- 20260731000001_pack_import_provenance.sql.
--
-- Invariants:
-- - The named-column copy preserves all 16 legacy columns verbatim
--   (key_block_id, world_id, block_type, canonical_name, status, revision,
--   body_json, source_anchor_json, created_from_command_id, created_at,
--   updated_at, source_work_id, source_chapter, source_provenance_kind,
--   extensions_nexus_json, modules_json). Existing rows gain only
--   owner_kind='world', NULL non-World owner columns, and creator_only=0
--   (via the column DEFAULTs).
-- - world_id becomes nullable: exactly one owner column is non-NULL per row,
--   enforced by the row CHECK. The 'world' DEFAULT on owner_kind keeps every
--   legacy INSERT shape (5/9/16-column) World-owned without code changes.
-- - Child tables (kb_source_anchors, kb_relationships, mind_states,
--   actor_world_bindings.world_sheet_entry_id) FK by table name and are
--   untouched; foreign_key_check proves the swap at the end.

PRAGMA foreign_keys=OFF;

-- Pre-guard: drop any leftover from a partial earlier run.
DROP TABLE IF EXISTS kb_key_blocks_new;

-- Step 1: new table with owner columns, exactly-one-owner CHECK, and
-- World-only creator_only CHECK. All legacy CHECKs/FKs reproduced verbatim
-- (including the V1.146 'pack_import' provenance variant).
CREATE TABLE kb_key_blocks_new (
    key_block_id TEXT PRIMARY KEY CHECK (key_block_id LIKE 'kb_%'),
    owner_kind TEXT NOT NULL DEFAULT 'world'
        CHECK (owner_kind IN ('world', 'character', 'actor_world_binding')),
    world_id TEXT
        REFERENCES narrative_worlds (world_id) ON DELETE CASCADE,
    character_id TEXT
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    actor_world_binding_id TEXT
        REFERENCES actor_world_bindings (binding_id) ON DELETE RESTRICT,
    creator_only INTEGER NOT NULL DEFAULT 0
        CHECK (creator_only IN (0, 1)),
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
    source_work_id TEXT,
    source_chapter INTEGER,
    source_provenance_kind TEXT
        CHECK (source_provenance_kind IS NULL
            OR source_provenance_kind IN (
                'manual',
                'review_time_extract',
                'finalize_time_extract',
                'cross_chapter_rescan',
                'author_explicit',
                'pack_import'
            )),
    extensions_nexus_json TEXT,
    modules_json TEXT,
    CHECK (
        (
            (owner_kind = 'world'
                AND world_id IS NOT NULL
                AND character_id IS NULL
                AND actor_world_binding_id IS NULL)
            OR (owner_kind = 'character'
                AND world_id IS NULL
                AND character_id IS NOT NULL
                AND actor_world_binding_id IS NULL)
            OR (owner_kind = 'actor_world_binding'
                AND world_id IS NULL
                AND character_id IS NULL
                AND actor_world_binding_id IS NOT NULL)
        )
        AND (creator_only = 0 OR owner_kind = 'world')
    )
);

-- Step 2: named-column copy; legacy rows become World-owned via the
-- owner_kind/creator_only DEFAULTs and NULL non-World owner columns.
INSERT INTO kb_key_blocks_new
    (key_block_id, world_id, block_type, canonical_name, status, revision,
     body_json, source_anchor_json, created_from_command_id, created_at,
     updated_at, source_work_id, source_chapter, source_provenance_kind,
     extensions_nexus_json, modules_json)
SELECT key_block_id, world_id, block_type, canonical_name, status, revision,
       body_json, source_anchor_json, created_from_command_id, created_at,
       updated_at, source_work_id, source_chapter, source_provenance_kind,
       extensions_nexus_json, modules_json
FROM kb_key_blocks;

-- Step 3: drop old table (its indexes drop with it), rename the new one.
DROP TABLE kb_key_blocks;
ALTER TABLE kb_key_blocks_new RENAME TO kb_key_blocks;

-- Step 4: recreate every pre-existing index (original names/SQL preserved).
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

-- Step 5: owner-scope indexes. Active-name uniqueness is per owner column
-- with the existing status predicate; the plain owner indexes serve the
-- RESTRICT FK lookups and owner-scoped listing.
CREATE UNIQUE INDEX IF NOT EXISTS idx_kb_key_blocks_character_active_unique
    ON kb_key_blocks (character_id, block_type, canonical_name)
    WHERE status NOT IN ('deleted', 'merged', 'deprecated');
CREATE UNIQUE INDEX IF NOT EXISTS idx_kb_key_blocks_binding_active_unique
    ON kb_key_blocks (actor_world_binding_id, block_type, canonical_name)
    WHERE status NOT IN ('deleted', 'merged', 'deprecated');
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_character_id
    ON kb_key_blocks (character_id);
CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_actor_world_binding_id
    ON kb_key_blocks (actor_world_binding_id);

PRAGMA foreign_keys=ON;
PRAGMA foreign_key_check;
