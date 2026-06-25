---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-25-v1.66-mid-meta-tracking"
verdict: "Approve"
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

---

## Revalidation (fix-wave-1, 2026-06-26)

- Re-review mode: targeted (qc3 blocking findings)
- Fix-wave diff verified: `766a2582..1e595fb5` (11 files, +237/-24)
- Scope strictly limited to W-1…W-4 + fixes F1/F2/F6/F7 (F3/F4/F5/F8 were qc1's — not re-opened)

### Finding re-validation
- **W-1 (F1, correctness) — RESOLVED.** `lib.rs` injects `window.__NEXUS_DAEMON_PORT__ = {port}` via `tauri::plugin::Builder::js_init_script` (runs before page JS); `tauri-client.ts` `resolveDesktopPort` checks the global (typeof number) **before** `process.env`. End-to-end `NEXUS_DAEMON_PORT=9420`: Rust→9420, global→9420, sidecar `--port 9420`, SPA→9420, base URL `http://127.0.0.1:9420`. Test `uses the injected Tauri global port when no explicit port is given` asserts `client.port===9420` + `fetchImpl` called with `http://127.0.0.1:9420/...`; `prefers the injected Tauri global over env var` asserts 7777 wins over env 8888. **9/9 vitest pass.** `daemon-runtime.md` §12.3 satisfied.
- **W-2 (F2) — RESOLVED.** `sidecar.rs` `status()` reads `(port, should_probe = state==Running && !owned)` under lock, releases lock, actively probes attached daemons via `probe_health(port)` (bounded 2s `HEALTH_PROBE_TIMEOUT` over loopback); on failure re-acquires lock with defensive guard before → `Error` + detail "external daemon stopped" + `version=None`. Owned daemons not re-probed (pid monitor owns liveness). New tests `attached_running_daemon_transitions_to_error_when_probe_fails` + `owned_running_daemon_does_not_probe_on_status` verify both branches. (cargo test blocked locally by F3/F4 build.rs binary gate — documented dev-prereq; test logic verified by reading.)
- **W-3 (F6) — RESOLVED.** `desktop-build.yml` adds `Swatinem/rust-cache@v2` keyed `desktop-macos-universal-aarch64-x86_64`, `workspaces: . + apps/desktop/src-tauri` — caches both repo-root `target/` + `apps/desktop/src-tauri/target/`.
- **W-4 (F7) — RESOLVED.** Path filter: `crates/nexus42/**` + `crates/nexus-daemon-runtime/**` → `crates/**` in **both** push + pull_request triggers.

### New findings introduced by the fix-wave
None within W-1…W-4 scope. (F3/F4 build.rs binary gate = fail-fast dev-experience improvement, not regression; F8 = qc1's, approved by qc1.)

### Updated verdict
**Approve** — all 4 blocking Warnings resolved with verifiable evidence (W-1 end-to-end via 9/9 vitest + §12.3; W-2 via test logic + bounded probe; W-3/W-4 via workflow diff). No new Critical/Warning. S-1…S-5 deferred V1.67+ (unchanged).
