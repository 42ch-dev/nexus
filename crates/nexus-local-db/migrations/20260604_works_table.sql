-- V1.33: Work entity (per work-experience-model §3.2)
CREATE TABLE IF NOT EXISTS works (
    work_id          TEXT PRIMARY KEY,
    creator_id       TEXT NOT NULL,
    workspace_slug   TEXT NOT NULL,
    status           TEXT NOT NULL
                      CHECK (status IN ('draft','active','paused','completed','archived')),
    title            TEXT NOT NULL,
    long_term_goal   TEXT NOT NULL,
    initial_idea     TEXT NOT NULL,
    creative_brief   TEXT,                  -- JSON text, nullable until intake complete
    intake_status    TEXT NOT NULL
                      CHECK (intake_status IN ('pending','in_progress','complete','skipped')),
    world_id         TEXT,                  -- nullable
    story_ref        TEXT,                  -- nullable
    inspiration_log  TEXT NOT NULL DEFAULT '[]',  -- JSON text, append-only
    primary_preset_id TEXT NOT NULL DEFAULT 'novel-writing',
    schedule_ids     TEXT NOT NULL DEFAULT '[]',  -- JSON text
    created_at       TEXT NOT NULL,         -- ISO 8601
    updated_at       TEXT NOT NULL          -- ISO 8601
);

CREATE INDEX IF NOT EXISTS idx_works_creator_status
    ON works (creator_id, workspace_slug, status);

CREATE INDEX IF NOT EXISTS idx_works_creator_intake
    ON works (creator_id, workspace_slug, intake_status);

CREATE INDEX IF NOT EXISTS idx_works_creator_updated
    ON works (creator_id, workspace_slug, updated_at DESC);
