-- V1.47 P0 fix (qc1 W-2 / qc2 W-1 / qc3 W-2): idempotency guard for
-- review→finding.
--
-- novel-quality-loop.md §8.3 asked the plan to lock in a decision on
-- idempotency: calling the review terminal hook twice on the same chapter
-- must NOT create duplicate findings.
--
-- - `source_schedule_id` — the originating `creator_schedules.schedule_id`
--   when the finding was synthesized by the review terminal hook. NULL for
--   manually-created findings (general CRUD path). Server-only column; not
--   surfaced in the wire contract (FindingApiDto).
-- - `findings_unique_review_per_chapter` — partial unique index ensuring at
--   most one finding per (work_id, chapter, source_schedule_id) triple when
--   `source_schedule_id IS NOT NULL`. The INSERT in
--   `create_finding_from_review` uses `ON CONFLICT DO NOTHING` against this
--   index; a second terminal transition for the same review schedule is a
--   no-op that returns the existing finding id.
--
-- Both additions are local DB schema; no wire-schema (nexus-contracts) change.

ALTER TABLE findings ADD COLUMN source_schedule_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS findings_unique_review_per_chapter
  ON findings (work_id, chapter, source_schedule_id)
  WHERE source_schedule_id IS NOT NULL;
