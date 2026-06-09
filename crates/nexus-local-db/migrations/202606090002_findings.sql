-- Findings table for quality-loop (novel-quality-loop §2.1, V1.39 P1).
--
-- Stores per-Work quality findings from the review/reflection-loop stage.
-- Each finding has a severity, status lifecycle, and a routing hint
-- (target_executor) indicating which preset should address it.

CREATE TABLE IF NOT EXISTS findings (
    finding_id       TEXT NOT NULL PRIMARY KEY,          -- ULID
    work_id          TEXT NOT NULL,                       -- FK → works.work_id
    chapter          INTEGER,                             -- optional chapter binding (NULL = Work-level)
    severity         TEXT NOT NULL DEFAULT 'info',        -- info | minor | major | blocker
    status           TEXT NOT NULL DEFAULT 'open',        -- open | resolved | wont_fix
    title            TEXT NOT NULL,                        -- short human-readable label
    description      TEXT NOT NULL DEFAULT '',             -- detailed finding body
    target_executor  TEXT NOT NULL DEFAULT 'none',        -- write | brainstorm | none | master
    creator_id       TEXT NOT NULL,                        -- owning creator (isolation)
    created_at       INTEGER NOT NULL,                     -- Unix epoch
    updated_at       INTEGER NOT NULL,                     -- Unix epoch
    FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE
);

-- Primary query patterns: list open findings per work, list findings per creator
CREATE INDEX IF NOT EXISTS idx_findings_work_status
    ON findings(work_id, status);
CREATE INDEX IF NOT EXISTS idx_findings_creator_status
    ON findings(creator_id, status);
-- Per novel-quality-loop.md §2.1: chapter-scoped lookups (review-stage hook hot path)
CREATE INDEX IF NOT EXISTS idx_findings_work_chapter_status
    ON findings(work_id, chapter, status);
