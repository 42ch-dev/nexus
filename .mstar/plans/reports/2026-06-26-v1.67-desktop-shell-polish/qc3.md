---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-26-v1.67-desktop-shell-polish"
verdict: "Request Changes"
generated_at: "2026-06-26T13:41:20Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: performance + reliability risk
- Report Timestamp: 2026-06-26T13:41:20Z

## Scope
- plan_id: `2026-06-26-v1.67-desktop-shell-polish`
- Review range / Diff basis: P-sec code commits `cf48a8f1` + `bc8d4bea`, merged at integration HEAD. `git show cf48a8f1 bc8d4bea`; diff basis vs `origin/main`.
- Working branch (verified): `iteration/v1.67`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 10 primary files (`apps/desktop/src-tauri/src/sidecar.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/Cargo.lock`, `apps/web/src/components/layout/daemon-status-bar.tsx`, `apps/web/src/components/layout/daemon-status-bar.test.tsx`, `apps/web/src/lib/nexus/desktop-capabilities.ts`, `apps/web/src/lib/nexus/desktop-capabilities.test.ts`, `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs`, `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs`) plus focused reads of `runtime_lock.rs`, `path_guard.rs`, the plan, and local AGENTS rules.
- Commit range: assigned commits `cf48a8f1` + `bc8d4bea` on integration HEAD `b3498361`; compared in context against `origin/main...HEAD`.
- Tools run: env bootstrap; `git rev-parse --show-toplevel`; `git branch --show-current`; `git rev-parse HEAD`; `git status --short`; `git show cf48a8f1 bc8d4bea`; `git diff --stat origin/main...HEAD`; `git log --oneline origin/main..HEAD`; GitNexus query (no useful indexed desktop symbols returned); focused `grep` / `read`; `cargo test` in `apps/desktop/src-tauri`; `cargo clippy -- -D warnings` in `apps/desktop/src-tauri`; `pnpm --filter web test -- --run src/components/layout/daemon-status-bar.test.tsx src/lib/nexus/desktop-capabilities.test.ts`; `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --lib`; `SQLX_OFFLINE=true cargo clippy -p nexus-daemon-runtime -- -D warnings`.

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-001 — Event-driven daemon status can silently remain stale forever after a missed event.** `DaemonStatusBar` now does one initial `getDaemonStatus()` and subscribes to `onDaemonStatusChanged()`, with the prior `POLL_MS` loop fully removed. However, there is no fallback re-sync if the Tauri event is missed: Rust `notify()` intentionally ignores `emit` errors, the React setup has a refresh→subscribe gap where a transition can occur before the listener is registered, and `onDaemonStatusChanged()` failure leaves no retry timer or periodic reconciliation. Result: a missed `Running → Degraded/Error/Stopped` transition can leave the footer showing the last-known status indefinitely until a manual action, remount, or app restart. This does not meet the reliability intent of R-V166-QC1-S1 as assigned. -> Add a low-frequency fallback reconciliation (for example, much slower than the removed 5s loop and/or only when no event has been received recently), a subscribe-failure retry, or another deterministic re-sync path; update the stale comment in `refresh()` that still says “the next poll will retry.”
- **W-002 — `restart_count` reset also applies to automatic crash restarts, so the bounded retry cap is no longer intact.** `start()` resets `inner.restart_count = 0` for every call. `handle_crash()` uses `self.start(app).await` for automatic restarts after incrementing the counter, so a successful auto-restart immediately zeroes the crash budget. Repeated crash → auto-start cycles therefore never reach `MAX_RESTART_ATTEMPTS`; they keep retrying with first-attempt jittered backoff rather than the intended bounded exponential sequence. This closes “manual start can recover after give-up” but regresses the cap semantics required by the sidecar reliability design and R-V166-QC3-S5’s “bounded retry cap intact” expectation. -> Split manual-start reset from monitor-driven restart (or add an internal start path that preserves the crash budget for auto-restarts) and add a test that repeated crash cycles eventually transition to `Stopped` after `MAX_RESTART_ATTEMPTS`.

### 🟢 Suggestion
- **S-001 — Directory fsync is in the right order, but durability errors now fail the API after rename.** The reviewed paths fsync the staged temp file, perform atomic rename, fsync the final file, and then fsync the parent directory. That ordering is appropriate for full crash durability. As a follow-up, consider documenting the API semantics if parent-dir fsync fails after the rename already succeeded: callers see an error even though file content may already be visible on disk.

## Source Trace

### W-001
- Finding ID: W-001
- Source Type: manual-reasoning + git-diff
- Source Reference: `apps/web/src/components/layout/daemon-status-bar.tsx:91-120`, `apps/desktop/src-tauri/src/sidecar.rs:125-133`, `cf48a8f1` diff deleting `POLL_MS` / `setTimeout`.
- Evidence:
  - React does only `await refresh(); unlisten = await desktop.onDaemonStatusChanged(...)` and has no timer after cleanup of the old `POLL_MS` loop.
  - Rust `notify()` uses `let _ = app_handle.emit(...)`, so delivery failure is intentionally unobservable to the state machine and not retried.
  - Focused grep found no `POLL_MS`, `setInterval`, or daemon-status fallback timer in `DaemonStatusBar`; the remaining `DaemonHealthIndicator` 10s poll is a separate browser header health probe and does not update the desktop footer state.
- Confidence: High

### W-002
- Finding ID: W-002
- Source Type: manual-reasoning + git-diff
- Source Reference: `apps/desktop/src-tauri/src/sidecar.rs:175-186`, `apps/desktop/src-tauri/src/sidecar.rs:323-377`, `apps/desktop/src-tauri/src/sidecar.rs:404-412`.
- Evidence:
  - `handle_crash()` snapshots `attempts = inner.restart_count`, increments `inner.restart_count += 1`, sleeps `backoff(attempts + 1)`, then calls `self.start(app).await`.
  - `start()` unconditionally resets `inner.restart_count = 0` before probing/spawning; this path is used by both manual `startDaemon` and automatic monitor restarts.
  - Existing test coverage validates jitter bounds and stop-during-backoff, but not repeated crash cycles reaching `MAX_RESTART_ATTEMPTS`.
- Confidence: High

### S-001
- Finding ID: S-001
- Source Type: git-diff + manual-reasoning
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:275-289`, `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:1491-1529`, `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:2326-2365`.
- Evidence: Each path performs temp-file write + temp-file `sync_all()`, `rename`, final-file `sync_all()`, then parent-directory `sync_all()`.
- Confidence: High

## Additional Checks
- **R-V166-QC3-S2:** Verified `probe_health()` uses a static `OnceLock<reqwest::Client>` with a client-level timeout, eliminating per-probe client / pool allocation churn.
- **R-V166-QC3-S3:** Verified parent directory fsync is present after atomic rename and after final-file fsync in the reviewed daemon write paths.
- **R-V166-QC3-S5:** Jitter bounds are implemented as ±25% around the capped exponential base (`75..=125`, then min with 8s). The jitter math itself is acceptable; the retry cap regression is caused by W-002.
- **Polling removal:** The old React 5s daemon-status-bar timer is fully removed. The 100ms Rust sleeps that remain are process-liveness waits (`spawn_monitor`, graceful stop timeout), not a status-refresh bridge; they are outside the removed UI polling loop but still worth tracking as liveness polling primitives.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes
