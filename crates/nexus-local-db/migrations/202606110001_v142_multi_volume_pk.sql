-- V1.42 P1 (T1): Multi-volume PK migration for work_chapters.
-- Spec: novel-workflow-profile.md §4.5.4, local-db-schema.md V1.42 amendment.
--
-- Steps:
-- 1. Backfill implicit volume = 1 for all existing rows (NULL → 1).
-- 2. Make volume NOT NULL DEFAULT 1.
-- 3. Recreate the table with the new composite PK (work_id, volume, chapter).
--    SQLite does not support ALTER TABLE … ALTER CONSTRAINT; we must
--    recreate the table. Data is preserved via INSERT INTO … SELECT.
-- 4. Recreate indexes (including volume-aware index for next_chapter query).
--
-- Idempotency: DROP IF EXISTS for the legacy table at the top ensures
-- re-running on an already-migrated DB is a no-op (the RENAME will
-- fail harmlessly because work_chapters_legacy already doesn't exist,
-- but we guard it anyway).

-- Pre-guard: drop legacy table from any prior partial run (idempotency, W-01).
DROP TABLE IF EXISTS work_chapters_legacy;

-- Step 1: Backfill implicit volume = 1 for all existing rows.
UPDATE work_chapters SET volume = 1 WHERE volume IS NULL;

-- Step 2+3: Recreate table with composite PK (work_id, volume, chapter).
-- SQLite requires rename + recreate pattern for PK changes.
ALTER TABLE work_chapters RENAME TO work_chapters_legacy;

CREATE TABLE work_chapters (
  work_id              TEXT NOT NULL,
  chapter              INTEGER NOT NULL,
  volume               INTEGER NOT NULL DEFAULT 1,
  slug                 TEXT,
  planned_word_count   INTEGER,
  actual_word_count    INTEGER,
  status               TEXT NOT NULL,
  outline_path         TEXT,
  body_path            TEXT,
  created_at           INTEGER NOT NULL,
  updated_at           INTEGER NOT NULL,
  PRIMARY KEY (work_id, volume, chapter),
  FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE
);

INSERT INTO work_chapters
  (work_id, chapter, volume, slug, planned_word_count, actual_word_count,
   status, outline_path, body_path, created_at, updated_at)
SELECT
  work_id, chapter, volume, slug, planned_word_count, actual_word_count,
  status, outline_path, body_path, created_at, updated_at
FROM work_chapters_legacy;

DROP TABLE work_chapters_legacy;

-- Step 4: Recreate indexes.
CREATE INDEX IF NOT EXISTS work_chapters_by_status ON work_chapters(status);
CREATE INDEX IF NOT EXISTS work_chapters_by_work_status_chapter
    ON work_chapters(work_id, status, chapter);
-- W-02: Composite index for volume-aware next-chapter query
-- (`WHERE work_id = ? AND status IN (...) ORDER BY volume, chapter LIMIT 1`).
CREATE INDEX IF NOT EXISTS idx_work_chapters_next_volume_aware
    ON work_chapters(work_id, status, volume, chapter);
