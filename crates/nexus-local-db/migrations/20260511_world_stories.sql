-- World-story association table for novel-writing preset.
--
-- Maps story references (directories under Stories/) to their parent world
-- and tracks chapter metadata for sync module consumption.

CREATE TABLE IF NOT EXISTS world_stories (
    id TEXT PRIMARY KEY,
    world_id TEXT NOT NULL,
    story_ref TEXT NOT NULL,
    workspace_path TEXT NOT NULL,
    chapter_count INTEGER NOT NULL DEFAULT 0,
    first_chapter_id TEXT,
    latest_chapter_id TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(world_id, story_ref)
);

CREATE INDEX IF NOT EXISTS idx_world_stories_world_id ON world_stories (world_id);
