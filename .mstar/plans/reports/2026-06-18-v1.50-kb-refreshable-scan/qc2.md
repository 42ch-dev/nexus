---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.50-kb-refreshable-scan
working_branch: feature/v1.50-kb-refreshable-scan
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-refreshable-scan
review_range: merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..e24574ae5f5f6e8186ee87fa1bc3d3acdc5f885c
verdict: Approve
generated_at: 2026-06-17T14:10:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security + correctness (primary); also regression coverage and error stability
- Report Timestamp: 2026-06-17T14:10:00Z

## Scope
- plan_id: 2026-06-18-v1.50-kb-refreshable-scan
- Review range / Diff basis: merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..e24574ae5f5f6e8186ee87fa1bc3d3acdc5f885c
- Working branch (verified): feature/v1.50-kb-refreshable-scan
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-refreshable-scan
- Files reviewed: 9 changed (+1946/-13); primary: `crates/nexus42/src/commands/creator/kb/rescan.rs` (577 LOC), `crates/nexus-kb/src/extract_sync.rs` (313 LOC), `crates/nexus-local-db/src/kb_extract_job.rs` (new upsert/cleanup), CLI + DAO test files, migration renumber.
- Commit range: 6 commits (T1–T6 + R-V150KBED-06)
- Tools run:
  - `cargo test -p nexus-local-db --test kb_extract_jobs_upsert` (6 passed)
  - `cargo test -p nexus42 --test kb_rescan_cli` (8 passed)
  - `cargo test -p nexus42 --test world_kb_promotion_cli` (11 passed — regression)
  - `cargo test -p nexus-local-db --test kb_extract_jobs_migration` (8 passed — regression)
  - `cargo +nightly fmt --all --check` (clean)
  - `cargo clippy --all -- -D warnings` (clean on touched crates)
  - Manual code inspection of authz gate, `compute_kb_diff`, `diff_and_apply`, `upsert_pending_candidate`, dry-run paths, error mapping, and migration file.

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- The rescan batch (`sync_candidates` + `sync_kb_rows`) performs per-row upserts/deletes and per-row `update_key_block` calls without an outer `BEGIN IMMEDIATE`/`COMMIT` transaction. For a single-user local CLI this is acceptable (re-runnable, protected by DB unique index on `(creator, work_entry_id, world)` and the per-row store contract). If concurrent daemon-driven rescans are added later (V1.51+), consider wrapping the chapter-scoped candidate sync in a transaction for a stronger "all-or-nothing per chapter" guarantee. (Low severity; not required for current acceptance criteria.)
- Consider adding a small hermetic test that exercises two rapid sequential rescans of the same chapter under the same creator to explicitly document the "last writer wins on payload" behavior (already covered indirectly by the idempotency tests).

## Source Trace
- Finding ID: (N/A — no blocking findings)
- Source Type: code review + test execution + static analysis
- Source Reference: plan §5 T1–T4, AC1–AC4, R-V150KBED-06; `kb_rescan_hermetic`, `require_world_owner`, `upsert_pending_candidate`, `compute_kb_diff`, `diff_and_apply`, test files, migration 202606180003
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 (non-blocking) |

**Verdict**: Approve

---

## Per-assignment checklist (qc-specialist-2 focus)

- [x] Author identity gate on `creator kb rescan`: cross-author attempt returns 403 using `WORLD_KB_FORBIDDEN`
  - `require_world_owner` (rescan.rs:427) re-exports and emits the same `WORLD_KB_FORBIDDEN_CODE` as T-B P0/P1 world kb paths.
  - Test `rescan_cross_author_returns_403` asserts status 403 + code presence; no rows written on failure.
- [x] Idempotent upsert via composite key
  - `upsert_pending_candidate` (kb_extract_job.rs:579) keys on the DB uniqueness `(creator_id, work_entry_id=canonical_name_guess, world_id) WHERE promotion_status IN ('pending','confirmed')`.
  - Confirmed rows are terminal (returns `Unchanged`).
  - Cross-chapter reuse of the same name refreshes `source_chapter_id` rather than duplicating (test `upsert_never_duplicates_across_chapters_for_same_name`).
  - No explicit outer transaction on the read-check-update; the unique index is the final guard. Acceptable for local CLI; re-runs are safe.
- [x] Delta-write helper `compute_kb_diff` correctness for deletions
  - Unmatched active rows (not present in the new extraction) are collected into `removed` (extract_sync.rs:115–119).
  - Test `compute_diff_removed_advisory_when_name_vanishes` and `rescan_after_chapter_edit_updates_candidate_rows` cover the case.
- [x] `--dry-run` flag: no side effects
  - `dry_run_shows_diff_without_writing` asserts that after a dry-run, `list_pending_for_world` and `list_by_world` are both empty.
  - `sync_candidates` and `sync_kb_rows` short-circuit to pure preview / `compute_kb_diff` when `dry_run=true`.
- [x] Stable error codes
  - `WORLD_KB_FORBIDDEN_CODE` is the single source (re-exported from world/kb.rs); all cross-author paths (adopt, edit, delete, rescan) use it.
- [x] R-V150KBED-06 (migration renumber)
  - Original colliding `202606180002` for the partial index was renumbered to `202606180003_works_schedule_json_partial_idx.sql`.
  - Content uses `CREATE INDEX IF NOT EXISTS` (idempotent).
  - The KB-extend migration retains `202606180002_kb_extract_jobs_extend.sql`.
  - Regression suites that were previously blocked now pass (`world_kb_promotion_cli` 11/11, `kb_extract_jobs_migration` 8/8).
- [x] Test coverage
  - 6 DAO + 8 CLI + 19 regression (11+8) tests executed and green in this review.
  - Coverage includes: idempotent re-run, edit-driven upsert+remove, dry-run isolation, cross-author 403, confirmed-is-terminal, cross-chapter reuse, body refresh of confirmed KeyBlocks, stale cleanup only affecting pending rows, malformed/missing target errors.

All security + correctness items from the assignment are satisfied. No Critical or unresolved Warning findings. Regression paths that were broken by the duplicate migration are now green.
