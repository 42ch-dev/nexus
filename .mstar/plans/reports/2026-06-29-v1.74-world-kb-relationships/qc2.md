---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-29-v1.74-world-kb-relationships"
verdict: "Approve"
generated_at: "2026-06-29"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (input validation, authorization, OCC CAS correctness, self-loop guard, symmetric semantics, B1 atomic rollback, B4 idempotency removal, A4 rule completeness)
- Report Timestamp: 2026-06-29

## Scope
- plan_id: 2026-06-29-v1.74-world-kb-relationships (lead; consolidated review covers P0 world-kb-relationships + P1 hygiene-slate-clear + integration codegen)
- Review range / Diff basis: 0fed23f8..38cacda2 (origin/main merge-base..iteration/v1.74 HEAD; 26 commits). Equivalent to `git diff 0fed23f8..38cacda2`.
- Working branch (verified): iteration/v1.74
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 12 (core security/correctness surface)
- Commit range: 0fed23f8..38cacda2
- Tools run:
  - `git branch --show-current` + `git rev-parse HEAD` (branch/HEAD verification)
  - `git diff 0fed23f8..38cacda2 -- crates/nexus-local-db/src/kb_relationships.rs crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs crates/nexus-daemon-runtime/tests/world_kb_relationships.rs crates/nexus-local-db/migrations/202606290001_kb_relationships.sql`
  - `git log --oneline 0fed23f8..38cacda2 -- '*strategy*'`
  - `git show 31908694 -- crates/nexus-daemon-runtime/src/api/handlers/strategy.rs` (B1 atomic rollback)
  - `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test world_kb_relationships` (11 passed)
  - `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test world_kb_patch` (14 passed)
  - Compass read: `.mstar/iterations/v1.74-world-kb-relationships-and-hygiene-compass-v1.md` §1.1 A4 + §6 risks
  - Grep for creator/auth/require_world_owner, idempotency_key, rows_affected, VersionMismatch, validate_relationship_input

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- **S1 (low)**: Cross-world relationship scope check returns 404 after auth gate (consistent with V1.73 entity patch behavior in same file). This is information-hiding, not a leak, but the error path for "relationship_id exists but in another world you own" is 404 rather than 403. No security impact because `require_world_owner` already ran and passed. (Evidence: `patch_relationship_update`/`patch_relationship_remove` lines ~1165-1168 and ~1232-1235 in world_kb.rs.)
- **S2 (low)**: Post-commit re-fetch of the inserted/updated row to build the response happens outside the transaction (standard pattern). A crash between commit and response could mean an immediate client refetch misses the row. Not an OCC or atomicity bug; the write is durable. (Evidence: `patch_relationship_add` ~1108-1113, `patch_relationship_update` ~1190-1196.)
- **S3 (low)**: The regression test for "concurrent daemon + canvas writes" (compass §6.7) is satisfied by the combination of `update_stale_version_returns_409` (handler) + `test_update_cas_fails_on_stale_revision` (store). No single named integration test using two `WorkspaceState` instances appears in the diff. The OCC contract is correctly exercised; naming is cosmetic.

## Source Trace
- Finding ID: F-001 (OCC CAS)
- Source Type: git-diff + test run
- Source Reference: `kb_relationships.rs:update_relationship_in_tx` (WHERE relationship_id=? AND revision=?; rows_affected()==1 else VersionMismatch); handler `map_relationship_cas_err` → 409; test `update_stale_version_returns_409`
- Confidence: High

- Finding ID: F-002 (auth)
- Source Type: git-diff + grep
- Source Reference: `world_kb.rs:patch_relationship` (require_creator + require_world_owner at top, before match on action); `require_world_owner` queries narrative_worlds.owner_creator_id
- Confidence: High

- Finding ID: F-003 (self-loop)
- Source Type: git-diff + test run
- Source Reference: `validate_relationship_input` (source_entity_id == target_entity_id → world_kb_validation_failed 422); `add_self_loop_rejected_422`
- Confidence: High

- Finding ID: F-004 (B1 atomic)
- Source Type: git-log + git-show
- Source Reference: commit 31908694; `rollback_template_write` now uses `atomic_write_with_dir_fsync` + dir fsync; two new unit tests
- Confidence: High

- Finding ID: F-005 (B4 idempotency)
- Source Type: grep + diff
- Source Reference: no `idempotency_key` in relationship DTO paths or handler; new `WorldKbPatchRelationshipRequest` has no such field
- Confidence: High

- Finding ID: F-006 (A4 completeness)
- Source Type: compass + code
- Source Reference: compass §1.1 A4 (7 rules); handler implements all 7 pre-write (ID existence via require_entities_in_world + scope, auth via require_world_owner, self-loop, taxonomy via validate + enum parse, symmetric flag storage, confidence range, version precondition via CAS)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Checklist Execution (security/correctness lens)

- [x] OCC correctness: `update_relationship_in_tx` / `delete_relationship_in_tx` use `WHERE relationship_id = ? AND revision = ?`; `rows_affected() == 1` returns new version, else re-reads revision and returns `LocalDbError::VersionMismatch` → handler emits 409 `WorldKbConflictError` with `current_version`. No silent-success path on stale write.
- [x] Self-loop guard: `source_entity_id != target_entity_id` enforced in `validate_relationship_input` (called before any DB mutation for add/update); returns 422. Test covers it.
- [x] Authorization: `patch_relationship` calls `require_creator` then `require_world_owner(pool, &world_id, &creator_id)` before dispatching to add/update/remove. Cross-world injection is rejected at the world-owner gate.
- [x] Symmetric consistency: storage is single row; `get_graph` projects forward + `symmetric_reverse` sharing `relationship_id`. Edit/delete via either projection targets the same row (CAS by id). Tests: `get_graph_projects_symmetric_reverse_edge` + store CAS tests.
- [x] B1 atomic rollback: `rollback_template_write` now uses `atomic_write_with_dir_fsync` (temp + fsync + rename + dir fsync). Two regression tests added covering restore-from-backup and remove-when-no-backup.
- [x] B4 idempotency_key removal: relationship DTOs and handler paths contain no `idempotency_key`. No code path reads a removed field.
- [x] Validation completeness (A4): all 7 rules implemented pre-write in the handler (see F-006). Validation short-circuits before any persistence.
- [x] Tests: 11 world_kb_relationships + 14 world_kb_patch all green under `SQLX_OFFLINE=true`.

## Evidence Notes
- All CAS paths (entity + relationship + promote) consistently use `rows_affected()` check + post-miss re-read for `current_version`.
- Auth ordering in relationship handler mirrors the corrected ordering used for `patch_entity` (auth before entity read to avoid cross-author existence leaks).
- No new SQL injection surface: all relationship queries use `sqlx::query!` / `query_as!` or the compile-time-checked helpers in `kb_relationships.rs`.
- The migration is additive with FKs to existing `kb_key_blocks`; no schema changes to prior tables.

---

## Revalidation
(none — initial review)
