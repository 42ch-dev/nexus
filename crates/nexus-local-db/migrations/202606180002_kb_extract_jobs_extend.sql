-- V1.50 T-B P1: Extend kb_extract_jobs with the World KB promotion state
-- machine (entity-scope-model.md §5.5).
--
-- Adds a promotion lifecycle that is *orthogonal* to the existing extraction
-- queue status (`queued|running|done|failed`, used by the V1.29/V1.40 LLM
-- extraction worker). Review-time heuristic extraction (V1.50 T-B P1) produces
-- candidate rows whose promotion state is governed by author confirm/dismiss
-- via `creator world kb adopt|reject`.
--
-- Deviations from plan §5. T1 (documented in Completion Report v2):
--   1. Column named `promotion_status` (not `status`) — the table already has
--      a `status` column with CHECK `('queued','running','done','failed')`;
--      SQLite cannot ALTER an existing CHECK in place and conflating the two
--      lifecycles in one column would be a schema smell.
--   2. `source_work_id` is not added as a new column — the V1.40 P3 `work_id`
--      column (TEXT) already carries the same source-work semantics and is
--      reused here. Only `source_chapter_id` is new.

-- Promotion state machine (entity-scope-model.md §5.5.1):
--   pending  → confirmed | rejected   (only via CLI adopt/reject)
--   confirmed → (terminal; row is now a regular kb_key_blocks KeyBlock)
--   rejected  → (terminal; archived in Logs/kb/rejected/)
ALTER TABLE kb_extract_jobs ADD COLUMN promotion_status TEXT
    NOT NULL DEFAULT 'pending'
    CHECK (promotion_status IN ('pending', 'confirmed', 'rejected'));

-- Proposed KeyBlock body as JSON (`{summary, attributes, tags}`). Heuristic
-- extraction fills a best-effort body that adopt will validate via
-- `SqliteKbStore::with_validation_mode(Novel)`.
ALTER TABLE kb_extract_jobs ADD COLUMN proposed_payload TEXT;

-- Source chapter number (references work_chapters.chapter within the row's
-- `work_id`). NULL for work-level candidates.
ALTER TABLE kb_extract_jobs ADD COLUMN source_chapter_id INTEGER;

-- Heuristic's BlockType guess (snake_case wire value, e.g. `character`).
ALTER TABLE kb_extract_jobs ADD COLUMN block_type_guess TEXT;

-- Heuristic's canonical_name guess (validated on adopt).
ALTER TABLE kb_extract_jobs ADD COLUMN canonical_name_guess TEXT;

-- Index for the `creator world kb pending <world_ref>` list query and the
-- idempotency pre-check (`is_idempotent` scans pending|confirmed rows for the
-- same work_id + canonical_name_guess).
CREATE INDEX IF NOT EXISTS idx_kb_extract_jobs_promotion_status_work
    ON kb_extract_jobs (promotion_status, work_id);
