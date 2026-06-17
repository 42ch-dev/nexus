-- V1.41 P1: covering indexes for pool + inspiration list queries
-- Spec: novel-writing/work-pool.md §2.2, local-db-schema.md §4.1.5
-- Both list_pool_entries and list_inspiration filter by (creator_id, status)
-- with ORDER BY updated_at / created_at DESC + LIMIT/OFFSET.

CREATE INDEX IF NOT EXISTS novel_pool_entries_by_creator_status
    ON novel_pool_entries(creator_id, status, updated_at DESC);

CREATE INDEX IF NOT EXISTS inspiration_items_by_creator_status
    ON inspiration_items(creator_id, status, created_at DESC);
