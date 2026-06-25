---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-25-v1.66-mid-meta-tracking"
verdict: "Request Changes"
generated_at: "2026-06-26"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Review Perspective: Performance and reliability risk (reviewer 3 of 3)
- Tools run: git diff, read, grep (code review only; no local build)

## Scope
- plan_id: 2026-06-25-v1.66-mid-meta-tracking (P-mid umbrella — multi-plan single tri-review per compass §3)
- Feature / scope label: V1.66 iteration integration — P0 desktop shell core + P-sec hygiene (3 residuals) + P1 sidecar lifecycle + macOS CI
- Review range / Diff basis: `merge-base: 6e1f18e0 (origin/main) + tip: c8d22976 (iteration/v1.66 HEAD)` — equivalent to `git diff 6e1f18e0...c8d22976` (118 files, +10020/-572)
- Working branch (verified): iteration/v1.66
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Commit range note: local HEAD e3f57923 (harness status-only); code reviewed against assigned tip c8d22976

## Findings

### 🔴 Critical
None.

### 🟡 Warning

**W-1 — Resolved daemon port NOT exposed to SPA — `TauriClient` hits wrong origin when `NEXUS_DAEMON_PORT` overridden.** Rust launcher resolves port (`NEXUS_DAEMON_PORT`→8420) + passes `--port <resolved>` to sidecar. JS `TauriClient` re-derives via `process.env?.NEXUS_DAEMON_PORT`→8420, but `process` is undefined in Tauri webview → SPA always uses 8420. If user sets `NEXUS_DAEMON_PORT`, sidecar runs on custom port but SPA fetches 127.0.0.1:8420 → all Local API calls fail while status shows "running". Violates `daemon-runtime.md` §12.3 ("Expose the resolved daemon base URL to the SPA client factory"). **Fix**: inject resolved port as webview global (`window.__NEXUS_DAEMON_PORT__`) or `get_daemon_port` Tauri command; `TauriClient` uses authoritative port. **(correctness bug)** Confidence: High.

**W-2 — Attached (non-owned) daemon crash never detected — status indicator stays "running" indefinitely.** When launcher finds healthy daemon on resolved port, sets `owned=false`, no pid-monitor spawned; `get_daemon_status` returns cached state without re-probe. If external daemon crashes, UI shows "running" until data commands time out. **Fix**: health-reprobe loop for attached daemons, OR active probe in `getDaemonStatus` (short timeout); transition to error/stopped + offer Restart. Confidence: High.

**W-3 — CI `desktop-build` has no Rust/incremental cache.** Workflow installs toolchain + pnpm but no `Swatinem/rust-cache@v2`; `beforeBuildCommand` builds nexus42 for both archs from scratch + Tauri rebuilds wrapper → ~15–25 min cold builds every push; flakiness/timeout risk. **Fix**: add `Swatinem/rust-cache@v2` keyed to desktop job + both targets; cache `apps/desktop/src-tauri/target` + repo-root `target`. Confidence: High.

**W-4 — CI path filter incomplete — transitive Rust crate changes won't trigger build.** Filter has `crates/nexus42/**` + `crates/nexus-daemon-runtime/**` but not the other workspace crates nexus42 depends on (nexus-local-db, nexus-orchestration, nexus-kb, etc.). A PR touching only those produces a different nexus42 binary but desktop build won't run → packaging regressions reach main. **Fix**: add `crates/**` to path filter. Confidence: High.

### 🟢 Suggestion (deferred V1.67+)
- **S-1** — Manual `startDaemon` should reset `restart_count` (currently stays at 5 after give-up; one fresh crash re-enters Stopped). Reset at top of `start()`. Medium.
- **S-2** — `probe_health` allocates new `reqwest::Client` per call (~60 during 15s startup window). Store one client in SidecarManager. High.
- **S-3** — Atomic rename durability missing directory fsync (file fsync'd before + after rename, but not the parent dir entry). Open parent dir + sync_all() after rename. Medium.
- **S-4** — `RuntimeLockGuard` doc comment misleading ("releases on drop" but Drop only logs; release is explicit). Update doc. High.
- **S-5** — Restart backoff lacks jitter (pure 500ms×2^n). Add rand(0..250ms). Low.

## Source Trace
| Finding | Source | Reference | Confidence |
|---------|--------|-----------|------------|
| W-1 | doc-rule + reasoning | daemon-runtime.md §12.3; lib.rs:206; tauri-client.ts:39; client-context.tsx:44 | High |
| W-2 | reasoning | sidecar.rs:138-143, 108-115; daemon-status-bar.tsx:87-111 | High |
| W-3 | reasoning | desktop-build.yml:46-81 | High |
| W-4 | reasoning | desktop-build.yml:7-32; Cargo.toml workspace; crates/nexus42/Cargo.toml:21-69 | High |
| S-1..S-5 | reasoning | sidecar.rs / chapters.rs / host_tool_handlers.rs / runtime_lock.rs | Med/High/Low |

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 5 |

**Verdict**: **Request Changes** — sidecar lifecycle directionally correct (bounded startup timeout, bounded restart backoff with cap, graceful SIGTERM→SIGKILL, pid-based liveness, port-conflict detection). P-sec hygiene sound (shared path guard, deduped RuntimeLockGuard, temp→rename→fsync). But W-1 (port exposure) is a correctness bug breaking non-default port usage; W-2 (attached-daemon health) lets UI lie about running state; W-3/W-4 are CI-reliability gaps. Recommend fixing W-1 + W-2 before approval; W-3 + W-4 also count (CI-reliability ≥ Warning). S-1..S-5 deferred V1.67+.
