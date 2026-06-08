---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-09-v1.39-fl-e-auto-chain-engine"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report — V1.39 P0 FL-E Auto-Chain Engine

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-09T00:00:00Z

## Scope
- plan_id: `2026-06-09-v1.39-fl-e-auto-chain-engine`
- Review range / Diff basis: `merge-base: c7a3fac1` (iteration/v1.39) + `tip: c143da1f` (feature/v1.39-fl-e-auto-chain-engine HEAD); equivalent to `git diff c7a3fac1...c143da1f` (run in the Review cwd). 15 commits, 14 files, +2034 / -54.
- Working branch (verified): `feature/v1.39-fl-e-auto-chain-engine`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p0`
- Files reviewed: 14
- Commit range: `c7a3fac1..c143da1f`
- Tools run: cargo clippy --all -- -D warnings, cargo test -p nexus-orchestration --test auto_chain, cargo test -p nexus-local-db, cargo test -p nexus-daemon-runtime, cargo +nightly fmt --all -- --check

## Findings

### 🔴 Critical
*No Critical findings.*

### 🟡 Warning

**W-1: Non-stage fields and stage advance are not atomic in `patch_work_stage`**

`apply_non_stage_fields` (Fix 3, `crates/nexus-daemon-runtime/src/api/handlers/works.rs:448`) performs a `patch_work` call **before** `advance_work_stage_atomic`. If the stage advance transaction fails (e.g., active FL-E schedule already exists), the non-stage fields (title, brief, status, etc.) have already been committed. The two operations should share a transaction or be reordered so stage validation happens first.

→ **Fix**: Move `apply_non_stage_fields` inside the `advance_work_stage_atomic` transaction, or validate stage gates before applying non-stage fields.

**W-2: Boot resume query lacks index coverage on `works` checkpoint columns**

`find_resumable_works` (`auto_chain.rs:318`) executes:
```sql
SELECT ... FROM works w
WHERE w.auto_chain_enabled = 1
  AND w.driver_schedule_id IS NOT NULL
  AND w.auto_chain_interrupted = 0
  AND w.status != 'completed'
  AND NOT EXISTS (
      SELECT 1 FROM creator_schedules cs
      WHERE cs.schedule_id = w.driver_schedule_id
        AND cs.status = 'running'
  )
```

The migration (`202606090001_works_auto_chain_checkpoint.sql`) adds the three columns but **no indexes**. At 1000+ Works, SQLite will full-scan `works` and do a correlated subquery lookup per row. The per-Work boot loop then does a re-fetch + `evaluate_next_step` + schedule INSERT — linear with no batching.

→ **Fix**: Add `CREATE INDEX works_auto_chain_resume ON works(auto_chain_enabled, auto_chain_interrupted, status) WHERE auto_chain_enabled = 1;` or at minimum index `driver_schedule_id`. Consider batching the boot resume loop (evaluate all, then insert all in a transaction).

**W-3: `tick_inner` loads ALL schedule rows on every terminal event**

`ScheduleSupervisor::tick_inner` (`supervisor.rs:149`) loads the entire `creator_schedules` table:
```rust
let all_rows = sqlx::query_as!(ScheduleRow, "SELECT ... FROM creator_schedules").fetch_all(pool).await?;
```

This is O(N) in total historical schedules, not just pending/running. Every `on_schedule_terminal` → `tick()` cascade pays this cost. For long-running daemons with many completed schedules, this will degrade.

→ **Fix**: Scope the SELECT to `WHERE status IN ('pending', 'running', 'paused')` or add a `WHERE status = 'pending'` filter for the pending set, with a separate query for running schedules.

### 🟢 Suggestion

**S-1: Redundant SSOT re-fetch in `process_auto_chain_after_terminal`**

`process_auto_chain_after_terminal` (`supervisor.rs:361`) calls `find_work_for_driver` (returns `WorkRecord`), then immediately re-fetches the same Work via `get_work` for "SSOT" reasons. This is a second DB round-trip for data already in hand. The justification (race between find and evaluate) is weak because the Work row was just found by the schedule_id; a concurrent mutation is unlikely and would be caught by the subsequent `set_driver` anyway.

→ **Fix**: Remove the redundant `get_work` call and use the record from `find_work_for_driver` directly, or document the race scenario explicitly.

**S-2: `enqueue_auto_chain_step` silently swallows missing preset mapping**

When `build_auto_chain_schedule` returns `None` (unknown stage), `enqueue_auto_chain_step` logs a warning and returns `Ok(())` (`supervisor.rs:475`). This is a configuration/development error that should be more visible — it means a stage in FL_E_STAGES has no schedule mapping.

→ **Fix**: Return an error or at least log at `error!` level so operators notice.

**S-3: Dynamic SQL in `auto_chain.rs` should migrate to compile-time macros**

Both `find_work_for_driver` and `find_resumable_works` use runtime `sqlx::query` with `format!` for column lists. While the values are bound safely, the column list string is not checked at compile time. Per `nexus-daemon-runtime/AGENTS.md` sqlx conventions, static queries should use `sqlx::query!` / `sqlx::query_as!`.

→ **Fix**: After the migration stabilizes and `cargo sqlx prepare` has run, convert these to compile-time macros. Mark with `// TODO(sqlx-macro)`.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| W-1 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:448-500`, `patch_work_stage` call ordering | High |
| W-2 | manual-reasoning | `crates/nexus-local-db/migrations/202606090001_works_auto_chain_checkpoint.sql` (no indexes); `auto_chain.rs:318-338` | High |
| W-3 | manual-reasoning | `crates/nexus-orchestration/src/schedule/supervisor.rs:157-166` (unfiltered SELECT) | High |
| S-1 | manual-reasoning | `crates/nexus-orchestration/src/schedule/supervisor.rs:361-395` (double DB fetch) | Medium |
| S-2 | manual-reasoning | `crates/nexus-orchestration/src/schedule/supervisor.rs:473-482` (None → Ok) | High |
| S-3 | static-analysis | `crates/nexus-orchestration/src/auto_chain.rs:64-74`, `320-338` (runtime query with SAFETY comment) | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

**Rationale**: No Critical findings. The three Warnings are real but do not block correctness or safety:
- W-1 (non-atomic patch) is a partial-update consistency edge case that affects only PATCH handlers, not the core auto-chain flow.
- W-2 (missing index) is a performance concern at scale; SQLite local workloads are unlikely to hit 1000+ Works in the near term, but an index should be added before V1.40.
- W-3 (full table scan on tick) is a supervisor scalability concern that affects all schedules, not just auto-chain; it predates this PR but is made more visible by the increased tick frequency from auto-chain completions.

All six acceptance criteria are met by the diff:
1. AC1 (`intake → research → produce → review → persist` auto-chain): `evaluate_next_step` + `enqueue_auto_chain_step` in `process_auto_chain_after_terminal`.
2. AC2 (chapter outer loop): `evaluate_after_persist` returns `NextChapter` when `current_chapter < total_planned_chapters`.
3. AC3 (Work completion stops enqueue): `evaluate_after_persist` returns `WorkComplete` when `current_chapter >= total_chapters`, and `mark_work_completed` clears the driver.
4. AC4 (daemon restart auto-resumes checkpointed): `find_resumable_works` + boot loop in `boot.rs:226-341`.
5. AC5 (`--note` does not create second driver): `creator run continue` appends inspiration but does not schedule; side-input test covers this (`auto_chain.rs` side-input tests).
6. AC6 (`--no-auto-chain` disables enqueue, checkpoint still written): `no_auto_chain` flag passed to Work creation; `evaluate_next_step` checks `auto_chain_enabled` early.

All verification commands pass:
- `cargo clippy --all -- -D warnings`: clean
- `cargo test -p nexus-orchestration --test auto_chain`: 21 passed in 0.39s
- `cargo test -p nexus-local-db`: 157 passed
- `cargo test -p nexus-daemon-runtime`: 265 passed (1 ignored)
- `cargo +nightly fmt --all -- --check`: clean

The diff stays within P0 scope (no P0.5/P1/P2/P3/P4/P5 creep). T10 (deferred tracker update) is correctly reflected in the tracker diff.

## Top 3 Performance/Reliability Observations

1. **Good**: Auto-chain errors are logged but do not propagate — the terminal transition always completes. This is the right default for reliability; a failed auto-chain enqueue must not orphan a completed schedule.
2. **Good**: The boot resume loop evaluates each Work independently and logs per-Work outcomes, making post-restart behavior observable.
3. **Concern**: The supervisor `tick()` loads all schedules on every completion, which will become a bottleneck as schedule history grows. This is not auto-chain-specific but is exacerbated by the higher completion frequency auto-chain creates.

## Clarification Needed from PM

None — the scope is clear and the implementation matches the plan.
