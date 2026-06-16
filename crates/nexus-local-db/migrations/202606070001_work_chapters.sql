-- V1.36: work_chapters table — per-chapter state SSOT
-- (novel-writing/workflow-profile.md §4.1.1)
CREATE TABLE IF NOT EXISTS work_chapters (
    work_id              TEXT NOT NULL,
    chapter              INTEGER NOT NULL,        -- chapter number; 1..total_planned_chapters
    volume               INTEGER,                  -- nullable; V1.36 single-chapter MVP leaves NULL
    slug                 TEXT,                     -- filename slug, e.g. "the-third-layer"
    planned_word_count   INTEGER NOT NULL DEFAULT 4000,
    actual_word_count    INTEGER,                  -- set on first transition to finalized
    status               TEXT NOT NULL DEFAULT 'not_started'
                         CHECK (status IN ('not_started','outlined','draft','finalized','published')),
    outline_path         TEXT,                     -- relative to workspace root
    body_path            TEXT,                     -- relative to workspace root
    created_at           TEXT NOT NULL,             -- ISO 8601
    updated_at           TEXT NOT NULL,             -- ISO 8601
    PRIMARY KEY (work_id, chapter),
    FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS work_chapters_by_status ON work_chapters(status);
