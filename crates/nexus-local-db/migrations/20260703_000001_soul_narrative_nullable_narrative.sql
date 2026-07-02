-- V1.81 G3: make memory_soul_narratives.narrative and generated_at
-- nullable so stats-only rows (fingerprint + distinct keyword count)
-- can be persisted for above-gate ungenerated creators, avoiding
-- keyword JSON re-scan on every poll.
--
-- Before: narrative TEXT NOT NULL, generated_at TEXT NOT NULL
-- After:  narrative TEXT,         generated_at TEXT
--
-- SQLite does not support ALTER COLUMN, so we recreate the table.

CREATE TABLE memory_soul_narratives_new (
    creator_id TEXT NOT NULL PRIMARY KEY,
    narrative TEXT,
    generated_at TEXT,
    fragment_count_at_generation INTEGER NOT NULL DEFAULT 0,
    max_fragment_created_at_at_generation TEXT,
    distinct_keyword_count_cache INTEGER NOT NULL DEFAULT 0,
    stats_fingerprint TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO memory_soul_narratives_new
    SELECT * FROM memory_soul_narratives;

DROP TABLE memory_soul_narratives;

ALTER TABLE memory_soul_narratives_new RENAME TO memory_soul_narratives;
