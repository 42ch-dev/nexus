---
report_kind: qc_review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.51-cross-chapter-rescan
verdict: Approve
generated_at: 2026-06-18T17:14:27Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-18T17:14:27Z

## Scope
- plan_id: 2026-06-18-v1.51-cross-chapter-rescan
- Review range / Diff basis: iteration/v1.51...HEAD (= 00829432...3d7c1f23)
- Working branch (verified): feature/v1.51-cross-chapter-rescan
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p1
- Files reviewed: 9 (diff stats: +1751/-22)
- Commit range: 008294327a8a33714948eb6d810794d338ceaa93..3d7c1f23c2de31466d3c87ae2e789f08981d1fe2
- Tools run:
  - `git diff iteration/v1.51...HEAD`
  - `cargo test -p nexus42 --test kb_rescan` (11 passed)
  - `cargo test -p nexus-orchestration --lib quality_loop::tests::aggregate_` (8 passed)
  - `cargo test -p nexus-local-db --test kb_extract_jobs_upsert` (6 passed)
  - Source trace on `resolve_work`, `require_world_owner`, `acquire_work_lock`, `sync_work_candidates`, `upsert_pending_candidate`, CLI dispatch in `kb.rs`

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- S-01: The work-scoped rescan path does not wrap the full reconciliation (lock + multiple upserts + kb diff_and_apply) in an explicit outer `BEGIN IMMEDIATE` transaction. Each `upsert_pending_candidate` and `diff_and_apply` call is individually atomic (V1.50 precedent), and the advisory file lock serializes writers across processes. This is consistent with the chapter-scoped path and acceptable for the current scope, but a future T-B P1 CAS upgrade may benefit from a larger transactional boundary for observability. Not a correctness or security defect for this plan.
- S-02: `source_chapters` provenance is carried inside `proposed_payload` JSON (additive) rather than a dedicated column. The dedicated `source_chapter_id` column holds only the lowest chapter for DB uniqueness. This matches the documented design (world-kb-runtime-architecture.md §5.5.1) and is queryable via JSON. If future analytics require indexed cross-chapter lookup, a dedicated column + index could be added without breaking the uniqueness contract. No residual opened.

## Source Trace

**F-01 — work_ref path construction (path traversal)**
- Source: `crates/nexus42/src/commands/creator/kb/rescan.rs:345` (`kb_rescan_work_hermetic`)
- Trace:
  1. CLI `--work <work_ref>` (clap `work: Option<String>`)
  2. `resolve_work(pool, creator_id, work_ref)` — parameterized SELECT on `works` table by `work_ref OR story_ref OR work_id`
  3. Only on success: `let work_dir = ws_dir.join("Works").join(work_ref);` + `acquire_work_lock(&work_dir)`
  4. Chapter bodies: `work_chapters::list_chapters` (DB) → `body_path` (relative from DB) → `ws_dir.join(body_rel)`
- No raw user input reaches any `join` or `std::fs` path operation. `work_ref` is validated by DB existence before any filesystem access.
- Cross-author: `require_world_owner` (world ownership check) runs before extraction or lock.
- Verdict: PASS. Matches assignment acceptance focus.

**F-02 — Cross-chapter upsert idempotency (same canonical → 1 row)**
- Source: `quality_loop.rs: aggregate_candidates_by_canonical_name` + `rescan.rs: sync_work_candidates`
- Trace:
  1. `extract_per_chapter` → per-chapter `KbCandidate` vecs
  2. Pure `aggregate_candidates_by_canonical_name` (case-insensitive key, first-seen case preserved, `source_chapters` array injected into payload)
  3. One call to `upsert_pending_candidate(..., work_id, source_chapter_id=first, ...)` per aggregate
  4. `upsert_pending_candidate` uses the V1.50 uniqueness `(creator_id, work_entry_id=canonical_name, world_id)` + `promotion_status IN ('pending','confirmed')`
- Test evidence: `cross_chapter_same_entity_collapses_to_one_pending_row` (3 chapters → 1 row, `source_chapters:[1,2,3]`); distinct entities → 3 rows.
- Stale cleanup: `delete_pending_for_chapter_work` (work_id + canonical_name, only pending rows).
- No orphan rows on failure: advisory lock + per-call atomicity; partial failure would leave prior aggregates committed (same as chapter-scoped path).
- Verdict: PASS. DB uniqueness + aggregation contract holds.

**F-03 — Author identity gate**
- Source: `rescan.rs:361` (`require_world_owner(pool, &world_id, creator_id)`)
- Same implementation and error path (`CliError::Api { status: 403, ... WORLD_KB_FORBIDDEN }`) as V1.50 chapter-scoped and T-B P0 adopt paths.
- Called after work resolution, before any extraction, lock, or upsert.
- Test: `cross_chapter_cross_author_returns_403`.
- Verdict: PASS. No bypass.

**F-04 — DB write safety (parameterized SQL, no partial orphans)**
- `upsert_pending_candidate`: static SELECT + conditional INSERT/UPDATE with bound parameters. No string concatenation.
- `delete_pending_for_chapter_work`: static DELETE with two binds, scoped to `promotion_status = 'pending'`.
- `sync_work_kb_rows`: uses existing `diff_and_apply` (V1.50) which is atomic per KeyBlock.
- No outer transaction around the whole rescan (consistent with prior art); the file lock provides cross-process serialization.
- Verdict: PASS for this scope.

**F-05 — Lock acquire-order discipline (T-B P0 §2.4)**
- Source: `rescan.rs:426` (non-dry): `_file_lock = Some(acquire_work_lock(&work_dir)?)` before `sync_work_candidates` and `sync_work_kb_rows`.
- Dry-run: `None` (read-only, no lock acquired).
- `acquire_work_lock` maps `FileLockError::Locked` → `CliError::Locked` (exit 75) and `Io` → `CliError::LockIo` (exit 78) — dual exit-code contract preserved.
- Test: `cross_chapter_lock_contention_returns_e_lock`, `cross_chapter_lock_io_failure_returns_e_lock_io`, `cross_chapter_dry_run_succeeds_under_lock_contention`.
- Verdict: PASS. File lock before any DB mutation.

**F-06 — Concurrent multi-Work safety**
- Same advisory lock primitive as T-B P0 (`Works/<work_ref>/.lock` via `nexus_local_db::file_lock::try_acquire`).
- Contention on the same work during rescan → E_LOCK 75.
- Different works: independent locks.
- Verdict: PASS.

**F-07 — CLI surface integrity (mutual exclusivity)**
- Source: `crates/nexus42/src/commands/creator/kb.rs:244` (dispatch)
- Clap: `target: Option<String>` (positional) + `#[arg(long)] work: Option<String>`
- Match:
  - `(Some(t), None)` → chapter path
  - `(None, Some(w))` → work path
  - `(Some, Some)` → explicit error: "Specify either ... not both."
  - `(None, None)` → explicit error: "Specify either ... or --work ..."
- No way for both paths to be invoked from the same command line.
- Verdict: PASS.

**F-08 — No bypass of V1.50 chapter-scoped guardrails**
- Chapter-scoped code (`kb_rescan`, `kb_rescan_hermetic`, `sync_candidates`, `parse_target`, etc.) is untouched.
- V1.50 regression tests (`kb_rescan_cli` 8 tests) continue to pass.
- Verdict: PASS.

**F-09 — R-V150KBED-08 closure**
- Source: `.mstar/status.json` under `residual_findings["2026-06-18-v1.50-kb-refreshable-scan"]`
- Entry `R-V150KBED-08`:
  - `lifecycle: "resolved"`
  - `closed_at: "2026-06-18"`
  - `closure_evidence`: detailed (plan, commits, 27 test names, spec references)
  - `resolution: { plan_id: "2026-06-18-v1.51-cross-chapter-rescan", commit: "..." }`
- Pre-existing open residual `R-V151-MERGE-CLIPPY-01` (medium, pre-existing on base, not caused by T-A P1) is correctly left open and routed to P-last.
- Verdict: PASS (closure evidence present and accurate).

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

All security and correctness acceptance criteria from the assignment are satisfied:
- `work_ref` never reaches a path join without prior DB validation.
- Cross-chapter upsert collapses to one row per canonical entity via existing DB uniqueness.
- Author gate is enforced on the adopt path.
- Advisory lock is acquired before any DB write (dry-run exempt).
- Concurrent contention produces the documented E_LOCK 75.
- CLI mutual exclusivity is enforced at dispatch.
- V1.50 chapter-scoped path is unchanged.
- R-V150KBED-08 is closed with evidence in `status.json`.

The two suggestions are design notes for future evolution (T-B P1 CAS, provenance column) and do not rise to Warning or Critical for this plan. No unresolved findings block approval.
