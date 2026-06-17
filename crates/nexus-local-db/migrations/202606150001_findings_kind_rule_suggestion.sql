-- V1.47 P0: Add `kind` and `rule_suggestion` columns to findings.
--
-- Implements novel-writing/quality-loop.md §2.1 (V1.47 Draft):
-- - `kind` — finding category (`continuity`, `craft`, `plot_hole`, `world_inconsistency`, …).
--   NOT NULL with default `'craft'` so existing rows and legacy insert paths
--   remain valid without backfill.
-- - `rule_suggestion` — optional prose suggestion for Layer 2 rules.
--   Persisted on the finding row only; V1.47 P0 does NOT write
--   `Works/<work_ref>/AGENTS.md`.
--
-- Both columns are local DB schema; no wire-schema (nexus-contracts) change.

ALTER TABLE findings ADD COLUMN kind TEXT NOT NULL DEFAULT 'craft';
ALTER TABLE findings ADD COLUMN rule_suggestion TEXT;
