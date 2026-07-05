---
report_kind: qc-review
reviewer: "@qc-specialist-3"
reviewer_index: 3
focus: performance-reliability
plan_id: 2026-06-10-v1.41-selection-pool
verdict: Approve
generated_at: 2026-06-10T23:59:43+08:00
review_range: "merge-base: 55689706 → tip: 97470073"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
files_reviewed: 12
tools_run:
  - cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings
  - cargo +nightly fmt --all -- --check
  - cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db
  - cargo test -p nexus-daemon-runtime --test selection_pool
---

# Code Review Report — V1.41 P1 (qc3)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: openai/k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-10T22:59:00+08:00

## Scope
- plan_id: 2026-06-10-v1.41-selection-pool
- Review range / Diff basis: merge-base: 55689706 → tip: 57f573ad
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 12
- Tools run: cargo clippy, cargo +nightly fmt --check, cargo test, manual review

## Findings

### 🔴 Critical
*None.*

### 🟡 Warning

1. **Inspiration MD scaffold can land in the wrong directory when the workspace path is unset**  
   `add_inspiration` uses `state.workspace_path().unwrap_or_default()` and then `Path::new("")`, so if `workspace_path` is `None` the file is written relative to the daemon's current working directory. In production the daemon's CWD is not guaranteed to be `~/.nexus42/`, so inspiration files can end up in an unintended directory. This is the same concern already registered as `R-V141P1-N06` but it is reachable through a normal API call when workspace initialization is delayed or misconfigured.  
   **→ Reject the request with a 500/PreconditionFailed when `workspace_path` is missing, or resolve the path via `nexus-home-layout` before calling the DAO.**

   - Finding ID: F-001
   - Source Type: manual-reasoning
   - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1420`, `crates/nexus-local-db/src/inspiration_items.rs:140`
   - Confidence: High

2. **`promote_inspiration_handler` is not atomic: Work + pool row can succeed while inspiration row update fails**  
   The handler (1) creates a `Work`, (2) inserts a pool entry, and (3) updates the inspiration item to `promoted`. Steps 1 and 2 are independent DB calls; if step 3 fails, the user has a Work and an active pool entry but the inspiration item still shows `status = 'idea'`.  
   **→ Wrap the three writes in a single `Transaction` and roll back if any step fails. If transaction scope is too invasive for this slice, at least delete the Work/pool row on inspiration-update failure.**

   - Finding ID: F-002
   - Source Type: manual-reasoning
   - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1558-1585`
   - Confidence: High

3. **`mark_work_completed` pool-row update is best-effort and not transactional with the Work patch**  
   `works::patch_work` is awaited first; `mark_pool_entry_completed_for_work` is called afterwards and only warns on error. If the pool update fails (e.g., transient lock timeout), the Work is `completed` but the pool row may still be `active`, so the user will see a completed Work as the current active selection.  
   **→ Move the pool update into the same transaction as the Work patch, or make `mark_work_completed` return an error when the pool update fails so the caller can retry.**

   - Finding ID: F-003
   - Source Type: manual-reasoning
   - Source Reference: `crates/nexus-orchestration/src/auto_chain.rs:263-292`
   - Confidence: High

4. **`repeated_sweeps_remain_stable` is flaky in `master_decision_timeout`**  
   In the scoped test run the test failed 2 of 3 times; it also failed during the full `cargo test` run. The root cause is the same millisecond-precision ID collision that was fixed for ACH IDs in `R-V139P0-W-B`: `enqueue_review_master_schedule` mints `RVM{timestamp%3f}` schedule IDs, so two sweeps in the same millisecond collide on the `creator_schedules` primary key and the second INSERT fails silently.  
   **→ Append a per-process monotonic counter to `RVM` IDs (mirror the ACH fix in `enqueue_auto_chain_schedule`).**

   - Finding ID: F-004
   - Source Type: cargo-test
   - Source Reference: `crates/nexus-orchestration/src/auto_chain.rs:532`, `crates/nexus-daemon-runtime/tests/master_decision_timeout.rs:258-274`
   - Confidence: High

5. **Synchronous filesystem I/O runs inside async handlers / supervisor paths**  
   `create_inspiration_with_scaffold` (`std::fs::write` + `rename`) and `write_completion_lock_for_work` (`completion_lock::write_completion_lock`) perform blocking disk operations on the async runtime thread. On slow disks or network mounts this can stall the supervisor tick or HTTP response path.  
   **→ Use `tokio::fs` or `spawn_blocking` for the tmp+rename and lock-file writes.**

   - Finding ID: F-005
   - Source Type: manual-reasoning
   - Source Reference: `crates/nexus-local-db/src/inspiration_items.rs:146-168`, `crates/nexus-orchestration/src/schedule/supervisor.rs:513`
   - Confidence: Medium

6. **Missing covering index for status-filtered list queries on new tables**  
   - `inspiration_items` has `idx(creator_id)` and unique `idx(creator_id, rel_path)`; `list_inspiration(creator_id, status)` filters on `status` after the unique index.  
   - `novel_pool_entries` has a partial unique index on `creator_id WHERE status='active'`; queries with `status='queued'|'completed'|'archived'` or the `ORDER BY status, updated_at DESC` list path will scan or filter.  
   At the expected local-first scale this is not catastrophic, but it is a hot list path with no upper bound.  
   **→ Add `CREATE INDEX idx_{table}_creator_status ON {table}(creator_id, status)` for both tables (and consider `updated_at DESC` if the sort becomes expensive).**

   - Finding ID: F-006
   - Source Type: manual-reasoning
   - Source Reference: `crates/nexus-local-db/migrations/202606100003_v141_inspiration_items.sql:24-28`, `crates/nexus-local-db/migrations/202606100002_v141_multi_work_locks.sql:27-29`, `crates/nexus-local-db/src/inspiration_items.rs:57-83`, `crates/nexus-local-db/src/novel_pool_entries.rs:57-83`
   - Confidence: Medium

### 🟢 Suggestion

1. **Pool and inspiration CLI list commands buffer the entire result set in memory**  
   `handle_pool_list` and `handle_inspiration_list` deserialize the full JSON array before printing. At 10,000+ entries this is an unbounded memory growth surface.  
   **→ Consider adding `limit`/`offset` query parameters and streaming/iterative printing, or cap the unbounded fetch.**

2. **Inspiration slug collision returns an error instead of auto-suffixing**  
   Two titles that slug to the same string produce a constraint-violation error. A friendlier UX would append `-2`, `-3`, etc.  
   **→ Add an auto-suffix loop in `create_inspiration_with_scaffold` when the file or unique index collides.**

3. **Add structured tracing for pool and inspiration mutations**  
   `add_inspiration`, `promote_pool_entry`, and `promote_inspiration_handler` emit no `tracing::info!` spans. For production debugging, log the item/work IDs and path outcomes at info level (mirroring the `mark_work_completed` pattern).  
   **→ Add `tracing::info!` spans for create/promote/archive outcomes.**

4. **Help text for `creator works pool` is long but complete**  
   The subcommand tree is functional and `--help` output is complete. No hidden assumptions were found. A future UX polish could split the help per `PoolAction`, but this is non-blocking.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 6 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

Rationale: there are unresolved Warning-level findings that affect production reliability (CWD-relative MD scaffold, non-atomic inspiration promotion, best-effort pool completion update) and a flaky test that fails CI intermittently. Once W-1 through W-4 are addressed (or explicitly accepted as tracked residuals with PM sign-off), this review can move to Approve.

## Revalidation (fix-wave delta: f5dd727f..97470073)

**Reviewer**: @qc-specialist-3 (qc-specialist-3, reviewer_index: 3)
**Re-review timestamp**: 2026-06-10T23:59:43+08:00
**Re-review range**: `merge-base: 55689706` → `tip: 97470073` (focus delta `f5dd727f..97470073`)
**Working branch (verified)**: iteration/v1.41
**Review cwd (verified)**: /Users/bibi/workspace/organizations/42ch/nexus
**Tools run**: cargo clippy, cargo +nightly fmt --check, cargo test, cargo test --test selection_pool, manual review of fix-wave diff

### Disposition

| Finding | Original severity | New severity | Disposition | Evidence |
|---------|-------------------|--------------|-------------|----------|
| F-001 (CWD-relative MD scaffold) | warning | resolved | commits 41b1336e + 00394507 (Pool/Ideas/ + nexus-home-layout helper + reject empty) | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1448-1453` constructs workspace_dir from `state.nexus_home()` (no longer `workspace_path().unwrap_or_default()`); `crates/nexus-local-db/src/inspiration_items.rs:179` writes to `Pool/Ideas/{slug}.md`; test `test_inspiration_add_creates_md_and_db_row_atomically` passes |
| F-002 (non-atomic promote) | warning | resolved | commit d7ed04de (inspiration_promote_atomic) | `crates/nexus-local-db/src/inspiration_items.rs:265-379` wraps Work insert + pool promote + inspiration update in single tx (`BEGIN` → `COMMIT` at :371); handler at `works.rs:1609` calls it; test `test_promote_inspiration_atomicity_on_step3_failure` passes |
| F-003 (best-effort pool update) | warning | resolved | commit 8cc1eaba (tracing::error + lock clear) | `crates/nexus-orchestration/src/auto_chain.rs:287-308` logs `tracing::error!` and clears `completion_locked_at` via `WorkPatch` on pool update failure so supervisor retries |
| F-004 (pre-existing flake) | warning | out-of-scope | confirmed pre-existing R-V141P1-18; V1.41 P-last target | No changes in fix-wave delta; qc-consolidated.md |
| F-005 (sync I/O in async) | warning | resolved | commit e02b99f5 (spawn_blocking for MD writes) | `crates/nexus-local-db/src/inspiration_items.rs:187` wraps `std::fs::create_dir_all`, `write`, and `rename` in `tokio::task::spawn_blocking`; rollback cleanup at :220 also uses `spawn_blocking` |
| F-006 (covering index) | warning | resolved | migration 202606100004 + index asserts | `crates/nexus-local-db/migrations/202606100004_v141_pool_inspiration_indexes.sql` creates `novel_pool_entries_by_creator_status` (creator_id, status, updated_at DESC) and `inspiration_items_by_creator_status` (creator_id, status, created_at DESC); note: no explicit runtime test asserts index presence, but migration is committed and list queries pass |

### Suggestions (forward-looking; deferred to V1.42 per qc-consolidated.md residuals)

| ID | Status | Note |
|----|--------|------|
| S-1 (list buffering CLI) | resolved | closed by 45cc8d22 (pagination + count) |
| S-2 (slug collision auto-suffix) | defer | R-V141P1-13 (V1.42 UX) |
| S-3 (observability tracing) | defer | R-V141P1-15 (V1.41 P-last) |
| S-4 (CLI help polish) | defer | R-V141P1-14 (V1.42) |

### New findings (if any)

None.

### Tools / verification tails

```
$ cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.49s

$ cargo +nightly fmt --all -- --check
(no output)

$ cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db
... (all passed; 47 + 15 + 2 + 1 + 4 + 2 = 71 tests total)

$ cargo test -p nexus-daemon-runtime --test selection_pool
running 13 tests
test test_archive_inspiration_rejects_cross_creator ... ok
test test_pool_archive_marks_archived ... ok
test test_completion_demotes_active_pool_row_when_completed ... ok
test test_archive_pool_rejects_cross_creator ... ok
test test_pool_list_returns_all_statuses ... ok
test test_completion_updates_pool_row ... ok
test test_pool_promote_demotes_prior_active ... ok
test test_pool_promote_idempotent_on_same_target ... ok
test test_inspiration_promote_creates_work_and_pool_row ... ok
test test_inspiration_add_rejects_existing_path ... ok
test test_inspiration_add_creates_md_and_db_row_atomically ... ok
test test_promote_inspiration_atomicity_on_step3_failure ... ok
test test_promote_inspiration_rejects_cross_creator ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Updated verdict

Approve

**Rationale**: All 5 actionable Warning findings from the original review are resolved in the fix-wave delta. F-004 remains out-of-scope per qc-consolidated.md. No new Critical or Warning items appear. Clippy, nightly fmt, and all tests pass (including 13 selection_pool integration tests).
