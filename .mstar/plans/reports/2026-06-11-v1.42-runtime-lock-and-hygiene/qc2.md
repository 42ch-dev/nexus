---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-11-v1.42-runtime-lock-and-hygiene"
verdict: "Approve"
generated_at: "2026-06-11"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (per mstar-roles parameter table: reviewer_index=2, focus=security_correctness)
- Report Timestamp: 2026-06-11T18:15:00+08:00

## Scope
- plan_id: 2026-06-11-v1.42-runtime-lock-and-hygiene
- Review range / Diff basis: merge-base: c82f9216 (P-1 HEAD) + tip: HEAD of iteration/v1.42 (5128efa8) — equivalent to `git diff c82f9216...HEAD`
- Working branch (verified): HEAD (detached at 5128efa8 on iteration/v1.42)
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p0-qc
- Files reviewed: 13 changed (primary focus on the 4 code commits implementing T1–T5: new runtime_lock.rs module, works.rs daemon handler wiring + RuntimeLockGuard, auto_chain.rs enqueue acquire, supervisor.rs terminal release, daemon-runtime integration tests, and the multi_work_switch test update). Also reviewed plan, primary spec §4.2, status.json residual updates, and the docs commit that stamps implementation.
- Commit range: c82f9216...5128efa8 (exactly 7 commits as assigned)
- Tools run: `git log c82f9216..HEAD --oneline`, `git diff c82f9216..HEAD --stat`, manual diff review of acquire/release paths / holder formats / RAII guard / stale recovery / error surfaces, `cargo test -p nexus-daemon-runtime --test runtime_lock` (6 hermetic integration tests), `cargo test -p nexus-daemon-runtime --test multi_work_switch`, `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` (clean), `cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- runtime_lock` (as specified for evidence).

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
- **W-001 (Correctness — stale recovery concurrent acquire race under force_stale path)**: The `acquire_runtime_lock` force-clear path (and the early-exit check in `patch_work`) follows a check-then-act pattern. After confirming via `is_lock_stale` that a holder is older than TTL, the subsequent `UPDATE ... SET runtime_lock_holder = ? WHERE work_id=? AND creator_id=?` is unconditional on the prior holder value or timestamp. Two processes that both observe the same stale lock (near-simultaneous recovery after a crash) can both pass the stale check and both perform the holder write, allowing both to proceed into their mutating handlers. The `release_runtime_lock` call is holder-matched, so the second release becomes a no-op; both mutations can execute under their respective "acquired" holders until one completes.

  Per `novel-multi-work-lifecycle.md` §1.1 and §4.1, the concurrency model is "Same Work, two processes: Forbidden for mutating operations" under a local-first single-writer assumption. Stale recovery (R-V141P0-01) is explicitly intended to allow progress after a crashed CLI/daemon holder (default 2h TTL, env override). The race only manifests for concurrent recovery attempts on an already-stale lock. Hermetic tests cover single stale recovery (AC2) and fresh-lock preservation, but do not exercise dual concurrent recovery.

  Evidence:
  - `crates/nexus-local-db/src/runtime_lock.rs:73-88` (the `if force_stale && is_lock_stale { ... } else { return Locked }` early return)
  - `runtime_lock.rs:91-105` (the UPDATE that writes the new holder with no prior-holder predicate)
  - `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1026-1037` (patch_work early-exit rejects only if NOT stale; stale falls through to `RuntimeLockGuard::acquire(..., true)`)
  - `auto_chain.rs:501-525` (daemon schedule acquire also uses force_stale=true)
  - `supervisor.rs:369-379` and the `release_daemon_schedule_lock` helper (holder-matched terminal release)
  - No new SQL injection: all values are bound parameters; runtime `sqlx::query` usage follows the crate's pre-existing dynamic-DML convention with SAFETY comments.

  Impact: Low in the documented local single-user model. The race window is narrow (both recoveries must sample the same backdated timestamp before either UPDATE commits). However, because V1.42 P0 makes the force-clear path production behavior for the first time, the edge is now reachable in the recovery scenario and should be tracked or hardened if the single-writer assumption is ever relaxed.

  Disposition for this review: Record as Warning (not Critical) because it does not violate the stated acceptance criteria (AC1 still holds for fresh locks; AC2 holds for single stale recovery) and does not introduce injection, auth bypass, or data-loss vectors. Recommend either (a) adding a predicate to the force-clear UPDATE (e.g., `AND runtime_lock_acquired_at = ?` or a generation column) or (b) explicit documentation of the "at most one recovery writer at a time" assumption in the module and in spec §4.2.

### 🟢 Suggestion
- **S-001 (Test coverage)**: Add a hermetic integration test that drives two near-concurrent stale-recovery acquires (e.g., via two HTTP PATCHes after setting a backdated lock, or two schedule enqueues). Assert that at least one succeeds and that post-completion the lock is released (or document the "last writer wins + mismatched release is harmless" outcome). This would make the recovery semantics self-documenting beyond the unit tests in `runtime_lock.rs`.
- **S-002 (Observability / UX of holder strings)**: The `cli:http:<uuid>` holder used for daemon HTTP mutators (see `RuntimeLockGuard::acquire` and `cli_holder("http")`) is correctly documented as an approximation (no real OS PID is available over the local API). When `creator works status` or error messages surface the holder, a user sees an opaque `cli:http:...` string. Consider a small future enhancement to map well-known caller prefixes to friendlier labels ("held by CLI", "held by daemon schedule", "held by HTTP request") while still returning the raw holder for debugging. Non-blocking for P0.
- **S-003 (sqlx compile-time macro hygiene)**: The new `runtime_lock` module uses runtime `sqlx::query(...)` + bound parameters + `// SAFETY: Dynamic SQL required for conditional lock acquire/release` comments for the two conditional UPDATE statements. This is consistent with the crate's pre-existing policy (see `nexus-local-db/AGENTS.md` and prior residuals such as R-V133P1-09). No correctness or injection regression. These sites are the natural candidates to revisit if/when a workspace-wide sqlx offline enforcement pass occurs (R-V140P4-INFRA and R-V140P0-S3 context). No action required for this plan.

## Source Trace
- Finding ID: W-001
- Source Type: manual-reasoning + static diff review of control flow and SQL
- Source Reference: `crates/nexus-local-db/src/runtime_lock.rs:59-110` (acquire_runtime_lock full body), `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1026-1040` (early stale check + guard acquire in patch_work), `crates/nexus-orchestration/src/auto_chain.rs:501-525` (enqueue acquire with force_stale), `crates/nexus-orchestration/src/schedule/supervisor.rs:369-379` + release helper (terminal release), plus the 6 new tests in `crates/nexus-daemon-runtime/tests/runtime_lock.rs`
- Confidence: High (the check-then-unconditional-UPDATE pattern is deterministic from the source; no dynamic dispatch or hidden locking primitives)

(Additional source traces for S-001–S-003 point to the same files plus `RuntimeLockGuard` Drop impl at works.rs:108-120 and the cli_holder call sites.)

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

All stated acceptance criteria are met:
- AC1 (concurrent mutating operations on same Work → second fails with holder hint) holds for fresh locks; the documented single-writer model plus holder-matched release provides the intended exclusion.
- AC2 (crashed CLI holder cleared after configurable TTL) is implemented (T4) and covered by hermetic tests.
- AC3 (spec §4.2 stamped implemented) delivered in commit 29179b2e.
- AC4 (defer-7 disposition) and AC5 (cargo test + clippy on scoped crates) satisfied.

No new security surfaces (injection, privilege escalation, sensitive data in holders, or auth bypass) were introduced. The RAII `RuntimeLockGuard` + explicit early release on success paths + Drop impl provides panic safety and release discipline for daemon HTTP handlers. Daemon schedule holders are acquired before enqueue and released on terminal transitions (including boot-recovery skip of locked Works). All new DML uses bound parameters.

The single Warning (W-001) describes a low-probability race on the force-stale recovery path that only arises under concurrent recovery attempts after a prior crash. Given the local-first single-user concurrency model explicitly called out in the primary spec, this is acceptable for P0. It does not block the stated acceptance criteria and can be hardened in a future iteration (e.g., V1.42 P1 or hygiene) if operational experience shows value.

Process violation R-V142P0-PROC is explicitly marked risk-accepted in the assignment and is out of scope for this implementation review.

## Evidence Captured (for Completion Report v2)
- `git rev-parse --show-toplevel`: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p0-qc
- `git rev-parse --abbrev-ref HEAD`: HEAD
- `git log c82f9216..HEAD --oneline`: 7 commits (1dad80fe ... 5128efa8) matching assignment
- `git diff c82f9216..HEAD --stat`: 13 files changed, +1078 insertions, -55 deletions
- `cargo test -p nexus-daemon-runtime --test runtime_lock`: 6 tests passed (test_concurrent_patch_second_fails_with_holder_hint, test_concurrent_inspiration_second_fails_with_holder_hint, test_stale_lock_cleared_after_ttl, test_fresh_lock_not_cleared_within_ttl, test_patch_acquires_and_releases_lock, test_inspiration_acquires_and_releases_lock)
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings`: clean (finished with no warnings emitted under -D)
- Report committed as: (see Completion Report Git line)
- Post-commit working tree: clean (git status --short empty)
