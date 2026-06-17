-- V1.41 multi-work lifecycle columns + novel_pool_entries (DF-60, DF-61 P0 minimal)
-- Spec: novel-writing/multi-work-lifecycle.md, novel-writing/work-pool.md, local-db-schema.md §4.1.4–§4.1.5

-- ─── works table: additive columns ─────────────────────────────────────

ALTER TABLE works ADD COLUMN runtime_lock_holder TEXT;
ALTER TABLE works ADD COLUMN runtime_lock_acquired_at TEXT;
ALTER TABLE works ADD COLUMN completion_locked_at TEXT;
ALTER TABLE works ADD COLUMN novel_completion_status TEXT
    CHECK (novel_completion_status IS NULL
        OR novel_completion_status IN ('finalize_complete', 'reopened'));
ALTER TABLE works ADD COLUMN lineage_from_work_id TEXT
    REFERENCES works(work_id) ON DELETE SET NULL;

-- ─── novel_pool_entries table (P0 minimal) ─────────────────────────────

CREATE TABLE IF NOT EXISTS novel_pool_entries (
    entry_id  TEXT PRIMARY KEY,
    creator_id TEXT NOT NULL,
    work_id   TEXT REFERENCES works(work_id) ON DELETE CASCADE,
    status    TEXT NOT NULL CHECK (status IN ('active', 'queued', 'completed', 'archived')),
    promoted_at TEXT NOT NULL,
    note      TEXT,
    UNIQUE (creator_id, work_id)
);

-- One active entry per creator (partial unique index)
CREATE UNIQUE INDEX IF NOT EXISTS novel_pool_entries_one_active_per_creator
    ON novel_pool_entries(creator_id) WHERE status = 'active';
