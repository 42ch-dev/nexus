---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-07-v1.96-implement-rework"
verdict: "Approve"
generated_at: "2026-07-07"
---

# Code Review Report — QC3 (Performance & Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: deepseek-v4-flash
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-07-07

## Scope
- plan_id: `2026-07-07-v1.96-implement-rework`
- Review range / Diff basis: `merge-base: f9b73d27` + `tip: HEAD (5adf3029)` — equivalent to `git diff f9b73d27...HEAD`
- Working branch (verified): `feature/v1.96-implement-rework`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 21
- Commit range: `f9b73d27..HEAD` (10 commits)
- Tools run: `git diff`, `git log`, manual source review

## Findings

### 🔴 Critical

#### C-1: `setup-wizard-page.tsx` `finish()` error handling missed the `errorMessage()` sweep (T2 residual)
- **Issue**: The `finish()` function in `setup-wizard-page.tsx` (line 54) still uses the old pattern `err instanceof Error ? err.message : 'Failed to save agent profile.'` instead of the shared `errorMessage()` helper. If `desktop.setAgentProfile()` throws a Tauri invoke error (plain object with `message` field, not an `Error` instance), the actual error message is silently dropped and replaced with the generic fallback string. The `errorMessage` import is not present in this file.
- **Fix**: Import `errorMessage` from `@/lib/error-message` and change to `const description = errorMessage(err) || 'Failed to save agent profile.';`
- **Severity**: Critical — this is a direct continuation of the P0 `[object Object]` bug class. While it no longer shows the literal `[object Object]` string (the fallback masks it), the actual error is silently discarded, making the diagnostic chain incomplete for the final setup step.
- **Source**: `apps/web/src/pages/setup-wizard-page.tsx:54`
- **Confidence**: High

### 🟡 Warning

#### W-1: Stderr drain task race on fast-crash path — no synchronization before reading intermediate buffer
- **Issue**: In `start_with_budget`'s error path (lines 297–335 of `sidecar.rs`), after killing the child, the code reads `stderr_tail` (the intermediate `Arc<Mutex<String>>` buffer) without waiting for the drain task to finish processing the `Terminated` event. The drain task only writes to this buffer *after* its receive loop exits (line 283–284). If the drain task hasn't yet processed the `Terminated` event (one async poll cycle), the main path reads an empty buffer and stderr is lost from the diagnostic message.
- **Impact**: On a daemon that crashes within milliseconds of spawn, there is a small window where stderr is not included in the error detail. The window is approximately one tokio poll cycle (sub-millisecond in practice), and the stderr emitted before the crash is already in the pipe buffer — the race is only about whether the drain task has finished copying the buffer. In the common case stderr is captured correctly.
- **Fix (optional)**: Store the `JoinHandle` from `tauri::async_runtime::spawn` and await it before reading `stderr_tail` on the error path. Alternatively, use a `tokio::sync::oneshot` channel from the drain task to signal completion.
- **Severity**: Warning (low practical impact, but a correctness gap)
- **Source**: `apps/desktop/src-tauri/src/sidecar.rs:263-274` (spawn), `:310-316` (read)
- **Confidence**: High

#### W-2: `errorMessage()` first branch catches `Error` instances redundantly — potential for confusion
- **Issue**: The `errorMessage()` function checks `'message' in err` before `err instanceof Error`. Since `Error` instances satisfy `'message' in err`, the first branch handles them. The second `err instanceof Error` branch is only reached when `message` is not a non-empty string. This works correctly but the ordering is counterintuitive — a reader might think `Error` instances are handled by the second branch. If the first branch's `typeof msg === 'string'` check is ever relaxed, `Error` instances with non-string `message` (e.g., `cause`) could produce unexpected results.
- **Fix**: Swap the order: check `err instanceof Error` first, then check for duck-typed `message`. This makes the intent clearer and avoids the subtle ordering dependency.
- **Severity**: Warning (low risk, but a maintainability concern)
- **Source**: `apps/web/src/lib/error-message.ts:2-4`
- **Confidence**: Medium

### 🟢 Suggestion

#### S-1: 25s wizard timeout should be a named constant
- **Issue**: The 25-second timeout in `setup-step-daemon.tsx` (line 103) is a hardcoded magic number. It is referenced in tests (`setup-step-daemon.test.tsx:230`) by repeating the literal `25_000`. A named constant would improve testability and documentation.
- **Fix**: Extract `const DAEMON_TIMEOUT_MS = 25_000;` at module scope.
- **Source**: `apps/web/src/pages/setup-step-daemon.tsx:103`
- **Confidence**: High

#### S-2: `setup-wizard-page.tsx` `finish()` error handling also misses `errorMessage()` (see C-1)
- **Note**: Already filed as C-1. Listed here for completeness — the fix is straightforward and should be applied before merge.

#### S-3: StepIndicator connector uses inline `style` for centering — consider a CSS variable
- **Issue**: The connector line centering uses `style={{ left: 'calc(var(--color-setup-wizard-step-circle-size) / 2)' }}` (line 122 of `setup-wizard-page.tsx`). This works but mixes concerns — the connector positioning logic is in JSX rather than CSS. If the circle size token changes, this inline style must be updated in sync.
- **Fix**: Define a `--setup-wizard-step-connector-left` CSS variable in `index.css` that computes the same `calc()` expression, and reference it via a Tailwind utility or `style` prop.
- **Source**: `apps/web/src/pages/setup-wizard-page.tsx:122`
- **Confidence**: Low (cosmetic, no functional impact)

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|------------------|------------|
| C-1 | manual-reasoning | `apps/web/src/pages/setup-wizard-page.tsx:54` | High |
| W-1 | manual-reasoning | `apps/desktop/src-tauri/src/sidecar.rs:263-274, 310-316` | High |
| W-2 | manual-reasoning | `apps/web/src/lib/error-message.ts:2-4` | Medium |
| S-1 | manual-reasoning | `apps/web/src/pages/setup-step-daemon.tsx:103` | High |
| S-3 | manual-reasoning | `apps/web/src/pages/setup-wizard-page.tsx:122` | Low |

## Per-Focus Analysis

### T3 — Stderr drain task (sidecar.rs)
- **Deadlock risk**: None. Lock order is consistent on both paths (stderr_tail → inner). The `inner` lock is released before `stderr_tail` is acquired in the main error path.
- **Memory bound**: 2 KiB cap with newline-boundary truncation is correct. `trim_stderr_tail` is called after every `push_str`, so `buf` never exceeds the cap.
- **Task termination**: The drain task breaks on `Terminated` event. When the daemon exits or is killed, the tauri shell plugin sends this event. The rx channel is closed when the command handle is dropped (on stop/restart). **No leaked task.**
- **⚠️ Race (W-1)**: No explicit synchronization before reading the intermediate buffer on the fast-crash path.

### T4 — Mount-probe + subscription (setup-step-daemon.tsx)
- **Double-event handling**: None. `applyStatus()` is idempotent for all state values. The mount probe and subscription can both fire for the same state without harmful side effects.
- **Timeout as sole re-probe trigger**: Correct. The 25s timeout is the only automatic re-probe. Retry/reset are user-initiated via `retryToken` in the effect dependency array.
- **Timeout cleanup**: `clearTimeout(timeoutId)` is called in `applyStatus` on terminal states and in the effect cleanup. **All paths covered.**
- **Subscription cleanup**: `unsub?.()` in effect cleanup. **Correct.**

### T5 — Token pipeline (tailwind.config.ts + index.css)
- **Build-time regression**: None. All 20 new tokens are CSS variable references (`cv(...)`). Tailwind generates utility classes from the token names, not values. Negligible overhead.
- **CSS variable bloat**: 42 new variables (21 light + 21 dark), ~3-6 KB total. Acceptable for a 622-line CSS file (~7% increase).
- **Token registration**: All sizing tokens are registered under the correct Tailwind theme keys (`spacing`, `maxWidth`, `padding`) per the V1.95 compound doc guardrail. **Correct.**

### T7 — Wizard-page restructure (setup-wizard-page.tsx)
- **Re-render risk**: None. `StepIndicator` is a simple O(4) loop with conditional classes. Step changes cause one re-render of the parent and the new step component mount.
- **Layout thrash**: Connector absolute positioning uses `style={{ left: 'calc(...)' }}` — a CSS calculation, not JS layout. No forced reflow. The `h-setup-wizard-step-row-height` is a CSS variable, not a JS-triggered measurement.

### P0 fix reliability — daemon crash loop scenario
- **Stderr per iteration**: Each restart spawns a new drain task and clears `stderr_tail`. On Error, the `detail` field contains stderr from that specific spawn attempt. **Correct.**
- **25s timeout vs 15s health timeout**: 25s > 15s, so the SidecarManager's health probe completes first. The wizard receives the Error event via subscription well before the timeout fires. The 25s timeout is a safety net for missed events. **No bad interaction.**
- **Mount-time probe catches missed events**: If the SPA subscribes after the only Error event fired, `getDaemonStatus()` returns the current state. **Correct.**

### Resource leaks
- **Timeouts**: `clearTimeout` on terminal states + effect cleanup. **Covered.**
- **Subscriptions**: `unsub?.()` on effect cleanup. **Covered.**
- **Async tasks**: `cancelled` flag prevents state updates after unmount. **Covered.**
- **Drain task**: Terminates on process exit or channel close. **Covered.**

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

**Reason**: C-1 is a blocking finding — the `finish()` error handler in `setup-wizard-page.tsx` was missed during the T2 `errorMessage()` sweep and silently drops Tauri invoke errors. This is a direct continuation of the P0 `[object Object]` bug class. The fix is a one-line change (import + use `errorMessage()`). W-1 and W-2 are non-blocking but should be reviewed; both have low practical impact.

**Blocking items to resolve before approval:**
1. **C-1**: Import `errorMessage` in `setup-wizard-page.tsx` and use it in the `finish()` catch handler.

## Revalidation (fix-wave 1)

- **Commit**: `acbbbe1a`
- **Scope**: C-1 only (targeted re-review per PM instruction)

### C-1: `setup-wizard-page.tsx` `finish()` error handling — ✅ Fixed

- `setup-wizard-page.tsx:55`: now uses `errorMessage(err) || 'Failed to save agent profile.'` — correct.
- `setup-gate.tsx:50`: now uses `errorMessage(err) || 'Daemon is not responding.'` — correct (also addresses qc1 S-001).
- Both files import `errorMessage` from `@/lib/error-message`.
- `rg "instanceof Error"` on both files: **0 hits**.

The fix exactly matches the prescribed remediation. No residual `instanceof Error` pattern remains in either file.

### Updated verdict

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (non-blocking) |
| 🟢 Suggestion | 2 (non-blocking) |

**Verdict**: ✅ **Approve** — C-1 resolved. W-1, W-2, S-1, S-3 remain as non-blocking items for PM to register as residuals or address in a follow-up.
