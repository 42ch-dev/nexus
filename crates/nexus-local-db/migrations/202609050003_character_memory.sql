-- v1.184 P3 Task 1: Character SOUL/Memory bearer storage.
-- Spec: .mstar/iterations/v1.184/specs/character-memory.md §2, §5.
--
-- Four dedicated character_* families mirror the Creator memory pipeline
-- tables. Child rows key character_id only; authorization derives the owner
-- from characters — no drifting owner_creator_id duplicate. Existing
-- soul_meta / memory_pending_review / memory_fragments /
-- memory_soul_narratives remain Creator-only and unchanged.
--
-- Same-Character binding provenance is enforced structurally: every
-- binding-provenance column pairs with character_id in a composite FK onto
-- actor_world_bindings (binding_id, character_id). All FK deletes are
-- RESTRICT so Characters and bindings cannot vanish under memory rows.

-- Composite unique target required by the provenance FKs below (binding_id
-- alone is the PK; SQLite needs a UNIQUE covering (binding_id, character_id)
-- to reference the pair).
CREATE UNIQUE INDEX IF NOT EXISTS idx_actor_world_bindings_id_character
    ON actor_world_bindings (binding_id, character_id);

-- ── character_soul_meta ──────────────────────────────────────────────────
-- Per-Character SOUL.md metadata for fast lookups without file I/O.

CREATE TABLE IF NOT EXISTS character_soul_meta (
    character_id TEXT NOT NULL PRIMARY KEY
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    file_path TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    personality_hash TEXT,
    experience_hash TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- ── character_memory_pending_review ─────────────────────────────────────
-- Session-end capture queue rows. actor_world_binding_id NULL = shared
-- Character scope; non-NULL = binding-local (one World life) provenance.

CREATE TABLE IF NOT EXISTS character_memory_pending_review (
    pending_id TEXT NOT NULL PRIMARY KEY,
    session_id TEXT NOT NULL,
    character_id TEXT NOT NULL
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    actor_world_binding_id TEXT,
    task_kind TEXT NOT NULL DEFAULT 'unknown',
    raw_digest TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (actor_world_binding_id, character_id)
        REFERENCES actor_world_bindings (binding_id, character_id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_character_pending_review_character_session
    ON character_memory_pending_review (character_id, session_id);

CREATE INDEX IF NOT EXISTS idx_character_pending_review_character
    ON character_memory_pending_review (character_id);

CREATE INDEX IF NOT EXISTS idx_character_pending_review_binding
    ON character_memory_pending_review (actor_world_binding_id);

-- ── character_memory_fragments ───────────────────────────────────────────
-- Review-promoted keyword fragments. revision backs the OCC check on
-- explicit local→shared promotion (clears actor_world_binding_id in place,
-- preserving fragment_id).

CREATE TABLE IF NOT EXISTS character_memory_fragments (
    fragment_id TEXT NOT NULL PRIMARY KEY,
    session_id TEXT NOT NULL,
    character_id TEXT NOT NULL
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    actor_world_binding_id TEXT,
    keywords TEXT NOT NULL DEFAULT '[]',
    summary TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    ttl TEXT,
    revision INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (actor_world_binding_id, character_id)
        REFERENCES actor_world_bindings (binding_id, character_id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_character_memory_fragments_character_scope_created
    ON character_memory_fragments (character_id, actor_world_binding_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_character_memory_fragments_binding
    ON character_memory_fragments (actor_world_binding_id);

-- ── character_soul_narratives ────────────────────────────────────────────
-- On-demand narrative cache per (character_id, actor_world_binding_id)
-- scope; NULL binding = shared Character scope. Staleness fingerprints and
-- fragment counts are computed inside the same bearer/binding scope.

CREATE TABLE IF NOT EXISTS character_soul_narratives (
    character_id TEXT NOT NULL
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    actor_world_binding_id TEXT,
    narrative TEXT,
    generated_at TEXT,
    fragment_count_at_generation INTEGER NOT NULL DEFAULT 0,
    max_fragment_created_at_at_generation TEXT,
    distinct_keyword_count_cache INTEGER NOT NULL DEFAULT 0,
    stats_fingerprint TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (character_id, actor_world_binding_id),
    FOREIGN KEY (actor_world_binding_id, character_id)
        REFERENCES actor_world_bindings (binding_id, character_id) ON DELETE RESTRICT
);

-- SQLite quirk mitigation: the composite PRIMARY KEY treats NULL bindings as
-- distinct. A partial UNIQUE index guarantees exactly one shared-scope
-- (NULL binding) cache row per Character.
CREATE UNIQUE INDEX IF NOT EXISTS idx_character_soul_narratives_character_only
    ON character_soul_narratives (character_id) WHERE actor_world_binding_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_character_soul_narratives_binding
    ON character_soul_narratives (actor_world_binding_id);
