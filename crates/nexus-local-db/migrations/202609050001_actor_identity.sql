-- v1.184 P0: Character bearer + ActorWorldBinding cardinality.
-- Additive/immutable. Existing creator_id columns and WorldSheet ownership stay unchanged.

CREATE TABLE IF NOT EXISTS characters (
    character_id TEXT NOT NULL PRIMARY KEY
        CHECK (
            character_id GLOB 'chr_[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]'
        ),
    owner_creator_id TEXT NOT NULL
        REFERENCES creators (creator_id) ON DELETE RESTRICT,
    display_name TEXT NOT NULL
        CHECK (
            display_name = trim(display_name)
            AND length(display_name) BETWEEN 1 AND 120
        ),
    status TEXT NOT NULL CHECK (status IN ('active', 'archived')),
    image_uri TEXT,
    persona_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_characters_owner_creator_id
    ON characters (owner_creator_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_characters_owner_active_display_name
    ON characters (owner_creator_id, display_name COLLATE NOCASE)
    WHERE status = 'active';

CREATE TABLE IF NOT EXISTS actor_world_bindings (
    binding_id TEXT NOT NULL PRIMARY KEY
        CHECK (
            binding_id GLOB 'awb_[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]'
        ),
    character_id TEXT NOT NULL
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    world_id TEXT NOT NULL
        REFERENCES narrative_worlds (world_id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive')),
    world_sheet_entry_id TEXT
        REFERENCES kb_key_blocks (key_block_id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_actor_world_bindings_character_id
    ON actor_world_bindings (character_id);

CREATE INDEX IF NOT EXISTS idx_actor_world_bindings_world_id
    ON actor_world_bindings (world_id);

CREATE INDEX IF NOT EXISTS idx_actor_world_bindings_world_sheet_entry_id
    ON actor_world_bindings (world_sheet_entry_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_actor_world_bindings_active_unique
    ON actor_world_bindings (character_id, world_id)
    WHERE status = 'active';
