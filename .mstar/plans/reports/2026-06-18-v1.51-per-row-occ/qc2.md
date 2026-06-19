---
report_kind: qc_review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.51-per-row-occ
verdict: Approve
generated_at: 2026-06-19T12:00:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (CAS atomicity, version monotonicity, acquire-order discipline, retry safety, E_VERSION stability, migration safety, transaction boundaries, injection surface)
- Report Timestamp: 2026-06-19T12:00:00Z

## Scope
- plan_id: 2026-06-18-v1.51-per-row-occ
- Review range / Diff basis: iteration/v1.51...HEAD (= 008294327a8a33714948eb6d810794d338ceaa93...e988291a1e9290de4ec3f586de32455fa15e7788)
- Working branch (verified): feature/v1.51-per-row-occ
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-b-p1
- Files reviewed: 15 (diff stat: 5 new, 9 modified, ~1479 insertions)
- Commit range: 00829432...e988291a (2 commits in range)
- Tools run: cargo test (cas_migration_roundtrip, cli_version_error, kb_adopt_cas, regression suites), cargo clippy -p nexus-local-db -p nexus42 -p nexus-orchestration -- -D warnings, git diff / log, manual source trace of CAS paths

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-001: E_VERSION surfacing loses actual version detail in primary CLI path**  
  `kb_adopt` maps `LocalDbError::VersionMismatch { actual: Some(v), .. }` to `CliError::VersionConflict { actual_version: None, .. }` (kb.rs:549).  
  The documented contract (plan §2, concurrency.md §7.6, completion report) is to surface "row was modified by another writer (holder=<id>); retry".  
  The `cas_check` path correctly re-reads `actual`, but the adoption error path discards it. Tests only cover the `None` → "?" display case.  
  Result: operator sees expected version but not the conflicting writer's actual version for the row.  
  → **Fix**: thread `actual` from `VersionMismatch` into `VersionConflict` (or add a dedicated variant) in the kb.rs mapping and update display to include it when present.  
  Source: crates/nexus42/src/commands/creator/world/kb.rs:541-554, errors.rs:155, main.rs:98, cas.rs:74-79.

- **W-002: Prescribed test target in assignment/plan does not exist**  
  Assignment workflow and plan verification section mandate:  
  `cargo test -p nexus-daemon-runtime --test cron_cas_retry`  
  No such test target exists (available targets listed in output; retry logic lives in `cas.rs` unit tests + `cron_supervisor` regression).  
  While core retry behavior is covered (cas.rs tests for `with_cas_retry`, supervisor loop passes regression), the exact command in the gate docs is unexecutable. This is a documentation/artifact mismatch.  
  → Update plan/assignment or add a thin integration test target alias for the cron CAS path.

### 🟢 Suggestion
- **S-001: Runtime SQL for trusted identifiers in `cas_check` re-read**  
  `cas_check` builds `SELECT version FROM {table} WHERE {id_column} = ?` via `format!` then binds the value.  
  Callers pass only internal constants ("kb_extract_jobs", "job_id", etc.), never untrusted input, so no SQL injection. Still, this is the only runtime (non-`query!`) path in the CAS module.  
  → Consider a small enum or const assertions for the two tables to keep the re-read path fully static where possible (maintainability).

- **S-002: `novel_pool_entries` CAS is infrastructure-only in this plan**  
  Column added with DEFAULT 0; `promote_to_active` etc. do not yet pass/use preimage version (explicitly noted as V1.52+).  
  The CAS helper and migration are ready, but no call-site yet exercises the guard for pool entries. Ensure future writers (cross-author pool contention under V1.41 multi-work switch) actually read-then-CAS.  
  Documented correctly in completion.md and concurrency.md §7.2, but worth a future-plan cross-check.

## Source Trace
- **CAS atomicity & version guard**: `cas_check` (cas.rs:54) after `UPDATE ... AND version = ?`; `mark_confirmed_in_tx_with_cas` (kb_extract_job.rs:1007) does the guarded UPDATE + disambiguation re-read inside the caller's tx.
- **Parameterized queries (injection closed)**: All CAS UPDATEs and the re-read in `cas_check` use `.bind()` for id/value; only table/column names are formatted from trusted constants. No user-controlled strings reach SQL text.
- **Version monotonicity**: Every mutating path does `version = version + 1` (cas.rs:169, kb_extract_job.rs:1016, tests). INSERTs receive DEFAULT 0. No reset-to-zero code in diff.
- **Acquire-order discipline (file lock → DB → CAS)**: 
  - kb_adopt: `try_acquire` (kb.rs:476) before `pool.begin()` (kb.rs:527) then CAS inside tx (kb.rs:541).
  - cron_fire: `maybe_acquire_cron_file_lock` (cron_supervisor.rs:348) before enqueue loop (which will contain CAS writes when T-A paths activate). Guard held until after enqueue (line 414).
  - Spec reference: concurrency.md §2.4, §7.5.
- **Deadlock / nested lock**: No two file locks acquired simultaneously. File lock is per-Work. CAS executes inside the single held `FileLockGuard`. No reverse-order acquisition (T-B P0 file lock always outer).
- **Cron-side retry safety**: `with_cas_retry` (cas.rs:102) — explicit `for attempt in 1..=max_attempts` (default 3), fixed backoff, `warn!` on each VersionMismatch retry, returns first non-retryable error or VersionMismatch after exhaustion. Inline loop in cron_supervisor.rs:371 also caps at 3/100ms. No unbounded loop.
- **E_VERSION stable path**: `LocalDbError::VersionMismatch` → `CliError::VersionConflict` (kb.rs:544) → exit 76 (main.rs:98-99). Display includes table/row/expected (actual may be "?"). Exit code distinct from E_LOCK (75).
- **Migration safety**: `202606190001_kb_extract_jobs_and_pool_version.sql` — two `ALTER TABLE ... ADD COLUMN version INTEGER NOT NULL DEFAULT 0`. SQLite allows this for new columns; existing rows get 0. Roundtrip test (`cas_migration_roundtrip`) verifies.
- **Transaction boundaries**: Adopt path uses single `tx` for `insert_key_block_in_tx` + `mark_confirmed_in_tx_with_cas`. On CAS failure (including version mismatch) the mapping returns Err and the tx is dropped (or explicit rollback in the `!flipped` arm). No orphan KeyBlock.
- **novel_pool_entries**: Column present; CAS call-sites deferred (completion.md). No current write path in scope mutates it under CAS.
- **Tests executed (all PASS)**:
  - `cargo test -p nexus-local-db --test cas_migration_roundtrip` (5/5)
  - `cargo test -p nexus42 --test cli_version_error` (4/4)
  - `cargo test -p nexus42 --test kb_adopt_cas` (4/4)
  - Regression: `nexus-local-db --lib`, `file_lock`, `cli_lock_contention`, `cron_supervisor`, `nexus-orchestration --lib -- llm`
  - `cargo clippy --all -- -D warnings` (clean on changed crates)
  - `cargo +nightly fmt --all --check` (PASS per completion report)
- **Spec / plan cross-check**: concurrency.md §7 (OCC), world-kb-runtime-architecture.md §6.1, plan acceptance criteria 1-6, acquire-order statements in completion report.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Additional Notes
- Core security invariants (CAS atomicity under parameterized WHERE, file-lock → CAS ordering, bounded retry, monotonic version, safe additive migration, transactional adopt) are correctly implemented and tested.
- The two Warnings are narrow: one observability gap in error reporting for the documented user message, and one test-target naming mismatch with the assignment text. Neither introduces injection, deadlock, lost-update, or unbounded-retry risk.
- No deviations from T-B P0/T-B P1 acquire-order discipline were found. No reverse lock acquisition introduced.
- CI-equivalent checks (clippy -D warnings, tests) passed for the changed crates.

## Revalidation (2026-06-19)

- **Resolved: W-001 (qc2, actual_version discarded)** — Changed `kb_adopt` error mapping from `matches!(...VersionMismatch { .. })` to `if let ...VersionMismatch { actual, .. } = &e`, capturing `actual` from the `VersionMismatch` and threading it into `VersionConflict { actual_version: *actual }` (kb.rs:544). Added two hermetic tests in `kb_adopt_cas.rs`: `test_version_conflict_surfaces_actual_version_in_error_message` (asserts "actual v3" in user-visible message) and `test_version_conflict_without_actual_displays_question_mark` (asserts "?" fallback). Updated `test_kb_adopt_stale_preimage_returns_version_conflict` to exercise the concurrent-writer race path.
- **Resolved: W-002 (qc2, missing test target)** — Created `crates/nexus-daemon-runtime/tests/cron_cas_retry.rs` with 3 tests: `test_cron_cas_happy_path` (CAS succeeds first try), `test_cron_cas_retry_succeeds_after_version_mismatch` (CAS fails first, succeeds on retry), `test_cron_cas_exhaustion_returns_version_mismatch` (all 3 retries fail, returns `VersionMismatch` with populated `actual`). All 3 pass. `cargo test -p nexus-daemon-runtime --test cron_cas_retry` is now executable.
- **Evidence**: Commits `<impl-commits>`; tests: `cargo test -p nexus-daemon-runtime --test cron_cas_retry` (3/3), `cargo test -p nexus42 --test kb_adopt_cas` (6/6, +2 new), `cargo test -p nexus42 --test cli_version_error` (4/4), `cargo test -p nexus-local-db --test cas_migration_roundtrip` (5/5), `cargo test -p nexus-local-db --test file_lock` (3/3), `cargo test -p nexus42 --test cli_lock_contention` (3/3), `cargo test -p nexus-orchestration --test cron_supervisor` (22/22), `cargo clippy --all -- -D warnings` (clean), `cargo +nightly fmt --all --check` (clean)
- **Re-verdict**: Approve
