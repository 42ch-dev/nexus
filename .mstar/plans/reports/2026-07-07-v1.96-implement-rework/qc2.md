---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-07-v1.96-implement-rework"
verdict: "Approve"
generated_at: "2026-07-07"
---

# Code Review Report — qc2 (security / correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk (stderr surfacing, error extraction, race discipline, token isolation, contract drift)
- Report Timestamp: 2026-07-07

## Scope
- **plan_id**: `2026-07-07-v1.96-implement-rework`
- **Review range / Diff basis**: `merge-base: f9b73d27 (iteration-start commit)` + `tip: HEAD (5adf3029)` — equivalent to `git diff f9b73d27...HEAD`
- **Working branch (verified)**: `feature/v1.96-implement-rework`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 20 files, +948/-115 (per branch-review-package)
- **Commits reviewed**: 9 commits (T1–T8 + fix waves)
- **Branch review-package**: `.mstar/sdd/2026-07-07-v1.96-implement-rework/branch-review-package.md`
- **Compass**: `.mstar/iterations/v1.96-setup-wizard-rework-and-daemon-diagnostic-delivery-compass-v1.md` (wire_contracts_changed: false)
- **Tools run**: `git diff`, `git log`, manual source reads (error-message.ts, sidecar.rs, setup-step-daemon.tsx, setup-step-welcome.tsx, tailwind.config.ts, index.css, DESIGN.md), grep for cancelled/timeout/toast patterns, contract diff check (schemas/ + contracts/ unchanged)

## Focus Areas (qc2 — security / correctness)

### 1. Stderr capture (T3 — sidecar.rs)
- **Implementation**: bounded async drain (`drain_stderr`) + `trim_stderr_tail` at 2 KiB, nearest newline boundary, tail only. Captured into `SidecarInner.stderr_tail`, cleared on new spawn. Appended verbatim only on Error transition via `format_error_detail`.
- **Security surface**: raw daemon stderr may contain file paths (`~/.nexus42/...`), config snippets, migration errors, or (future) user-scoped data. Surfaced to SPA via `DaemonStatus.detail` → wizard error UI + toasts.
- **Trust boundary assessment**: daemon is **local-first** (same-user process on user's machine). No cross-user or network exposure. Stderr is from the user's own `nexus42 daemon start` invocation. Acceptable within local trust boundary.
- **Mitigations present**:
  - Hard 2 KiB cap + newline trim (no unbounded buffer).
  - Only surfaced on terminal Error (not on every status).
  - Generic fallback preserved when tail empty.
- **Finding**: No injection or exfiltration risk. **Acceptable** with note that future sensitive data in stderr (e.g. secret paths) would still be visible to the local user — expected behavior.
- **Correctness**: drain task runs concurrently with health probe (no spawn-path block); child kill + lock release prevents deadlock; tail reset on new spawn verified in test.

### 2. errorMessage() helper (T1 — lib/error-message.ts)
- **Implementation**:
  ```ts
  export function errorMessage(err: unknown): string {
    if (err && typeof err === 'object' && 'message' in err) {
      const msg = (err as { message: unknown }).message;
      if (typeof msg === 'string' && msg.length > 0) return msg;
    }
    if (err instanceof Error) return err.message;
    if (typeof err === 'string') return err;
    return '';
  }
  ```
- **Correctness**: order is correct (`'message' in` first for Tauri plain-object errors, then `instanceof Error`). Returns '' for unknown shapes — callers supply fallback.
- **Security (XSS/injection)**: returns **plain string only**. No HTML construction, no `dangerouslySetInnerHTML`, no template interpolation into DOM. React/JSX `description` props escape by default. Used exclusively as `toast({ ..., description: message })` or `setError(...)` text nodes.
- **Tests**: 7 cases (Error, {message}, non-string message, string, undefined, null, plain {}) — all pass.
- **Finding**: **Clean**. No XSS surface introduced.

### 3. Toast pattern (T2) + global error routing
- All wizard error paths now route through `useToast()` (variant: 'error') + `errorMessage(err)`.
- Sites swept: welcome (browse + setWorkspacePath), main-banner, daemon-status-bar, preset-yaml, desktop-capabilities.
- No remaining inline `<p role="alert">` for Tauri errors in wizard steps.
- **XSS surface**: same as (2) — plain string into toast description. Toaster component (already present in main.tsx) renders as text.
- **Finding**: **Correct**. Unified pattern eliminates `[object Object]` and centralizes surface.

### 4. cancelled race discipline + timeout correctness (T4 — setup-step-daemon.tsx)
- **Guards**: `let cancelled = false;` set true in cleanup. Every `setReady` / `setError` / `applyStatus` path checks `if (cancelled) return;`.
- **Timeout**:
  - Single `setTimeout(..., 25_000)` scheduled once per effect run.
  - `clearTimeout(timeoutId)` in cleanup **and** on terminal states (running/degraded/error/stopped).
  - Re-probe path: after reset, `setRetryToken` forces effect re-run → fresh timeout.
  - Browser probe path also guarded.
- **Starting state**: explicit branch `status.state === 'starting'` → `setReady(false); setError(null);` (no longer a no-op).
- **Mount probe**: `getDaemonStatus()` called on subscribe before relying on events (catches "event already fired" case).
- **Tests**: cover mount-probe, starting branch, 25s timeout re-probe, subscription-throw + reset, stderr detail surfacing.
- **Finding**: **Correct**. No setState-after-unmount path. Timeout fires exactly once per mount and is cleared. Re-probe logic sound.

### 5. Token wiring (T5) — no shadowing of V1.95 tokens
- New tokens (21 surface + 1 step-row-height):
  - `setup-wizard-surface-*` (card, panels, input-row, cta, padding, colors)
  - `setup-wizard-step-row-height`
- **Preservation**: all 13 V1.95 `setup-wizard-step-*` tokens untouched (names + values). Compound doc `tailwind-theme-key-routing-for-sizing-tokens.md` respected.
- **Registration**: tailwind.config.ts (both groups), index.css (:root + .dark), DESIGN.md + DESIGN.dark.md (Level 3 Production).
- **Consumption**: wizard-page, all three steps, tests assert classes.
- **Finding**: **No collision**. New namespace is additive and isolated. Light/dark parity maintained.

### 6. Mutation / contract drift
- `git diff f9b73d27...HEAD -- schemas/ crates/nexus-contracts/` → empty (no output).
- Compass claim `wire_contracts_changed: false` holds.
- Desktop capabilities remain Tauri IPC (no new HTTP routes).
- **Finding**: **Verified clean**.

### 7. Test correctness
- New: `error-message.test.ts` (7 cases), `setup-step-agent.test.tsx`, `setup-step-done.test.tsx`, expanded `setup-step-daemon.test.tsx` (mount probe, starting, timeout, reset races, stderr detail).
- Assertions target the exact invariants (cancelled guards, timeout re-probe, stderr inclusion, CTA classes, token classes).
- No "happy path only" tests — error and race paths covered.
- **Finding**: **Adequate** for security/correctness scope.

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W-1 (accept within boundary)**: Raw daemon stderr (paths, config details, migration text) is now surfaced verbatim in `DaemonStatus.detail` and rendered in wizard error UI.  
  **Rationale**: Local-first daemon on user's own machine. 2 KiB cap + newline trim present. No network exfiltration. Acceptable for diagnostic value.  
  **Recommendation**: Document in future user-facing "troubleshooting" copy that error details may include local paths. No code change required for V1.96.

### 🟢 Suggestion
- **S-1**: Consider adding a future "copy error details" button on the daemon error state (already surfaced in `detail`) to make diagnostics easier for support without forcing users to screenshot.
- **S-2**: The 25 s timeout is inside the 30 s acceptance window from compass. If first-launch cold starts routinely exceed 20 s, consider surfacing a "still working" hint at 15 s before the hard message.

## Source Trace (key)
- Stderr logic: `sidecar.rs:263-294` (drain spawn), `298-327` (Error path), `243-254` (trim), tests `346-420`.
- errorMessage: `lib/error-message.ts:1-9`, test `702-736`.
- Daemon step race/timeout: `setup-step-daemon.tsx:24-110` (cancelled + timeout), `34-41` (starting branch), `77-103` (25s logic).
- Token surface: `tailwind.config.ts:107-`, `index.css:222-`, `DESIGN.md:891-`.
- Contract check: `git diff ... -- schemas/ crates/nexus-contracts/` (empty).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 (accepted in local boundary) |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Completion Report v2

**Agent**: qc-specialist-2  
**Task**: V1.96 plan QC (security/correctness lens) — full branch review  
**Status**: Done  
**Scope Delivered**: Reviewed all 8 SDD tasks + fix waves against compass focus areas (stderr trust boundary, error extraction safety, toast XSS, cancelled discipline, timeout correctness, token isolation, contract drift, test invariants).  
**Artifacts**: `.mstar/plans/reports/2026-07-07-v1.96-implement-rework/qc2.md` (this file)  
**Validation**:
- `git diff f9b73d27...HEAD` inspected for contracts/schemas (clean).
- Source reads + grep on cancelled/timeout/toast/errorMessage paths.
- Token registration cross-checked (tailwind + css + DESIGN + usage).
- All critical correctness guards (cancelled before every setState, timeout cleared, stderr cap) verified present.
**Issues/Risks**: One Warning (stderr content) accepted as within local trust boundary; no blocking security or correctness defects.
**Plan Update**: None required (no residual findings for qc2 to register).
**Handoff**: Ready for qc-consolidated + QA. All qc2 items pass.
**Git**: (to be recorded after commit)
