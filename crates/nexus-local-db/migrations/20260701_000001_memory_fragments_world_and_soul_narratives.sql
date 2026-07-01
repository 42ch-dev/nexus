-- V1.81: per-World data foundation + Creator-SOUL narrative cache.
--
-- Part A: add nullable world_id to memory_fragments so fragments can
-- record which world they emerged from. Existing rows default NULL
-- (Creator-core-only). No FK is added — world_id is a provenance tag,
-- not ownership.
--
-- Part B: memory_soul_narratives caches the on-demand LLM synthesis
-- of the whole Creator SOUL (all fragments, world-agnostic). Stale
-- invalidation keys off fragment_count_at_generation and
-- max_fragment_created_at_at_generation snapshots.

ALTER TABLE memory_fragments ADD COLUMN world_id TEXT;

CREATE INDEX IF NOT EXISTS idx_memory_fragments_creator_world_created
    ON memory_fragments (creator_id, world_id, created_at DESC);

CREATE TABLE IF NOT EXISTS memory_soul_narratives (
    creator_id TEXT NOT NULL PRIMARY KEY,
    narrative TEXT NOT NULL,
    generated_at TEXT NOT NULL,
    fragment_count_at_generation INTEGER NOT NULL,
    max_fragment_created_at_at_generation TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
