-- V1.89: reading_progress + reading_annotations — persisted reading depth.
--
-- BL-11 MVP slice (web-ui.md §28): persisted per-(creator, work, chapter) scroll
-- progress plus character-offset annotations/highlights with optional notes.
-- Reading surface remains read-only for the body (canvas is sole authoring surface).
--
-- reading_progress: upsert semantics; one row per (creator_id, work_id, chapter).
-- scroll_progress is stored as thousandths (0–10000) — clients interpret the unit.
--
-- reading_annotations: CRUD over annotation_id primary key; character-offset
-- anchoring into body plain text. Color enum {yellow, blue, green, pink}. Each
-- row may carry an optional free-text note. Offsets may drift after body edits
-- in canvas; the UI shows a drift notice — no reconciliation logic in MVP.

CREATE TABLE IF NOT EXISTS reading_progress (
    creator_id       TEXT NOT NULL,
    work_id          TEXT NOT NULL,
    chapter          INTEGER NOT NULL,
    scroll_progress  INTEGER NOT NULL DEFAULT 0
                     CHECK (scroll_progress >= 0 AND scroll_progress <= 10000),
    updated_at       TEXT NOT NULL,  -- ISO 8601
    PRIMARY KEY (creator_id, work_id, chapter),
    FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_reading_progress_work_chapter
    ON reading_progress(work_id, chapter);

CREATE TABLE IF NOT EXISTS reading_annotations (
    annotation_id    TEXT PRIMARY KEY,
    creator_id       TEXT NOT NULL,
    work_id          TEXT NOT NULL,
    chapter          INTEGER NOT NULL,
    start_offset     INTEGER NOT NULL CHECK (start_offset >= 0),
    end_offset       INTEGER NOT NULL CHECK (end_offset > start_offset),
    selected_text    TEXT NOT NULL DEFAULT '',
    color            TEXT NOT NULL DEFAULT 'yellow'
                     CHECK (color IN ('yellow', 'blue', 'green', 'pink')),
    note             TEXT,
    created_at       TEXT NOT NULL,  -- ISO 8601
    updated_at       TEXT NOT NULL,  -- ISO 8601
    FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_reading_annotations_work_chapter
    ON reading_annotations(creator_id, work_id, chapter);

CREATE INDEX IF NOT EXISTS idx_reading_annotations_creator
    ON reading_annotations(creator_id);
