-- V1.41 P1: inspiration_items + novel_pool_entries extensions (DF-61)
-- Spec: novel-work-pool.md, local-db-schema.md §4.1.5

-- ─── Extend novel_pool_entries with title + updated_at ──────────────────
-- P0 migration created the table without display title and updated_at.
-- These are required by novel-work-pool.md §2.2 and P1 list/promote flows.

ALTER TABLE novel_pool_entries ADD COLUMN title TEXT NOT NULL DEFAULT '';
ALTER TABLE novel_pool_entries ADD COLUMN updated_at TEXT NOT NULL;

-- ─── inspiration_items table ───────────────────────────────────────────

CREATE TABLE IF NOT EXISTS inspiration_items (
    item_id  TEXT PRIMARY KEY CHECK (item_id LIKE 'npi_%'),
    creator_id TEXT NOT NULL,
    rel_path  TEXT NOT NULL,
    title     TEXT NOT NULL,
    status    TEXT NOT NULL CHECK (status IN ('idea', 'promoted', 'archived')),
    promoted_work_id TEXT REFERENCES works(work_id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    promoted_at TEXT
);

CREATE INDEX IF NOT EXISTS inspiration_items_by_creator
    ON inspiration_items(creator_id);

CREATE UNIQUE INDEX IF NOT EXISTS inspiration_items_unique_creator_path
    ON inspiration_items(creator_id, rel_path);
