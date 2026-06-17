---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.50-cron-brainstorm-write
verdict: Approve
generated_at: 2026-06-17T11:14:17Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security + correctness (focus on CAS/TOCTOU closure, gating read safety, cron evaluator abuse surface, test fidelity for concurrent-writer contract, stable errors, and daemon crash-loop resistance)
- Report Timestamp: 2026-06-17T11:14:17Z

## Scope
- plan_id: 2026-06-18-v1.50-cron-brainstorm-write
- Review range / Diff basis: merge-base 0ea2995ff45569b541b17097c4c919dabab4bb16..f16daaddf616583e1ee85f2a9cfa8c6db7f15b18
- Working branch (verified): feature/v1.50-cron-brainstorm-write
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-brainstorm-write
- Files reviewed: 16 (plan + status + 14 source files)
- Commit range: 0ea2995ff45569b541b17097c4c919dabab4bb16..f16daaddf616583e1ee85f2a9cfa8c6db7f15b18
- Tools run: git diff, grep, cargo test (hermetic suite), cargo clippy -p (touched crates)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-001 (test fidelity)**: The four `set_schedule_json_tx_*` tests (`rejects_stale_preimage`, `applies_on_matching_preimage`, `concurrent_writers_serialise`, `missing_work_errors`) are sequential simulations of the race (Writer A reads, Writer B mutates via helper, A’s CAS fails, A retries). They correctly exercise the CAS contract and the re-read disambiguation logic inside the transaction. True multi-threaded writers are not required for SQLite (single writer at the OS level); the logical race + re-read inside tx is the right model. Consider a brief comment in the test module stating “logical race simulation per SQLite single-writer model” so future readers do not mis-classify them as “only sequential”.
- **S-002 (defensive)**: `has_active_role_schedule` builds the IN list via `format!` with a module constant (`ACTIVE_STATUS_LIST`). The value is not derived from user input, so no injection surface exists. If the status list ever becomes dynamic, it must be turned into repeated bound parameters. Current usage is safe.

## Detailed Security + Correctness Analysis (per assignment focus)

### 1. `set_schedule_json_tx` CAS — SQL binding and TOCTOU closure
- Implementation: `UPDATE works SET schedule_json = ?, updated_at = ? WHERE work_id = ? AND COALESCE(schedule_json, '') = ?` inside a caller-supplied `&mut Transaction`.
- Normalization: Both sides treat NULL and '' as “unset” (COALESCE on the column, `.unwrap_or("")` on the expected value passed by caller). Matches spec §2.3.
- Atomicity: The read-check and UPDATE are a single statement under the transaction → the window that existed in the old `get → apply → set` is closed (not merely shrunk).
- Zero-row handling: Re-SELECT inside the same tx to distinguish “Work missing” (returns `MissingVersionKey`) from “CAS mismatch” (returns `Ok(false)`). Callers can then decide retry vs surface.
- CLI path (`handle_set`): reads `stored`, computes new blob, opens tx, calls `_tx` with `stored.as_deref()` as expected, on `!applied` does explicit rollback + `CliError::Config` with a clear retry message. No silent lost update.
- Daemon path: the cron evaluator **never writes** `schedule_json` (only reads via `list_works_with_schedule_json`). The only writer that can race is another CLI or a future admin path; both are now forced through the CAS.

**Conclusion**: R-V150P0-W5 is resolved. The TOCTOU is closed.

### 2. All live writers now use the atomic variant
- Production write to `schedule_json` from the CLI is now exclusively `set_schedule_json_tx` (inside tx).
- The old non-transactional `set_schedule_json` remains in the crate for test fixtures and migration tests — those call sites are not part of the daemon-vs-CLI race surface.
- No other production call site to the non-tx writer was found in the diff or via grep for live code.

### 3. Per-Work gating read paths (`intake_status`, `runtime_lock_holder`, `completion_locked_at`)
- Source: `list_works_with_schedule_json` — a simple `SELECT … FROM works WHERE schedule_json IS NOT NULL AND schedule_json != ''` backed by the partial index.
- Columns are read into `WorkCronRow` and then inspected by pure Rust `gate_reason`. No string interpolation of these fields into SQL.
- The scan predicate uses only the partial-index column; no user-controlled values are concatenated.
- `has_active_role_schedule` for idempotency uses a constant status list inside `IN (...)`. Safe.

No SQL injection or TOCTOU on the read side for gating decisions.

### 4. Cron expression evaluation — malicious schedule_json / CPU spike risk
- `cron_fires_at_minute` normalizes 5-field → 6-field then uses `cron::Schedule::from_str` + `after(&just_before).next() == Some(minute_start)`.
- Parse failures (garbage cron or bad TZ) are caught at Work level in `evaluate_work` → `skipped_parse_error` + warn log; the Work is skipped for that sweep.
- A user-controlled “* * * * *” will legitimately fire every minute — this is the intended semantics for a per-Work cron (spec §2.1 / §4.1). The evaluator does not amplify it into CPU work beyond one `Schedule::after/next` call per role per Work.
- The outer sweep is bounded by the number of Works that have a non-empty `schedule_json` (the partial index scan). No global “all Works” or unbounded fan-out.
- Daemon task (`run_one_tick`, `spawn_cron_supervisor`): every tick is wrapped; errors are logged at warn and the loop continues. A single bad schedule cannot crash-loop the supervisor.

**Conclusion**: Bounded. No ReDoS or spin surface from a malicious cron blob.

### 5. The four hermetic CAS tests
- All four tests are present and pass:
  - `set_schedule_json_tx_rejects_stale_preimage`
  - `set_schedule_json_tx_applies_on_matching_preimage`
  - `set_schedule_json_tx_concurrent_writers_serialise` (the race scenario)
  - `set_schedule_json_tx_missing_work_errors`
- They use the real DAO inside in-memory DBs with explicit tx boundaries. The “concurrent” test simulates the exact lost-update window that the old code had.
- Given SQLite’s single-writer model, true OS-thread contention on the same row is not the failure mode; the logical read-then-CAS-mismatch is. The tests cover the contract that matters.

### 6. Stable error codes and daemon resilience
- CLI CAS mismatch: `CliError::Config` with a stable message containing a retry hint (not a panic or 500-style path).
- Daemon: `evaluate_cron_fires` never panics on bad input; every per-Work or per-role error increments a counter and continues. `run_one_tick` logs but does not propagate.
- `cron_fires_at_minute` returns `false` on parse failure (no panic).
- Validation at `creator works cron set` time (`validate_cron_expr`) still rejects garbage before it is stored.

## Source Trace
- CAS implementation: `crates/nexus-local-db/src/works.rs:1538` (`set_schedule_json_tx`)
- CLI writer: `crates/nexus42/src/commands/creator/works/cron.rs:647` (handle_set now uses `_tx`)
- Evaluator read + gating: `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:128` (`evaluate_cron_fires`, `gate_reason`, `has_active_role_schedule`)
- Daemon task wrapper: `crates/nexus-daemon-runtime/src/cron_supervisor.rs:104` (`run_one_tick`)
- Hermetic CAS tests: `crates/nexus-orchestration/tests/cron_supervisor.rs:400` (the four `set_schedule_json_tx_*` cases)
- Old non-tx writer: still present at `works.rs:1334` but only called from tests/migrations in the reviewed diff.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Residual Context (for PM)
The implementer already closed R-V150P0-W5 in this wave and declared two new residuals in the plan Completion Report (R-V150P1CRONBW-01 medium deferred for novel-write preset; R-V150P1CRONBW-02 low accepted for clippy --all hygiene). No new blocking security or correctness residuals were found by this reviewer.
