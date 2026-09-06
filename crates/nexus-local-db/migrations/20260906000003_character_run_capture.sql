-- v1.185 P3 Task 1: explicit run capture receipts and pending provenance.
-- Spec: actor-product-model.md §11.6; architecture-handoff §3.

ALTER TABLE character_memory_pending_review
    ADD COLUMN source_operation_id TEXT;

DROP INDEX IF EXISTS idx_character_pending_review_character_session;

CREATE UNIQUE INDEX IF NOT EXISTS idx_character_pending_review_manual_session
    ON character_memory_pending_review (character_id, session_id)
    WHERE source_operation_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_character_pending_review_source_operation
    ON character_memory_pending_review (source_operation_id)
    WHERE source_operation_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS character_run_captures (
    operation_id TEXT NOT NULL PRIMARY KEY,
    session_id TEXT NOT NULL,
    character_id TEXT NOT NULL
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    binding_id TEXT NOT NULL,
    lifecycle_epoch INTEGER NOT NULL CHECK (lifecycle_epoch >= 0),
    pending_id TEXT NOT NULL UNIQUE,
    captured_at TEXT NOT NULL
);
