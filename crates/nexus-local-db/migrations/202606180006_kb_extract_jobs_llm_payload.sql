-- V1.51 T-A P0: Extend kb_extract_jobs with LLM extraction metadata.
--
-- Plan: `.mstar/plans/2026-06-18-v1.51-llm-extraction.md`
-- Spec: `.mstar/knowledge/specs/llm-extract.md` §3.2
--       `.mstar/knowledge/specs/entity-scope-model.md` §5.5.6
--
-- Closes R-V150KBED-01: the V1.50 heuristic defaulted every review-time
-- candidate to `block_type_guess='character'`. V1.51 swaps the heuristic for
-- the `nexus.llm.extract` capability at the review-time extraction hook,
-- which now fills an LLM-judged block_type + canonical_name + confidence +
-- source_quote. This migration adds the two columns that have no existing
-- home: `llm_confidence` and `llm_source_quote`.
--
-- `block_type` reuses the existing `block_type_guess` column (V1.50 T-B P1);
-- `canonical_name` reuses `canonical_name_guess`. The `proposed_payload` JSON
-- also carries all four LLM keys (see llm-extract.md §3.1) so the adopt CLI
-- can read them from either place; the dedicated columns exist so `kb pending`
-- can sort/filter by confidence without parsing JSON.
--
-- Additive only: existing V1.50 heuristic rows default to NULL for both
-- columns (no destructive change; no data backfill needed). The promotion
-- state machine (entity-scope-model §5.5.1–§5.5.2) is unchanged — these
-- columns carry extraction metadata, not promotion state.

-- LLM self-reported confidence in [0.0, 1.0]. NULL for heuristic-extracted
-- rows (V1.50 behavior) and for legacy V1.29/V1.40 extraction-queue rows.
ALTER TABLE kb_extract_jobs ADD COLUMN llm_confidence REAL;

-- Verbatim chapter excerpt justifying the extraction (entity-scope-model
-- §5.5.6). NULL for heuristic-extracted rows and legacy queue rows.
ALTER TABLE kb_extract_jobs ADD COLUMN llm_source_quote TEXT;
