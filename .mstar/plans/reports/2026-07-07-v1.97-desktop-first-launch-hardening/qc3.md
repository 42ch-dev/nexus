---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-07-v1.97-desktop-first-launch-hardening"
verdict: "Approve with residuals"
generated_at: "2026-07-08"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: deepseek-v4-flash
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-07-08

## Scope
- plan_id: `2026-07-07-v1.97-desktop-first-launch-hardening`
- Review range / Diff basis: `merge-base: 070e26f7 (main) + tip: ab618ee9 (feature branch HEAD)`; equivalent to `git diff 070e26f7...ab618ee9`
- Working branch (verified): `feature/v1.97-desktop-first-launch-hardening`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 16 (564 insertions, 18 deletions)
- Commit range: `070e26f7..ab618ee9` (5 commits)
- Tools run: `git diff`, `git log`, `read`, `grep`
- Deep review: triggered (S1: 564 lines / 16 files, S2: sidecar lifecycle/process mgmt, S6: ≥3 module boundaries)
- Lenses applied: Performance Lens, Reliability Lens

## Findings

### 🟡 Warning

#### W-001: Stderr drain task — inner.stderr_tail write races with health-probe failure snapshot (preserved V1.96 residual)

**File:** `apps/desktop/src-tauri/src/sidecar.rs`, lines 265–276 (spawn path), lines 312–318 (failure snapshot)

The stderr drain task writes the captured tail into `inner.stderr_tail` **after** the drain loop completes (line 274–275). However, the health-probe failure path reads from the shared `Arc<Mutex<String>>` (`stderr_tail`, line 313) directly — not from `inner.stderr_tail`. This means:

- If the daemon crashes before the drain task processes any events, the `Terminated` event arrives, the drain loop exits with an empty buffer, and `inner.stderr_tail` is set to `Some("")`.
- The failure snapshot reads from the Arc, which is also empty → `stderr_snapshot = None` → generic fallback message. This is **correct behavior** (no lost error), but the dual storage (`Arc` + `inner.stderr_tail`) is confusing and the `inner.stderr_tail = Some("")` write is redundant noise.

**Risk:** Low. No error is lost — the Arc read in the failure path and the `inner.stderr_tail` write are consistent. The prior V1.96 residual R-V196QC3-W001 tracked a sub-ms drain race; this diff preserves the same architecture and does **not** worsen it.

**Suggestion:** In a future iteration, consolidate to a single storage path: either read from `inner.stderr_tail` after the drain task completes (with a bounded wait), or remove the `inner.stderr_tail` write and always read from the shared Arc. Not a V1.97 blocker.

- Source Type: deep-lens: Reliability Lens
- Confidence: Medium

#### W-002: No test for `start()` (auto, no budget reset) from `Error` state

**File:** `apps/desktop/src-tauri/src/sidecar.rs`, tests at lines 927–954

The new `error_state_retries_and_attaches_when_health_ready` test (line 927) uses `start_daemon()` which resets the crash budget. There is no test for `start()` (auto restart, no budget reset) from `Error` state. The behavior is the same code path (`start_with_budget` with `reset_budget=false`), and the budget-reset behavior is separately tested in `start_daemon_resets_crash_budget` (line 853), so the gap is small.

**Risk:** Low. The `start_with_budget` code path is identical; only the `reset_budget` parameter differs. The budget-not-reset behavior is implicitly covered by `crash_restart_stops_when_budget_exhausted` (line 956) which shows that `handle_crash` → `start()` with exhausted budget correctly lands in `Stopped`.

**Suggestion:** Add a test for `start()` (no budget reset) from `Error` state in a follow-up to close the gap completely.

- Source Type: deep-lens: Testing Lens
- Confidence: Medium

### 🟢 Suggestion

#### S-001: Spawn-name fix is correct but the commit message could document the Tauri v2 API rule

**File:** Commit `ab618ee9` — "V1.97 T5: fix sidecar spawn name — Tauri v2 shell().sidecar() takes filename only"

The fix (`"binaries/nexus42"` → `"nexus42"` in both `sidecar.rs` line 249 and `capabilities/main.json` line 611) is correct. Tauri v2's `shell().sidecar()` resolves the binary name against `bundle.externalBin` entries using the bare filename, not the path prefix. The old `"binaries/nexus42"` would fail to resolve at runtime, causing a silent spawn failure.

The commit message is clear. No action needed.

- Source Type: manual-reasoning
- Confidence: High

#### S-002: `overflow-hidden` on card is correct but the wizard-page test could also assert no visible overflow

**File:** `apps/web/src/pages/setup-wizard-page.tsx` line 64, test at `setup-wizard-page.test.tsx` line 71

The `overflow-hidden` class on the card (line 64) correctly contains the flex children. The test asserts the class is present (line 71). For stronger regression coverage, a future test could render a very long step label or content and assert no scroll/overflow on the card element. Not a V1.97 blocker.

- Source Type: deep-lens: Performance Lens
- Confidence: Low

#### S-003: `drain_stderr` uses `from_utf8_lossy` — acceptable for diagnostics, worth noting

**File:** `apps/desktop/src-tauri/src/sidecar.rs`, line 527

`String::from_utf8_lossy` replaces invalid UTF-8 bytes with U+FFFD. For diagnostic display this is acceptable, but if the daemon emits structured error messages with non-UTF-8 content, the replacement could obscure the real error. The daemon's stderr is expected to be UTF-8 (Rust logging), so this is a theoretical concern only.

- Source Type: deep-lens: Reliability Lens
- Confidence: Low

## Residual Candidates

| ID | Severity | Title | File | Description |
|----|----------|-------|------|-------------|
| R-V197QC3-W001 | low | Stderr drain dual-storage path (Arc + inner.stderr_tail) | `sidecar.rs:265-276` | Dual storage is confusing but not buggy. Consolidate in future iteration. |
| R-V197QC3-W002 | nit | Missing `start()` from Error state test | `sidecar.rs` tests | Add test for auto-restart (no budget reset) from Error state. |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve with residuals

## Assessment by Focus Area

### 1. First-launch reliability (spawn → health-probe → state-transition)
**Pass.** The initial-state fix (`Stopped` instead of `Starting`) eliminates the deadlock where a clean-state launch would short-circuit at `start_with_budget` without ever spawning. The spawn-name fix (`binaries/nexus42` → `nexus42`) fixes the Tauri v2 API mismatch that silently prevented sidecar resolution. Together, these two changes make the clean-state spawn path actually reachable.

The state machine is bounded and actionable:
- `Stopped` → `start_with_budget` → attach attempt → spawn → health probe (15s timeout, 250ms interval) → `Running` or `Error`
- `Error` → `start_daemon` (resets budget) → retry
- Backoff: exponential (500ms base, ×2 per attempt, ±25% jitter, 8s cap, 5 max attempts)
- No indefinite limbo on paths that CAN succeed (existing-install attach is a single probe → immediate success or fallthrough to spawn)

### 2. Stderr capture reliability
**Acceptable.** The drain task architecture is preserved from V1.96. The dual storage path (`Arc<Mutex<String>>` shared with failure snapshot + `inner.stderr_tail` write after drain completes) is confusing but not buggy — the failure path reads from the Arc directly, which is race-free. The prior V1.96 residual R-V196QC3-W001 (sub-ms drain race) is neither worsened nor fixed by this diff.

### 3. Crash budget / retry bounds
**Pass.** `MAX_RESTART_ATTEMPTS = 5`, exponential backoff with jitter, budget reset on successful health probe or manual `start_daemon`. Budget exhaustion lands in `Stopped` with clear diagnostic. No unbounded retry loop.

### 4. Resource hygiene
**Pass.** Drain task is self-terminating (on `Terminated` or channel close). Monitor task (`spawn_monitor`) terminates when `handle_crash` completes. `Arc<Mutex<...>>` usage is correct throughout — `tokio::sync::Mutex` in async contexts, `blocking_lock()` only in the documented-safe `set_app_handle` call site. No leaked tasks on stop/quit paths.

### 5. Test reliability
**Pass.** All new sidecar tests use `#[tokio::test(flavor = "current_thread")]` — single-threaded, deterministic. They use `tauri::test::mock_app()` + health server to avoid the shell plugin dependency, correctly exercising the attach path. The tests genuinely verify the reliability claims: initial `Stopped` state, `Starting`-without-child non-suppression, Error-state retry, and budget reset.

## Cannot Verify
- Clean-state desktop smoke (requires interactive Tauri window in isolated `~/.nexus42` profile) — acknowledged as known smoke carry-over per Assignment.
- Existing-install desktop smoke (requires pre-existing config/workspace) — same carry-over.
- These are documented hard gates for PM Done, not QC review blockers.

## Completion Report v2

**Agent**: qc-specialist-3
**Task**: Plan-level QC review (performance + reliability focus) for V1.97 Desktop First-Launch Reliability Hardening
**Status**: Done
**Scope Delivered**: Full review of 16 files across the diff range, covering sidecar FSM, stderr capture, crash budget, resource hygiene, and test reliability.
**Artifacts**: `.mstar/plans/reports/2026-07-07-v1.97-desktop-first-launch-hardening/qc3.md`
**Validation**: Deep review triggered (S1+S2+S6). Reliability Lens + Performance Lens applied. 0 Critical, 2 Warning, 3 Suggestion findings.
**Issues/Risks**: 2 low-severity residuals proposed. No blocking issues.
**Plan Update**: None required.
**Handoff**: PM to review residual candidates and register in `status.json` if accepted.
**Git**: Report committed to `feature/v1.97-desktop-first-launch-hardening`.
