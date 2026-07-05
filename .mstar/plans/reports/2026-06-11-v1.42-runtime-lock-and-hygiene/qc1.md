---
report_kind: qc-review
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-11-v1.42-runtime-lock-and-hygiene"
verdict: "Approve"
generated_at: "2026-06-11"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro (volcengine-plan/deepseek-v4-pro)
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-11T18:00:00+08:00

## Scope
- plan_id: `2026-06-11-v1.42-runtime-lock-and-hygiene`
- Review range / Diff basis: `merge-base: c82f9216` (P-1 HEAD) + `tip: HEAD` of `iteration/v1.42` (`5128efa8`) — equivalent to `git diff c82f9216...HEAD`
- Working branch (verified): `HEAD` (detached) at `5128efa8`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p0-qc`
- Files reviewed: 13 (8 Rust source, 5 harness/docs)
- Commit range: `c82f9216..5128efa8` (7 commits)
- Tools run: `cargo test -p nexus-local-db -- runtime_lock`, `cargo test -p nexus-daemon-runtime --test runtime_lock`, `cargo test -p nexus-daemon-runtime --test multi_work_switch -- runtime_lock`, `cargo test -p nexus-orchestration --test auto_chain -- enqueue`, `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings`, `cargo clippy -p nexus-orchestration -- -D warnings`, `git diff`, `git log`, manual code review

## Findings

### 🔴 Critical
None.

### 🟡 Warning

**W-01: `RuntimeLockGuard` Drop does not release lock — misleading RAII name**
- **Trigger**: If a handler panics or returns early between `RuntimeLockGuard::acquire()` and explicit `lock.release().await`, the `Drop` implementation only logs a warning — it does not release the lock because async operations are not possible in `Drop`.
- **Impact**: A crashed/panicked handler leaves the runtime lock held until TTL expiry (default 2h). During that window, no other process can mutate the Work. The TTL-based recovery is the fallback, but 2h is a long window for a local-only tool.
- **Fix**: Either (a) rename the guard to `RuntimeLockHandle` or `RuntimeLockToken` to avoid implying RAII safety, or (b) add a prominent doc comment on the struct warning that Drop is not a release path, or (c) use `tokio::spawn` in Drop for best-effort async release (with its own tradeoffs). The current explicit `release()` pattern in `patch_work` and `append_inspiration` is correct for the happy path.
- **Source Reference**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:108-123` (Drop impl)
- **Confidence**: High

**W-02: `patch_work` handler complexity — pre-existing `too_many_lines` suppression now carries additional lock logic**
- **Trigger**: The `patch_work` handler was already annotated with `#[allow(clippy::too_many_lines)]` before this change (line 998). The V1.42 P0 change adds a completion-lock check, a stale-lock check, and a `RuntimeLockGuard::acquire` call — increasing the function's cognitive load.
- **Impact**: Maintainability risk — the function now mixes lock acquisition, stage-gate validation, world_id clear-rejection, non-stage patching, and supervisor tick triggering in a single handler. Future changes to any of these concerns risk unintended interactions.
- **Fix**: Deferred refactoring — extract the lock acquisition + completion-lock check into a helper (e.g., `guard_mutating_operation`), or split the handler into smaller composed functions. Not blocking for this review wave; the existing `#[allow]` was already present and the lock logic is correctly ordered (check → acquire → operate → release).
- **Source Reference**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:998-1132`
- **Confidence**: Medium

### 🟢 Suggestion

**S-01: `RuntimeLockGuard` could be extracted to a shared module for CLI reuse**
- **Improvement**: Currently defined inline in `nexus-daemon-runtime/src/api/handlers/works.rs`. If CLI paths (e.g., `creator run` subcommands) need the same RAII acquire/release pattern, the guard should be extracted to `nexus-local-db` or a shared utility crate. This avoids duplication and keeps the holder format contract in one place.
- **Source Reference**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:31-123`
- **Confidence**: Medium

**S-02: `release_daemon_schedule_lock` uses holder-based lookup without work_id scoping**
- **Improvement**: In `supervisor.rs:1104-1128`, the release function looks up `WHERE runtime_lock_holder = ?` without also filtering by `work_id`. While holder strings include a UUID/schedule_id component that makes collisions extremely unlikely, a holder-based lookup is less precise than a `(work_id, holder)` pair. Consider embedding the work_id in the daemon holder format (e.g., `daemon:schedule:<schedule_id>:<work_id>`) for unambiguous release, or adding a `work_id` parameter to the release function.
- **Source Reference**: `crates/nexus-orchestration/src/schedule/supervisor.rs:1104-1128`
- **Confidence**: Low

**S-03: Consider a test for the Drop/panic path**
- **Improvement**: Add a hermetic test that simulates a handler panic (e.g., via `std::panic::catch_unwind` around a guard acquire without explicit release) and verifies that TTL-based recovery clears the stale lock. This would provide regression coverage for the Drop limitation documented in W-01.
- **Source Reference**: `crates/nexus-daemon-runtime/tests/runtime_lock.rs`
- **Confidence**: Medium

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-01 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:108-123` | High |
| W-02 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:998-1132` | Medium |
| S-01 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:31-123` | Medium |
| S-02 | manual-reasoning | `crates/nexus-orchestration/src/schedule/supervisor.rs:1104-1128` | Low |
| S-03 | manual-reasoning | `crates/nexus-daemon-runtime/tests/runtime_lock.rs` | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

### Rationale

The V1.42 P0 implementation is architecturally sound and maintainable:

1. **Module boundaries are clean**: `runtime_lock.rs` in `nexus-local-db` encapsulates all lock logic with a well-defined public API (`AcquireResult` enum, `acquire_runtime_lock`, `release_runtime_lock`, `is_lock_stale`, `clear_stale_lock`, holder builders, `ttl_from_env`). The module is properly re-exported from `lib.rs`.

2. **RAII guard design is reasonable**: The `RuntimeLockGuard` in the daemon handlers correctly acquires on construction and releases via explicit `release()` call. The Drop limitation (W-01) is a known Rust async constraint, not a design flaw — the explicit release pattern is correct for the happy path, and TTL recovery handles the failure path.

3. **Spec/code alignment is exact**: All mutating paths specified in §4.2 are covered — daemon `patch_work` and `append_inspiration` (via `RuntimeLockGuard`), schedule enqueue (via `enqueue_auto_chain_schedule`), schedule terminal release (via `on_schedule_terminal`), auto-chain tick skip (via `find_resumable_works`). Holder formats match §4.1. Stale recovery with configurable TTL matches the spec.

4. **Test architecture is hermetic and comprehensive**: 10 unit tests in `runtime_lock.rs`, 6 integration tests in `nexus-daemon-runtime/tests/runtime_lock.rs`, 1 multi-work switch test, 3 auto-chain enqueue tests. All use tempdir + in-memory SQLite — no shared state, no flaky dependencies. All tests pass. Clippy is clean on all P0 crates.

5. **Error handling is consistent**: Locked → 423 with holder hint; stale → force-clear with info log; release failure → warn log (best-effort); auto-chain acquire failure → warn + continue (non-fatal). No silent failures.

6. **Closeout commit content is accurate**: The spec §4.2 stamp correctly transitions from "Gap" to "Implemented" with a reference to the plan. The plan status correctly marks T1–T7 as done with commit references. The residual fix commit (5128efa8) correctly normalizes severity enums and archives closed entries.

7. **Defer-7 disposition is well-documented**: All 7 items have clear dispositions in the archived residuals file. No re-litigation needed.

The two Warnings are non-blocking: W-01 is a known Rust async constraint with a working TTL fallback; W-02 is a pre-existing complexity concern that was not introduced by this change. No Critical findings.
