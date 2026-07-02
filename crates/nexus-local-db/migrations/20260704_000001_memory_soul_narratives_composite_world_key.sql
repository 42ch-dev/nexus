-- V1.82: per-World SOUL narrative composite key.
--
-- Changes the `memory_soul_narratives` cache key from `creator_id` alone to
-- the composite `(creator_id, world_id)`, where `world_id IS NULL` represents
-- the Creator-level (whole) narrative and a non-NULL `world_id` represents a
-- per-World narrative for that world's fragment subset.
--
-- The recreation preserves all V1.81 stats-cache columns. Existing V1.81 rows
-- are copied as `world_id = NULL`, so each creator's existing whole-Creator
-- narrative survives unchanged.

CREATE TABLE memory_soul_narratives_new (
    creator_id TEXT NOT NULL,
    world_id   TEXT,                    -- NULL = Creator-level (whole) narrative
    narrative  TEXT,
    generated_at TEXT,
    fragment_count_at_generation INTEGER NOT NULL DEFAULT 0,
    max_fragment_created_at_at_generation TEXT,
    distinct_keyword_count_cache INTEGER NOT NULL DEFAULT 0,
    stats_fingerprint TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (creator_id, world_id)
);

-- SQLite quirk mitigation: PRIMARY KEY does NOT enforce uniqueness on NULL
-- world_id rows (SQLite treats NULL as distinct in PK/UNIQUE). A partial UNIQUE
-- index guarantees exactly one Creator-level (NULL world_id) row per creator.
CREATE UNIQUE INDEX idx_memory_soul_narratives_creator_only
    ON memory_soul_narratives_new (creator_id) WHERE world_id IS NULL;

-- By-name column-list copy (V1.81 greploop lesson: never positional SELECT *).
-- Existing rows are all Creator-level → world_id defaults to NULL.
INSERT INTO memory_soul_narratives_new (
    creator_id, world_id, narrative, generated_at,
    fragment_count_at_generation, max_fragment_created_at_at_generation,
    distinct_keyword_count_cache, stats_fingerprint, created_at, updated_at
)
SELECT
    creator_id, NULL, narrative, generated_at,
    fragment_count_at_generation, max_fragment_created_at_at_generation,
    distinct_keyword_count_cache, stats_fingerprint, created_at, updated_at
FROM memory_soul_narratives;

DROP TABLE memory_soul_narratives;

ALTER TABLE memory_soul_narratives_new RENAME TO memory_soul_narratives;
