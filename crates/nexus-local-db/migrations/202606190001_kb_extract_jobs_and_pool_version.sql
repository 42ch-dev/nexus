-- V1.51 T-B P1: Add version columns for per-row optimistic concurrency control.
--
-- Spec: concurrency.md §7 (OCC extension — V1.51 T-B P1).
-- Plan: 2026-06-18-v1.51-per-row-occ.md §2.1, §4.1.
--
-- The `version` column enables CAS (Compare-And-Swap) updates:
--   UPDATE ... WHERE id = ? AND version = expected_version
-- If rows_affected = 0, the row was modified by another writer between the
-- caller's read and its update → E_VERSION (exit 76).
--
-- Both tables start with version = 0 for existing rows (DEFAULT).

ALTER TABLE kb_extract_jobs ADD COLUMN version INTEGER NOT NULL DEFAULT 0;

ALTER TABLE novel_pool_entries ADD COLUMN version INTEGER NOT NULL DEFAULT 0;
