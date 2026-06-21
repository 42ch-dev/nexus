---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.55-df31-workspace-interface"
verdict: "Request Changes"
generated_at: "2026-06-21"
---

# Code Review Report — V1.55 P1 DF-31 Workspace Interface Skeleton

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p7
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-21T13:45:00Z

## Scope
- plan_id: 2026-06-22-v1.55-df31-workspace-interface
- Review range / Diff basis: merge-base: origin/main (9f5298e4) + tip: iteration/v1.55 HEAD (c30cdd48); P1 commits only
- Working branch (verified): iteration/v1.55
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- P1 commit range reviewed: 05801730..9b3d70ce (P1 merge only, excluding prior P0/P2 merges)
- Files reviewed: 6 (940 insertions in P1)
  - `crates/nexus-home-layout/src/lib.rs`
  - `crates/nexus-daemon-runtime/src/workspace/session.rs`
  - `crates/nexus-daemon-runtime/src/api/handlers/workspace.rs`
  - `crates/nexus-daemon-runtime/src/workspace/mod.rs`
  - `crates/nexus-daemon-runtime/src/api/mod.rs`
  - `.mstar/plans/2026-06-22-v1.55-df31-workspace-interface.md`
- Tools run:
  - `cargo test -p nexus-home-layout` — 60/60 pass
  - `cargo test -p nexus-daemon-runtime --lib api::handlers::workspace` — 18/18 pass
  - `cargo test -p nexus-daemon-runtime --lib workspace::session` — 7/7 pass
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` — clean
  - `cargo clippy -p nexus-home-layout -- -D warnings` — clean
  - `cargo +nightly fmt --all --check` — clean
  - `cargo test -p nexus-daemon-runtime --test daemon_boot_llm_wiring` — **1 failure** (see W-005)
  - Pre-existing claim verification per `.mstar/AGENTS.md`: failing test passes on `origin/main` @ 9f5298e4

## Findings

### 🔴 Critical

#### C-001: `WorkspaceSessionManager::consume_session` is not atomic — concurrent commits on the same session can both succeed
- **Source**: manual reasoning + `crates/nexus-daemon-runtime/src/workspace/session.rs:203-232`
- **Evidence**: `consume_session` clones the entry outside the lock (lines 205-211), validates `consumed`/`is_expired` without the lock (lines 213-220), then re-acquires the lock only to set `consumed = true` (lines 222-230). Two concurrent callers can both observe `consumed = false` and both return `Ok(info)`, violating the "reject stale/conflicting commits" conflict model documented in the plan and in the handler.
- **Impact**: A double-commit bug under concurrent load; the HTTP handler returns 200 to both callers for the same `session_id`, breaking session idempotency.
- **Fix**: Perform the get-check-mark sequence under a single lock acquisition (e.g., `guard.get_mut(session_id)` and update the flag inline after validation). Add a regression test that spawns two threads/tasks committing the same session and asserts exactly one succeeds.
- **Machine severity**: `critical`

### 🟡 Warning

#### W-001: Single global `Mutex<HashMap>` and O(n) cleanup under lock in `WorkspaceSessionManager`
- **Source**: `crates/nexus-daemon-runtime/src/workspace/session.rs:99-103, 242-245`
- **Evidence**: All session operations serialize through one `std::sync::Mutex`. `cleanup_expired` is invoked on every `open_session`, `validate_session`, and `consume_session`, holding the lock while scanning every entry.
- **Impact**: Latency grows linearly with the number of sessions and all operations are serialized. This contradicts the acceptance criterion "Lock contention risk is documented" — the risk is present but not documented.
- **Fix**: (1) Document the contention ceiling and expected session count in the module rustdoc; (2) move cleanup to a background tokio task or an ordered expiry structure; (3) add a metric/log line for session table size.
- **Machine severity**: `high`

#### W-002: Expired sessions accumulate during idle periods
- **Source**: `crates/nexus-daemon-runtime/src/workspace/session.rs:133, 167-168, 204, 242-245`
- **Evidence**: Expired entries are only removed when another session operation triggers `cleanup_expired`. There is no background cleanup task.
- **Impact**: If the daemon opens many sessions and then becomes idle, memory is not reclaimed until the next operation. This contradicts the acceptance criterion "No unbounded memory growth in session table" for long-running daemons.
- **Fix**: Spawn a `tokio::time::interval` task that calls `cleanup_expired` periodically, or switch to a time-bucketed expiry structure.
- **Machine severity**: `high`

#### W-003: Mutex poisoning policy inconsistency — session manager panics on poisoned mutex
- **Source**: `crates/nexus-daemon-runtime/src/workspace/session.rs:147, 173, 209, 226, 243` vs. `crates/nexus-daemon-runtime/src/workspace/mod.rs:3-9`
- **Evidence**: The crate documents a policy of recovering from poisoned mutexes with `unwrap_or_else` + `tracing::warn!`. The session manager uses `.expect("session manager mutex poisoned")` in five places.
- **Impact**: A panic in any request handler while holding the session lock will crash all subsequent session operations instead of recovering. This is a reliability regression relative to the rest of the crate.
- **Fix**: Replace `.expect(...)` with `.unwrap_or_else(|poisoned| { tracing::warn!(...); poisoned.into_inner() })` consistent with `workspace_path` accessors.
- **Machine severity**: `high`

#### W-004: HTTP error mapping depends on fragile string matching
- **Source**: `crates/nexus-daemon-runtime/src/api/handlers/workspace.rs:256-278`
- **Evidence**: `commit_workspace` maps session errors to `NexusApiError` variants by checking `err_msg.contains("not found")`, `err_msg.contains("already been committed")`, and `err_msg.contains("expired")`.
- **Impact**: A future refactor that rewords error messages will silently change HTTP semantics (e.g., a stale session could return 500 instead of 409).
- **Fix**: Introduce a typed `SessionError` enum in `session.rs` and match on variants instead of strings.
- **Machine severity**: `medium`

#### W-005: `nexus-daemon-runtime` integration test fails on the integration branch
- **Source**: `cargo test -p nexus-daemon-runtime --test daemon_boot_llm_wiring with_runtime_deps_registers_all_llm_capabilities`
- **Evidence**: Test expects 23 builtin capabilities but finds 24. Verified the same test passes on `origin/main` @ 9f5298e4, so the failure is not pre-existing. Root cause is the V1.55 P3 script-scaffold capability added to `CapabilityRegistry` without updating this assertion.
- **Impact**: The touched crate's test suite is red on `iteration/v1.55`, blocking CI-based approval.
- **Fix**: Update the assertion to 24 in `crates/nexus-daemon-runtime/tests/daemon_boot_llm_wiring.rs:227` (or derive the expected count from a registry SSOT).
- **Machine severity**: `high` (CI gate failure; scope attribution: P3, but blocks P1 crate-level verification)

### 🟢 Suggestion

#### S-001: Add observability for degraded session paths
- **Source**: `crates/nexus-daemon-runtime/src/api/handlers/workspace.rs:256-278`
- **Evidence**: Stale, expired, and missing sessions are logged at `debug!` level. The acceptance criterion asks for "Tracing at appropriate levels (info for normal, warn for degraded)".
- **Improvement**: Promote stale/expired session rejections to `tracing::warn!` (degraded behavior) and keep successful open/commit at `info!`.
- **Machine severity**: `low`

#### S-002: Add concurrent open/commit stress test
- **Source**: `crates/nexus-daemon-runtime/src/workspace/session.rs` tests
- **Evidence**: Tests cover expired cleanup and double-commit serially, but there is no concurrent test.
- **Improvement**: Add a test that concurrently opens many sessions and commits a subset to verify lock safety and atomic consume.
- **Machine severity**: `low`

#### S-003: Tighten `validate_workspace_path_safe` to reject normalization edge cases
- **Source**: `crates/nexus-home-layout/src/lib.rs:378-399`
- **Evidence**: The validator accepts `.hidden`, `foo/./bar`, `foo//bar`, and trailing slashes. These are safe in the sense that they do not escape the workspace, but they can create inconsistent snapshot keys.
- **Improvement**: Reject empty components, leading `./`, and trailing slashes, or document that callers must normalize the path before snapshot comparison.
- **Machine severity**: `low`

#### S-004: Document the 5-minute TTL in the Local API spec surface
- **Source**: Plan acceptance criteria
- **Evidence**: The TTL constant is in code (`session.rs:107`) but not in the route docs or user-facing specs.
- **Improvement**: Add a note to the handler rustdoc and to `.mstar/knowledge/specs/daemon-runtime.md` (or the DF-31 tracker) that sessions expire after 5 minutes by default.
- **Machine severity**: `low`

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| C-001 | manual-reasoning | `session.rs:203-232` | High |
| W-001 | manual-reasoning | `session.rs:99-103, 242-245` | High |
| W-002 | manual-reasoning | `session.rs:133, 242-245` | High |
| W-003 | linter/doc-rule | `session.rs` vs `workspace/mod.rs:3-9` | High |
| W-004 | manual-reasoning | `workspace.rs:256-278` | High |
| W-005 | test-run | `cargo test -p nexus-daemon-runtime --test daemon_boot_llm_wiring` | High |
| S-001 | manual-reasoning | `workspace.rs:256-278` | Medium |
| S-002 | manual-reasoning | `session.rs` tests | Medium |
| S-003 | manual-reasoning | `home-layout/src/lib.rs:378-399` | Medium |
| S-004 | doc-rule | Plan AC + `session.rs:107` | Medium |

## Standard Checklist

### Code quality
- [x] Naming clear and consistent.
- [ ] Error handling is explicit but maps by string (W-004).
- [x] Comments explain intent.

### Performance and reliability
- [ ] Hot path avoids avoidable overhead — global mutex + O(n) scan on every call (W-001).
- [ ] Resource lifecycle partially correct — mutex poison policy inconsistent (W-003); consume not atomic (C-001).
- [ ] Unbounded operation risk present — expired sessions not cleaned during idle (W-002).
- [ ] Degradation/failure behavior partially observable — degraded paths logged at debug, not warn (S-001).

### Tests
- [x] Path-bound rejection tests present.
- [x] Stale/expired session tests present.
- [ ] Concurrent open/commit test missing (S-002; relates to C-001).

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 5 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

**Rationale**: C-001 is a merge-blocking correctness defect in the session conflict model. W-001, W-002, and W-003 directly contradict the stated acceptance criteria for TTL documentation/cleanup, bounded memory, and lock-contention reliability. W-005 makes the crate-level test suite red on the integration branch and must be resolved before merge (even though the code change originates in P3, the assertion lives in the touched crate). Once C-001 is fixed and W-001/W-002/W-003/W-005 are addressed or explicitly accepted as tracked residuals, this review can move to Approve.
