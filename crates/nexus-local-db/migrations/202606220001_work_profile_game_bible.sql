-- V1.54 P1: Extend work_profile enum to include 'game_bible'
-- Second non-novel profile — game-bible-profile.md §2
--
-- SQLite does not support ALTER COLUMN to modify CHECK constraints,
-- so we recreate the table with the updated constraint. All existing
-- indexes are recreated after the rename.

-- Step 1: Create a new table with the expanded CHECK constraint
CREATE TABLE works_new (
    work_id TEXT PRIMARY KEY,
    creator_id TEXT NOT NULL,
    workspace_slug TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'active', 'paused', 'completed', 'archived')),
    title TEXT NOT NULL,
    long_term_goal TEXT NOT NULL,
    initial_idea TEXT NOT NULL,
    creative_brief TEXT,
    intake_status TEXT NOT NULL CHECK (intake_status IN ('pending', 'in_progress', 'complete', 'skipped')),
    world_id TEXT,
    story_ref TEXT,
    inspiration_log TEXT NOT NULL DEFAULT '[]',
    primary_preset_id TEXT NOT NULL DEFAULT 'novel-writing',
    schedule_ids TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    work_profile TEXT DEFAULT NULL
        CHECK (work_profile IS NULL OR work_profile IN ('novel', 'essay', 'game_bible')),
    work_ref TEXT DEFAULT NULL,
    total_planned_chapters INTEGER DEFAULT NULL,
    current_chapter INTEGER NOT NULL DEFAULT 0,
    current_stage TEXT,
    stage_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (stage_status IN ('pending','active','in_progress','complete','skipped','failed')),
    auto_chain_enabled INTEGER NOT NULL DEFAULT 0,
    driver_schedule_id TEXT,
    auto_chain_interrupted INTEGER NOT NULL DEFAULT 0,
    auto_review_master_on_timeout INTEGER NOT NULL DEFAULT 0,
    auto_chronology BOOLEAN NOT NULL DEFAULT 0,
    novel_completion_status TEXT,
    completion_locked_at TEXT,
    runtime_lock_holder TEXT,
    runtime_lock_acquired_at TEXT,
    lineage_from_work_id TEXT,
    schedule_json TEXT
);

-- Step 2: Copy data from old table to new table
INSERT INTO works_new SELECT * FROM works;

-- Step 3: Drop old table
DROP TABLE works;

-- Step 4: Rename new table to original name
ALTER TABLE works_new RENAME TO works;

-- Step 5: Recreate all indexes (preserving original names)
CREATE INDEX IF NOT EXISTS idx_works_creator_status
    ON works (creator_id, workspace_slug, status);
CREATE INDEX IF NOT EXISTS idx_works_creator_intake
    ON works (creator_id, workspace_slug, intake_status);
CREATE INDEX IF NOT EXISTS idx_works_creator_updated
    ON works (creator_id, workspace_slug, updated_at DESC);
CREATE INDEX IF NOT EXISTS works_auto_chain_resume
    ON works (auto_chain_enabled, auto_chain_interrupted, status);
CREATE INDEX IF NOT EXISTS idx_works_schedule_json
    ON works (schedule_json);
CREATE INDEX IF NOT EXISTS idx_works_schedule_json_nonempty
    ON works (schedule_json) WHERE schedule_json IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_works_creator_id
    ON works (creator_id);
CREATE INDEX IF NOT EXISTS idx_works_world_id
    ON works (world_id);
CREATE INDEX IF NOT EXISTS idx_works_status
    ON works (status);
CREATE UNIQUE INDEX IF NOT EXISTS idx_works_unique_story_ref
    ON works (creator_id, story_ref)
    WHERE story_ref IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_works_work_ref
    ON works (work_ref);
CREATE INDEX IF NOT EXISTS idx_works_creator_work_ref
    ON works (creator_id, work_ref)
    WHERE work_ref IS NOT NULL;
