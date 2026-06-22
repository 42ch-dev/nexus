-- Adds creator_id column to reference_sources for cross-creator isolation (V1.58 P3).
--
-- H-002 (QC2 security finding): reference sources previously had no creator/workspace
-- scoping — any active creator could enumerate and trigger refresh on sources across
-- creators. The body.md file path already encodes creator identity (filesystem isolation),
-- but the DB row lacked a creator_id. This column closes the gap for list/get/refresh paths.
--
-- Pre-1.0: existing rows get a placeholder. The row-level isolation is additive;
-- no existing non-creator-scoped query paths are removed.

ALTER TABLE reference_sources ADD COLUMN creator_id TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_reference_sources_creator_id
    ON reference_sources(creator_id);
