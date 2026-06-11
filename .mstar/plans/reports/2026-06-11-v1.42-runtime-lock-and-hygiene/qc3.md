---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-11-v1.42-runtime-lock-and-hygiene"
verdict: "Approve"
generated_at: "2026-06-11"
---

# Code Review Report — QC3 (Performance & Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-11T17:30:00+08:00

## Scope
- plan_id: 2026-06-11-v1.42-runtime-lock-and-hygiene
- Review range / Diff basis: merge-base: c82f9216 + tip: HEAD of iteration/v1.42 (5128efa8) — equivalent to git diff c82f9216...HEAD
- Working branch (verified): HEAD (detached at 5128efa8)
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p0-qc
- Files reviewed: 13 files changed, +1078 / -55 lines
- Commit range: 1dad80fe..5128efa8 (7 commits)
- Tools run: cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- runtime_lock; cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings

## Findings

### 🔴 Critical
_None._

### 🟡 Warning
_None._

### 🟢 Suggestion

#### S-1: RuntimeLockGuard Drop is best-effort only; panic or cancellation leaks lock until TTL
- **Source**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs` lines 108-123
- **Issue**: The `Drop` implementation for `RuntimeLockGuard` cannot synchronously release the async database lock. It logs a `tracing::warn!` but leaves the lock held. If the handler panics or the HTTP future is cancelled (e.g., client disconnect), the `runtime_lock_holder` row remains set for up to the TTL threshold (default 2h).
- **Impact**: Users may experience multi-hour lockouts on the affected Work with no automatic recovery except waiting for TTL or manual intervention.
- **Mitigation**: The code explicitly documents this limitation and relies on TTL-based stale recovery, which is acceptable for pre-1.0 local-only usage.
- **Fix recommendation**: (1) Wrap handler bodies in `std::panic::catch_unwind` + explicit release in a `finally`-like block; or (2) add a background maintenance task that scans for `cli:*` holders whose process no longer exists and auto-clears them.

#### S-2: Missing hermetic test for schedule terminal lock release
- **Source**: `crates/nexus-orchestration/src/schedule/supervisor.rs` lines 365-379, 1097-1128
- **Issue**: The `release_daemon_schedule_lock` helper and its call site in `on_schedule_terminal()` have no unit or integration test coverage. There is no verification that a schedule reaching terminal state correctly releases its `daemon:schedule:<id>` holder.
- **Impact**: Regressions in terminal transition lock release would only be caught in production or manual testing.
- **Fix recommendation**: Add a supervisor test that (a) enqueues a schedule, (b) simulates acquiring a daemon schedule lock on the associated Work, (c) transitions the schedule to Completed, and (d) asserts the Work's `runtime_lock_holder` is cleared.

#### S-3: TTL boundary tests lack near-threshold coverage
- **Source**: `crates/nexus-local-db/src/runtime_lock.rs` lines 331-417; `crates/nexus-daemon-runtime/tests/runtime_lock.rs`
- **Issue**: Stale-recovery tests use 3h and 5h (well over the 2h default threshold). There are no tests at the exact boundary (2h), just under (1h 59m), or just over (2h 1m). Off-by-one-second bugs in `is_lock_stale` would not be caught.
- **Fix recommendation**: Add unit tests for `elapsed == ttl_secs` (should NOT be stale) and `elapsed == ttl_secs + 1` (should be stale).

#### S-4: No database index on `runtime_lock_holder`
- **Source**: `crates/nexus-local-db/src/runtime_lock.rs` (acquire/release/clear); `crates/nexus-orchestration/src/schedule/supervisor.rs` (`release_daemon_schedule_lock`)
- **Issue**: The `release_daemon_schedule_lock` query does `WHERE creator_id = ? AND runtime_lock_holder = ?` without a covering index. SQLite will perform a full table scan over the `works` table for every schedule terminal event.
- **Impact**: Negligible for typical local usage (tens to hundreds of works), but becomes O(N) per terminal event as the works table grows.
- **Fix recommendation**: Add `CREATE INDEX idx_works_runtime_lock ON works(creator_id, runtime_lock_holder)` in a new migration. This also benefits the `find_resumable_works` query which filters on `runtime_lock_holder IS NULL`.

#### S-5: `ttl_from_env()` re-reads environment variable on every lock acquire
- **Source**: `crates/nexus-local-db/src/runtime_lock.rs` lines 221-226
- **Issue**: `std::env::var("NEXUS_RUNTIME_LOCK_TTL_SECS")` is called on every `acquire_runtime_lock` invocation. This is a syscall that parses the environment on each mutating operation.
- **Impact**: Minor overhead per operation; unnecessary since environment variables are effectively immutable for the process lifetime.
- **Fix recommendation**: Cache the value with `std::sync::OnceLock<i64>` or read once at application startup and pass it as a parameter.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| S-1 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:108-123` (Drop impl) | High |
| S-2 | manual-reasoning | `crates/nexus-orchestration/src/schedule/supervisor.rs:365-379,1097-1128` | High |
| S-3 | manual-reasoning | `crates/nexus-local-db/src/runtime_lock.rs:331-417` | High |
| S-4 | manual-reasoning | `crates/nexus-orchestration/src/schedule/supervisor.rs:1112` (UPDATE WHERE) | Medium |
| S-5 | manual-reasoning | `crates/nexus-local-db/src/runtime_lock.rs:221-226` | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 5 |

**Verdict**: Approve

**Rationale**: All acceptance criteria (AC1–AC5) are met. Hermetic tests pass (10 unit + 6 integration = 16 runtime_lock tests). Clippy passes with zero warnings. The implementation correctly wires runtime lock acquire/release on all mutating paths per spec §4.2, implements TTL stale recovery with configurable threshold, and adds auto-chain skip logic for foreign holders. Schedule terminal transitions release daemon schedule locks. No performance bottlenecks were identified on the hot path (single-row UPDATE by PK). The five Suggestions are non-blocking improvements for robustness, test coverage, and minor efficiency; they do not affect the correctness or safety of the shipped behavior.

## Positive Findings

- **Robust stale recovery**: `force_stale=true` on all daemon acquire paths ensures crashed or leaked locks are recovered automatically without user intervention.
- **Good observability**: `tracing::info!` / `tracing::warn!` at all lock acquire, release, stale-clear, and failure paths.
- **Auto-chain skip**: `find_resumable_works` correctly excludes Works with `runtime_lock_holder IS NOT NULL`, preventing auto-chain from competing with active mutating operations.
- **Holder validation**: `release_runtime_lock` verifies `expected_holder` match, preventing cross-process lock release.
- **Hermetic test quality**: Integration tests in `nexus-daemon-runtime/tests/runtime_lock.rs` verify concurrent patch blocking, inspiration append blocking, stale clear, fresh lock preservation, and acquire/release lifecycle.

## Residual Notes

- Process violation `R-V142P0-PROC` is documented and risk-accepted; not in scope for this implementation review.
- Defer-7 disposition is archived in `.mstar/archived/residuals/2026-06-11-v1.42-runtime-lock-and-hygiene.json`.
- Suggestion S-1 (Drop best-effort) could be promoted to a future residual if a background cleanup task is planned.
