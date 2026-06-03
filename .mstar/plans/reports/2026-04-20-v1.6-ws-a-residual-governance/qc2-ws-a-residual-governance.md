---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-04-20-v1.6-ws-a-residual-governance"
verdict: "Request Changes"
generated_at: "2026-04-20"
---

# QC Report — V1.6 WS-A Residual Governance

**Reviewer**: @qc-specialist-2 (Reviewer #2)
**Review scope**: `git diff 75a1012..2182cbf` on `feature/v1.6`
**Primary accent**: Security & correctness (input validation, auth boundaries, state consistency)
**Secondary accent**: Maintainability & interface contract clarity

---

## Executive Summary

The 4 residual fixes (R1, R2, R3, R6) are **structurally correct** and address the described issues. `cargo clippy --all -- -D warnings` passes clean. However, **one Warning-level finding** blocks Approve: the R2 TOCTOU race fix introduces an untested fallback code path (`rows_affected() == 0` after UPDATE) that is not exercised by any unit test. Given R2 is a **medium-severity** residual, the fix itself must be covered.

**Verdict: Request Changes** — one Warning to resolve (add test coverage for R2 TOCTOU fallback path).

---

## Critical Findings (blocking)

*None.*

---

## Warnings (non-blocking but important — must be resolved before Approve)

### W1 — R2 TOCTOU fallback path untested (`rows_affected() == 0` branch)

**Location**: `crates/nexus-orchestration/src/schedule/supervisor.rs:695-706` and `:731-742`
**Severity**: Warning
**Finding**: The `resume_schedule()` method adds `rows_affected() == 0` checks after both UPDATE statements (running path and pending fallback path). This is the core R2 fix. However, **neither branch is exercised by existing tests**:

- `r2_resume_toctou_race_returns_current_status` (supervisor.rs:1598) does **not** hit the `rows_affected() == 0` path. The second `resume_schedule()` call fails at the **early status check** (line 609: `row.status.as_str() != "paused"`) which runs *before* the UPDATE. The test name claims to test the TOCTOU race, but it actually tests sequential double-resume.
- `r2_resume_paused_schedule_succeeds_normally` (supervisor.rs:1629) hits the success path (`rows_affected() == 1`).

**Impact**: The untested fallback path is the primary defense against the medium-severity R2 residual. A regression (e.g., someone refactoring the error handling or removing the fallback) would not be caught by CI.

**Recommendation**: Add a test that deterministically triggers `rows_affected() == 0` after the UPDATE. Two viable approaches:
1. **Mock/strategy injection**: Inject a test-only `sqlx::query` wrapper that returns `rows_affected() == 0` on the second call.
2. **DB trigger/simulated race**: Use a raw SQL UPDATE inside the test between the status check and the UPDATE (this is tricky because the method holds no lock across the gap).
3. **Simpler**: Add an integration test that uses `sqlx::query` to change the status to `running` *between* two concurrent `resume_schedule` calls. While true concurrency is hard, you can approximate by calling `resume_schedule` on two cloned supervisor instances sharing the same pool — the SQLite single-writer model will serialize one UPDATE after the other.

**Cross-reviewer note**: This is a test-coverage gap, not a code-correctness issue. The code logic is sound.

---

## Suggestions (improvements — optional)

### S1 — `Scheduler::tick()` doc comment inconsistent with return value

**Location**: `crates/nexus-orchestration/src/scheduler/mod.rs:84-91`
**Severity**: Suggestion
**Finding**: The doc comment states "Returns the number of schedules that were found due (not necessarily admitted)", but the implementation (post-R3) always returns `0`. The inline comment acknowledges this: "Return 0 as a conservative estimate".

**Recommendation**: Update the doc comment to reflect the new behavior, or change the return type to `()` if the count is no longer meaningful. If future callers need admission counts, expose a supervisor method.

### S2 — `reconstruct_runner` overwrites existing runner without check

**Location**: `crates/nexus-orchestration/src/engine.rs:591-595`
**Severity**: Suggestion
**Finding**: `HashMap::insert` overwrites any existing runner for the same session ID. If `recover_sessions` were called twice (e.g., daemon restart followed by a manual recovery trigger), the runner would be reconstructed and replaced. This is benign in the current single-call-at-boot pattern but could mask bugs in future extensions.

**Recommendation**: Add a `contains_key` check and log a `debug!` or `warn!` if a runner already exists, to make duplicate recovery visible.

### S3 — `#[allow(dead_code)]` on `Scheduler.pool` could be removed

**Location**: `crates/nexus-orchestration/src/scheduler/mod.rs:68`
**Severity**: Suggestion
**Finding**: The `pool` field is no longer used after R3 but retained with `#[allow(dead_code)]` for "future use". This is a minor code smell.

**Recommendation**: Either remove the field and re-add it when needed, or add a more specific comment explaining when it will be used (e.g., link to a planned issue or WS).

---

## Files Reviewed

| File | Commit(s) | Residual | Review Focus |
|------|-----------|----------|--------------|
| `crates/nexus42d/src/api/handlers/orchestration/schedules.rs` | `f7379b3` | R1 | Cancel-path error logging, standalone pause propagation |
| `crates/nexus-orchestration/src/schedule/supervisor.rs` | `f7379b3` | R2 | `rows_affected()` checks, TOCTOU handling, cache consistency |
| `crates/nexus-orchestration/src/scheduler/mod.rs` | `3b0464b` | R3 | Dead code removal, `tick()` delegation, import cleanup |
| `crates/nexus-orchestration/src/engine.rs` | `68a9367` | R6 | `reconstruct_runner`, FlowRunner lifecycle, terminal session handling |
| `crates/nexus-orchestration/tests/cron_trigger.rs` | `3b0464b` | R3 | Test updates for `tick()` behavior change |
| `.mstar/plans/2026-04-20-v1.6-ws-a-residual-governance.md` | `2182cbf` | — | Plan documentation accuracy |

---

## Verification Results

| Check | Command | Result | Notes |
|-------|---------|--------|-------|
| Clippy | `cargo clippy --all -- -D warnings` | ✅ Pass | Clean, no warnings |
| Format (stable) | `cargo fmt --check --all` | ⚠️ Diff in generated files only | Diffs are in `crates/nexus-contracts/src/generated/` which is excluded by `.rustfmt.toml` `ignore` directive; unstable feature warning on stable channel is expected. Changed source files are clean. |
| Format (nightly) | `cargo +nightly fmt --all -- --check` | ⏭️ Skipped | Nightly toolchain not available in review environment; plan evidence claims clean. |
| Tests | `cargo test --workspace` | ⏭️ Skipped | Unable to run in review environment; plan evidence claims 776 passed, 1 failed (pre-existing `auth::tests::get_returns_none_for_unknown_creator` in `nexus42`, unrelated). |
| Dead code verification | `rg "SELECT schedule_id FROM creator_schedules.*scheduled_at" --type rust` | ✅ Confirmed | Plan evidence confirmed: 0 results in codebase. |

---

## Per-Residual Assessment

### R1 — Cancel signal `pause_schedule()` error silently ignored

**Status**: ✅ Correctly resolved

**Code**: `schedules.rs:548-554` changes `let _ = supervisor.pause_schedule(...)` to explicit `if let Err(e) = ...` with `tracing::warn!`.

**Assessment**:
- The cancel operation is **not blocked** by pause failure — correct per compass requirement.
- The standalone pause path (`schedules.rs:482`) already propagated errors via `.map_err()` — no change needed there.
- The `warn!` log includes the schedule ID and error. No sensitive data exposure (error type is `SupervisorError`, no credentials).
- Tests `r1_cancel_pause_failure_does_not_block_cancel` and `r1_running_schedule_pause_then_cancel_succeeds` verify supervisor-level behavior. The HTTP handler `warn!` path is not directly tested (testing log output is impractical in unit tests), but the functional behavior is covered.

### R2 — `resume_schedule()` TOCTOU race on concurrent callers

**Status**: ⚠️ Code correct, test coverage gap (see W1)

**Code**: `supervisor.rs:695-706` and `:731-742` add `rows_affected() == 0` checks after UPDATE statements.

**Assessment**:
- The fix is **correct** for SQLite's single-writer model: if a concurrent caller already transitioned the schedule, the second caller's UPDATE matches 0 rows and the fallback returns the current status without touching cache.
- The fallback path queries `status_of()` and maps all 6 schedule statuses to strings. This is defensive and complete.
- **However**, the `rows_affected() == 0` branch is **not exercised by any test** (see W1). The existing test hits the early status check, not the UPDATE fallback.
- No additional serialization is needed (holding `Mutex` across async DB ops is indeed an anti-pattern, as noted in the compass).

### R3 — `Scheduler::tick()` dead code

**Status**: ✅ Correctly resolved

**Code**: `scheduler/mod.rs` removes the redundant `SELECT schedule_id ...` query, `HashSet` tracking, and associated imports.

**Assessment**:
- The removed query was truly dead: `tick_clocked()` already performs the same filtering.
- Unused imports `HashSet` and `ScheduleId` were removed.
- The `cron_trigger.rs` test was updated to remove assertions on the return value (which now always returns `0`).
- No production code uses the return value of `tick()` (the `run()` loop discards it).
- `rg` verification confirmed no remaining references to the removed query pattern.

### R6 — Recovered sessions lack FlowRunner instances (`NoGraphLoaded`)

**Status**: ✅ Correctly resolved

**Code**: `engine.rs:533-604` adds `reconstruct_runner()` and iterates non-terminal summaries in `recover_sessions()`.

**Assessment**:
- **Security**: `preset_id` comes from persisted `SessionSummary`. `load_embedded_preset()` only loads presets compiled into the binary via `include_dir!` — no arbitrary filesystem access. Unknown presets are handled gracefully (warning logged, no runner created).
- **State consistency**: Terminal sessions are skipped (`is_terminal()` check), preventing unnecessary runner creation. The session is still added to the tracker via `recover_sessions()` (idempotent deduplication in `EngineSharedState`).
- **Error handling**: `reconstruct_runner` returns `Result<(), EngineError>`. On failure, a `tracing::warn!` is emitted and the session remains in the tracker. `run_step` will fail with `NoGraphLoaded` for such sessions — this is expected degraded behavior.
- **Tests**: All 3 R6 tests cover the key scenarios:
  - `r6_recovered_session_has_runner_no_graph_loaded_fix`: Unknown preset → graceful degradation.
  - `r6_recovered_session_with_known_preset_has_runner`: Known preset → runner reconstructed, `NoGraphLoaded` avoided.
  - `r6_terminal_sessions_skipped_during_recovery`: Terminal sessions skipped.
- **Potential issue (minor)**: The `reconstruct_runner` test for known preset (`novel-writing`) creates a session with a task ID (`gathering`) that exists in the preset. If the preset changes, the test might break. However, embedded presets are version-controlled and the test uses a stable preset ID.

---

## Cross-Reviewer Ready Notes

### Areas for Reviewer #1 / #3 to cross-validate

1. **R2 concurrent test coverage**: My W1 finding notes that the `rows_affected() == 0` path is untested. Another reviewer may have found a way to test this deterministically, or may disagree on severity. If a test is added, I will re-review.
2. **R6 runner lifecycle**: Verify that `reconstruct_runner` does not leak memory or create duplicate runners if `recover_sessions` is called multiple times. The current code uses `HashMap::insert` which overwrites — acceptable for boot-time single call, but worth confirming.
3. **R3 API contract change**: `Scheduler::tick()` now returns `0` instead of admission count. Confirm no callers outside tests rely on the return value.

### What this reviewer uniquely verified

- **Security boundary on R6**: Confirmed `load_embedded_preset()` restricts loading to compiled-in presets only (no FS traversal).
- **R1 error propagation completeness**: Verified that both cancel-path (now fixed) and standalone pause path (already correct) handle errors appropriately.
- **R2 TOCTOU code-path analysis**: Traced that the `rows_affected() == 0` fallback is unreachable in current tests (early status check intercepts first).
- **Clippy clean**: Confirmed `cargo clippy --all -- -D warnings` passes with zero warnings on changed crates.

---

## Acceptance Criteria Check (Compass §4 WS-A)

| Criterion | Status | Evidence |
|-----------|--------|----------|
| R1: Log `pause_schedule()` error at `warn!` level; cancel not blocked | ✅ Met | `schedules.rs:548-554` |
| R2: Check `rows_affected()` after `resume_schedule()` UPDATE; return current status if 0 | ⚠️ Partially met | Code correct, but fallback path untested (W1) |
| R3: Remove dead `Scheduler::tick()` query; delegate to `tick_clocked()` | ✅ Met | `scheduler/mod.rs` refactored; `rg` confirms no remaining references |
| R6: Reconstruct FlowRunner from preset after session recovery | ✅ Met | `engine.rs:533-604`; 3 tests cover known/unknown/terminal scenarios |
| V1.5 QC2 S2 verification (`apply_llm_summarize` `[u8;32]` vs hex) | ✅ Met | Plan documents "No production caller exists — only test calls. No conversion bug." |
| `cargo clippy --all -- -D warnings` clean | ✅ Met | Verified in review environment |
| `cargo +nightly fmt --all -- --check` clean | ⏭️ Not verified | Plan evidence claims clean; nightly unavailable in review env |
| `cargo test --workspace` green | ⏭️ Not verified | Plan evidence claims 776 passed, 1 pre-existing failure |

---

## Residual / Tech Debt Forwarded to PM

| ID | Description | Severity | Target |
|----|-------------|----------|--------|
| W1 | R2 TOCTOU fallback path (`rows_affected() == 0`) lacks test coverage | Warning | This plan |
| S1 | `Scheduler::tick()` doc comment inconsistent with `0` return value | Suggestion | Opportunistic |
| S2 | `reconstruct_runner` silently overwrites existing runner | Suggestion | Opportunistic |
| S3 | `#[allow(dead_code)]` on `Scheduler.pool` is minor code smell | Suggestion | Opportunistic |

---

*Report generated by @qc-specialist-2 as part of V1.6 WS-A QC triple-review.*
