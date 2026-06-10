---
report_kind: qc-review
reviewer: "@qc-specialist-3"
reviewer_index: 3
focus: performance-reliability
plan_id: 2026-06-10-v1.41-selection-pool
verdict: Request Changes
generated_at: 2026-06-10T22:59:00+08:00
review_range: "merge-base: 55689706 → tip: 57f573ad"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
files_reviewed: 12
tools_run:
  - cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings
  - cargo +nightly fmt --all -- --check
  - cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db
  - cargo test -p nexus-daemon-runtime --test master_decision_timeout repeated_sweeps_remain_stable (repeated 3x)
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
