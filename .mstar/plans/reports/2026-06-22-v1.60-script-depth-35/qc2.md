---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-22-v1.60-script-depth-35"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (preset validation, section completion logic, KB extraction safety on malformed LLM JSON, migration CHECK compatibility, BlockType handling, edge cases in completion detection)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-22-v1.60-script-depth-35 (Track B — Script Profile Depth 3.5 Promotion)
- Review range / Diff basis: 7cec348d..4d322c7c (Wave 1)
- Working branch (verified): iteration/v1.60
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed (key surfaces for security/correctness):
  - `.mstar/plans/2026-06-22-v1.60-script-depth-35.md`
  - `.mstar/knowledge/specs/script-profile.md`
  - `crates/nexus-orchestration/embedded-presets/script-writing/preset.yaml`
  - `crates/nexus-orchestration/embedded-presets/script-writing/prompts/{outline,draft,revise,finalize,finalize-exit}.md`
  - `crates/nexus-local-db/src/work_chapters.rs` (is_script_complete + supporting)
  - `crates/nexus-orchestration/src/quality_loop.rs` (candidate_from_llm_json_for_profile + block_type_to_script_category + tests)
  - `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql`
  - `crates/nexus-local-db/src/lib.rs`, `works.rs` (profile helpers)
- Commit range: 7cec348d..4d322c7c
- Tools run:
  - `git diff 7cec348d..4d322c7c --stat`
  - Targeted reads of completion logic, KB extraction profile dispatch, migration, preset gates
  - `grep` for `is_script_complete`, `candidate_from_llm_json_for_profile`, `block_type_to_script_category`, "script" profile paths
  - Manual inspection of frontmatter parsing, filesystem path construction, LLM JSON handling branches

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-001 — `is_script_complete` performs filesystem reads under caller-supplied `workspace_dir`; no explicit path canonicalization or boundary check inside the function itself.**
  The function builds paths as `workspace_dir.join("Works").join(work_ref).join(rel_path)`. It relies on the caller (daemon handler layer) to have already validated that `work_ref` came from a trusted DB row and that `workspace_dir` is the correct workspace root for the authenticated creator. Inside `is_script_complete` there is no additional `canonicalize()` + prefix check or rejection of `..` components. If a compromised or misbehaving caller ever passed a malicious `workspace_dir` or a `work_ref` containing path separators, this could traverse.
  - Location: `crates/nexus-local-db/src/work_chapters.rs` (the new `is_script_complete` function, ~lines 1440-1490 in the post-diff file).
  - Context: This is the same pattern used by other profile completion helpers (e.g., game-bible). The security boundary is intended to be at the handler that resolves the work and the workspace root. Still, for a function named "is_*_complete" that is exported from the local-db crate, an explicit defensive comment or a small internal sanitizer would be prudent.
  - Fix recommendation: Add a short doc comment stating the trust assumption ("caller must ensure workspace_dir is a validated workspace root and work_ref is a DB-sourced slug") and/or perform a minimal sanity check (reject if work_ref contains '/' or '\', or use a helper that asserts the final path is under the expected Works/ tree).

- **W-002 — Migration 202606230001_work_profile_script.sql recreates the `works` table; the new CHECK constraint is a strict superset, but the migration does not include a `PRAGMA foreign_key_check` or equivalent post-rename validation step.**
  The migration correctly:
  - Creates `works_new` with the expanded CHECK (`... OR work_profile IN ('novel', 'essay', 'game_bible', 'script')`)
  - Copies data (`INSERT INTO works_new SELECT * FROM works`)
  - Drops and renames
  - Recreates indexes
  Existing rows with `NULL` or the prior three values remain valid. No data loss path is apparent. However, because this is a table-recreate migration on a table that participates in FK relationships (world_id, creator_id, etc.), a post-migration integrity check would be the belt-and-suspenders practice.
  - Location: `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql`.
  - Risk: Low (the copy is a straight SELECT * and the constraint only widens). Still, for future migrations that touch this table, the pattern should include an explicit check.
  - Fix recommendation: Add a one-line comment in the migration (or in the crate's migration test harness) noting that a `PRAGMA foreign_key_check` or equivalent was considered and deemed unnecessary for this widening-only change, or actually run one in a test migration step.

### 🟢 Suggestion
- **S-001 — `candidate_from_llm_json_for_profile` for "script" has a broad default to "dialogue" for unknown block_types.**
  The function `block_type_to_script_category` maps the three native script BlockTypes directly and then has a long list of cross-domain mappings (character→dialogue, event→beat, etc.). The `_` arm defaults to "dialogue" with only a `tracing::debug!`. This is reasonable (dialogue is the most generic script-level unit), but it means a completely novel or mistyped block_type from an LLM will silently become dialogue without failing the extraction.
  - Location: `quality_loop.rs:877-910` (the new script arm) and the test cases that assert the defaults.
  - Impact: Low for correctness (the candidate is still produced and can be reviewed/adopted); the debug log gives operators visibility. If stricter "unknown block_type must be rejected" semantics are desired later, this is the knob.
  - Related tests: `candidate_from_llm_json_for_profile_script_unknown_defaults_dialogue` and cross-domain tests — all pass and document the intended behavior.

- **S-002 — `is_script_complete` only inspects two "critical" files (`Scripts/script.md` + `Beats/beat-sheet.md`).**
  Per the reviewed code and the preset comments, these two are the gate for "script complete". The plan and spec mention per-act completion semantics and that all acts reaching `accepted` can trigger stop. The implementation in V1.60 P1 appears to use these two top-level files as the proxy (mirroring how game-bible uses certain design files). If future script workflows introduce additional mandatory top-level sections whose acceptance must also be required, the constant `CRITICAL_SECTIONS` will need to grow and the function will need to be updated in lockstep with the preset gates.
  - This is a design-evolution note rather than a defect in the current batch.

- **S-003 — Preset `script-writing` declares `requires_capabilities` that include `judge.llm` and `creator.inject_prompt`, but the five-question exit judge lives in `finalize-exit.md`.**
  The gate structure (work_profile == script, intake complete, Scripts/ dir exists) plus the `exit_when: llm_judge` on finalize is consistent with the game-bible Depth 3.5 pattern. No obvious injection surface in the reviewed prompt templates (they are static files rendered with controlled variables).
  - Minor hygiene: the preset version is "1"; the sync test `preset_version_mapping_matches_yaml_includes_cron_presets` was extended to cover it (per plan T6).

### Notes (positive evidence)
- Migration is widening-only on the CHECK constraint; all prior `work_profile` values (NULL, novel, essay, game_bible) remain valid.
- `is_script_complete` correctly short-circuits on missing intake_status != complete, missing work_ref, or any critical section not having `section_status: accepted`. It returns false early with clear tracing.
- KB extraction profile dispatch for "script" is wired through the same `candidate_from_llm_json_for_profile` path used by novel and game_bible; the new `block_type_to_script_category` is exercised by dedicated tests that cover native types + cross-domain mappings + unknown default.
- Preset gates are present and match the plan (work_profile gate, filesystem presence gate, intake gate).
- No `unwrap`/`expect` on production paths in the new script completion or KB extraction branches (tests legitimately use them).
- All reviewed SQL remains parameterized.

## Source Trace
- **W-001 (fs path trust)**: `work_chapters.rs` implementation of `is_script_complete`; path construction from `workspace_dir` + `work_ref` + rel_path; absence of canonicalize inside the function.
- **W-002 (migration integrity check)**: the 20260623 migration file (table recreate + copy + rename pattern).
- **S-001 (script KB default)**: `quality_loop.rs: block_type_to_script_category` and the "unknown defaults dialogue" test.
- Cross-reference: plan T4 (section completion), T5 (profile-aware KB), T7 (hermetic tests), compass Q5, script-profile.md §8 (completion semantics).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

(The two Warnings are defensive-hardening and migration-hygiene items. Core correctness properties hold: the CHECK is backward-compatible, completion detection correctly gates on the documented critical sections + intake, LLM JSON extraction for script is profile-aware with documented fallbacks and tests, and the preset gates are present. No injection, no panic paths on malformed input in the reviewed extraction, no data-corrupting migration behavior.)
