-- V1.52 T-A P0: Audit columns for high-confidence auto-promoted KB candidates.
--
-- Plan: `.mstar/plans/2026-06-19-v1.52-outline-five-q-and-auto-promote.md`
-- Spec: `.mstar/knowledge/specs/novel-writing/quality-loop.md` §5.6
--       `.mstar/knowledge/specs/entity-scope-model.md` §5.5
--
-- When `creator world kb adopt --auto <world_ref>` promotes a pending candidate
-- without human confirmation, it writes these three columns so the decision is
-- auditable: when, why, and by which actor (CLI + active creator_id).
--
-- Additive only: existing pending/confirmed/rejected rows default to NULL
-- (no destructive change; no data backfill needed).

ALTER TABLE kb_extract_jobs ADD COLUMN auto_promoted_at TEXT;
ALTER TABLE kb_extract_jobs ADD COLUMN auto_promoted_reason TEXT;
ALTER TABLE kb_extract_jobs ADD COLUMN auto_promoted_by TEXT;
