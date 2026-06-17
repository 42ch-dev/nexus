-- V1.36: Add novel-profile columns to works table
-- (novel-writing/workflow-profile.md §2.1)

ALTER TABLE works ADD COLUMN work_profile TEXT DEFAULT NULL
    CHECK (work_profile IS NULL OR work_profile IN ('novel'));

ALTER TABLE works ADD COLUMN work_ref TEXT DEFAULT NULL;

ALTER TABLE works ADD COLUMN total_planned_chapters INTEGER DEFAULT NULL;

ALTER TABLE works ADD COLUMN current_chapter INTEGER NOT NULL DEFAULT 0;
