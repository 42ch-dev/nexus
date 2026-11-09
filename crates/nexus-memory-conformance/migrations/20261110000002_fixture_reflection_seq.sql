-- Newest-first listing order for `list_reflections` (greptile P1#3).
--
-- Rows are immutable and never deleted, so one monotonic sequence per row is
-- a total order: same-second `created_at` ties no longer decide the page, and
-- a `reflection_id` (a random uuid) never participates in ordering. `seq` is
-- fixture-internal: the wire shape carries no ordering column.
ALTER TABLE reflections ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;

WITH numbered AS (
    SELECT reflection_id, ROW_NUMBER() OVER (ORDER BY created_at, reflection_id) AS position
    FROM reflections
)
UPDATE reflections SET seq = (
    SELECT position FROM numbered WHERE numbered.reflection_id = reflections.reflection_id
);

-- The list scan order this migration introduces (newest first within a
-- subject); the oldest-first index from the prior migration is superseded.
DROP INDEX IF EXISTS idx_reflections_subject_created;
CREATE INDEX IF NOT EXISTS idx_reflections_subject_seq
    ON reflections (space_id, subject_id, seq DESC);
