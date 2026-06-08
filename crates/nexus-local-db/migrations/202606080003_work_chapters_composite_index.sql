-- V1.38: Add composite index for next_chapter() lookup.
-- Pattern: SELECT MIN(chapter) WHERE work_id = ? AND status IN (...)
-- Without this, SQLite scans all work_chapters rows for the work.
CREATE INDEX IF NOT EXISTS work_chapters_by_work_status_chapter
    ON work_chapters(work_id, status, chapter);
