---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-28-v1.73-canvas-world-kb-beta"
verdict: "Approve"
generated_at: "2026-06-29"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (Seat 2)
- Report Timestamp: 2026-06-29

## Scope
- plan_id: 2026-06-28-v1.73-canvas-world-kb-beta
- Review range / Diff basis: merge-base: 87ab75bb (origin/main) ... tip: d04a6b4e (HEAD) — equivalent to `git diff 87ab75bb...d04a6b4e`
- Working branch (verified): iteration/v1.73
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 95 changed (focus on security/correctness surface: world_kb.rs, kb_store.rs, outline.rs, tests, contracts)
- Commit range: 87ab75bb...d04a6b4e
- Tools run: cargo test -p nexus-daemon-runtime --test world_kb_patch (9/9 passed), cargo test -p nexus-daemon-runtime --test outline_patch (15/15 passed), git diff, manual source review of auth/OCC/state-machine/validation paths

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- (S-01) In `promote_reject`, when CAS rows_affected != 1 the handler returns a 409 `world_kb_conflict`. The pre-check in `promote_candidate` already ensures the candidate is `pending`, so the 409 path is only reachable on genuine races. Consider surfacing a distinct "already_terminal" 422 in the future for clearer client UX, but current behavior is safe and non-leaking.
- (S-02) B2 volume validation uses the documented "max+1 allowed" bounded rule (not strict existence-only). This is intentional to preserve the "move chapter to the next sequential volume" authoring flow. The tests and code comments make the contract explicit; no correctness gap.
- (S-03) Conflict errors (409) include the entity/job id and a safe hint ("refetch..."). No sensitive data or cross-world information is leaked. Acceptable for local-first API.

## Source Trace
- Finding ID: N/A (no blocking findings)
- Source Type: manual code review + test execution + git diff
- Source Reference: crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs (auth, promote state machine, CAS, require_world_owner), crates/nexus-local-db/src/kb_store.rs (cas_update_key_block_fields + COALESCE), crates/nexus-daemon-runtime/src/api/handlers/outline.rs (B1–B4 validators), tests/world_kb_patch.rs, tests/outline_patch.rs
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Authorization (Seat 2 focus)
- All four World KB routes (`patch-entity`, `promote-candidate`, `graph`, `candidates`) call `require_world_owner` after `require_creator`.
- `require_world_owner` does a direct `SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?` and returns:
  - 404 for missing world
  - 403 with explicit reason for cross-creator or NULL owner
- Cross-creator world blocks are rejected before any KB read/write. Matches entity-scope-model §1.2 ownership and the implementer's claim that the guard mirrors `nexus.kb_snapshot.write`.

## Promotion State Machine Integrity
- `promote_candidate` first loads the job and checks `promotion_status == "pending"`. Terminal states (`confirmed`/`rejected`) return 422 `world_kb_validation_failed` citing entity-scope-model §5.5.2.
- Adopt: `insert_key_block_in_tx` + `mark_confirmed_in_tx_with_cas` inside a single `BEGIN`/`COMMIT`. On CAS false (no longer pending) the tx is rolled back and a 422 is returned. No double-adopt possible.
- Reject: direct CAS `UPDATE ... SET promotion_status = 'rejected' WHERE ... AND promotion_status = 'pending' AND version = ?`. Rows affected != 1 → 409 (race) or already-terminal (harmless).
- Merge: CAS-update target body + CAS-reject candidate inside one tx. Same atomicity guarantees.
- All transitions are atomic at the DB level. No window for a rejected candidate to be re-adopted.

## OCC Correctness (per-row CAS on kb_key_blocks.revision)
- `cas_update_key_block_fields` uses:
  ```sql
  WHERE key_block_id = ? AND COALESCE(revision, 0) = ?
  ```
- NULL → 0 normalization is applied on both the read side (`current_version = kb.revision.unwrap_or(0)`) and the write guard. This correctly handles pre-existing rows (revision NULL) and first-edit (NULL→1).
- The follow-up `SELECT revision` on rows_affected==0 distinguishes "not found" from "version mismatch" and returns `actual` (NULL normalized to 0) in the `VersionMismatch` error.
- 409 `WorldKbConflictError` is mapped from `LocalDbError::VersionMismatch`. The handler returns before any write on stale `expected_version`.
- Two concurrent patches on the same entity cannot both succeed — the second will see rows_affected==0 and surface 409.
- Conflict response does not leak other worlds' data or internal DB state.

## Input Validation (B1–B4)
- **B1 (slug)**: `validate_chapter_slug` enforces kebab-case (`^[a-z0-9-]+$`), 1..=80, and Work-wide uniqueness (excluding the chapter being patched). Re-asserting the same slug on the same chapter is allowed. Uppercase, spaces, and duplicates are rejected with 422. Injection/path-traversal via slugs is blocked by the character allowlist.
- **B2 (volume)**: `validate_volume_target` rejects <1 and > (max+1). The "max+1 allowed" rule is explicit in code + tests and preserves the legitimate "create next volume by binding" flow. Arbitrary far-future volumes (typos) are rejected rather than auto-created.
- **B3 (foreshadow temporal order)**: `timeline_link_foreshadow` now loads both events, requires both to have `realizes_chapter_id`, and enforces `source_chapter <= target_chapter`. Null realization on either end produces a clear 422 explaining the requirement.
- **B4 (published-chapter guard)**: `ensure_chapter_not_published` is called for `move_chapter` and `attach_to_volume` in `apply_structure_patch`. The older `patch_chapter` path retains its `BadRequest` guard. Both routes are now covered.

## Wire Contract Correctness
- All new DTOs live under `crates/nexus-contracts/src/generated/.../world_kb/` and the parallel TypeScript package.
- No changes to existing outline or work contracts. The additions are purely additive (new routes + new projection/response types). No breaking change to existing consumers.

## Test Coverage (executed)
- `cargo test -p nexus-daemon-runtime --test world_kb_patch` → 9/9 passed (stale version 409, cross-author 403, deleted-entity 422, adopt/reject, graph filtering, etc.).
- `cargo test -p nexus-daemon-runtime --test outline_patch` → 15/15 passed (B1 slug rules, B2 volume rules, B3 temporal order with null cases, B4 published guard on both patch paths).

## CI / Static Checks
- All relevant tests green at review time.
- No new clippy warnings introduced in the reviewed security/correctness surface (the `too_many_lines` allow on `patch_outline_chapter` is pre-existing and justified in a comment).

**Verdict**: Approve
