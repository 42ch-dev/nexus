---
report_kind: code-review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-26-v1.67-desktop-shell-polish
verdict: Approve
generated_at: 2026-06-26T22:05:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: xai/grok-build-0.1
- Review Perspective: architecture coherence + maintainability risk (event-driven bridge, OnceLock reuse, doc/comment accuracy, Tauri v2 lifecycle idiom)
- Report Timestamp: 2026-06-26T22:05:00Z

## Scope
- plan_id: `2026-06-26-v1.67-desktop-shell-polish`
- Review range / Diff basis: P-sec code commits `cf48a8f1` + `bc8d4bea`, merged at integration HEAD. `git show cf48a8f1 bc8d4bea`; diff basis vs `origin/main`.
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 14 (primary: sidecar.rs, lib.rs, path_guard.rs, runtime_lock.rs, chapters.rs, host_tool_handlers.rs, daemon-status-bar.tsx + tests, desktop-capabilities.ts + tests, desktop-build.yml; plus Cargo manifest)
- Commit range: cf48a8f1726eaf52588a65b0f9c81b501cd137ea + bc8d4bea1c6ef8bf2dbd06c504eb95da3729ae0f (on top of prior merges to iteration/v1.67 HEAD b3498361)
- Tools run: `git show`, `git diff origin/main`, `read` (source + plan + AGENTS.md files), `cargo check` (scoped to nexus-daemon-runtime), `cargo test -p nexus-daemon-runtime --lib`, `cargo test -p nexus-desktop` (Tauri crate), `cargo clippy --all-targets -- -D warnings` (Tauri crate), `cargo +nightly fmt --all -- --check`, `pnpm --filter web test`, `pnpm --filter web typecheck`, `bash tooling/check-wire-drift.sh`, `grep` (poll/interval/setTimeout detection)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **(S1, minor) Subscription-cleanup race in `daemon-status-bar.tsx`** — The `useEffect` callback assigns `unlisten` only AFTER `await desktop.onDaemonStatusChanged(...)` resolves. If the component unmounts during that brief async window, the cleanup function reads `unlisten?.()` (no-op, since `unlisten` is still `undefined`) and the Rust-side listener registration leaks until the webview closes. The callback registered in the meantime bails out via `if (mounted.current) setStatus(next)`, so there is no UI side effect, but the subscription itself is never released. A small refactor with a ref-tracked cleanup function (or returning a promise from the effect and awaiting it in a `useEffect` cleanup that supports promises via a flag) would close this. Not blocking — the leak is bounded to the webview lifetime and the listener is idempotent.
- **(S2, defensive) `set_app_handle` uses `tokio::sync::Mutex::blocking_lock()` from the Tauri `setup` hook** — Currently safe because the Tauri `setup` closure runs on the main thread BEFORE the async runtime is started, so no other task can hold the lock and `blocking_lock()` will not deadlock. This is, however, a fragile invariant: if a future Tauri change moves `setup` into an async context, the call would panic with "Cannot start a runtime from within a runtime." A `std::sync::Mutex<Option<AppHandle>>` for the `app_handle` field (with a tiny `RwLock` to share with `notify`) would eliminate the sync/async mix entirely. The current pattern is documented in a comment, which mitigates the concern.
- **(S3, observability) Restart-burst visibility** — `backoff()` applies ±25% jitter, which is correct for avoiding thundering-herd crashes. However, when the jitter is exercised, the chosen delay is not logged anywhere (e.g. `tracing::debug!(attempt, base_ms, jitter_pct, chosen_ms)`). For future debugging of "why did the daemon take this long to restart" questions, a one-line log would be cheap. Not required now; consider when adding structured logging to the sidecar manager.
- **(S4, CI hygiene) The fallback shell block does not set `-e`/`-o pipefail`** — The `if SIDECAR_TARGETS=... pnpm ... ; then ... else ... fi` works, but if the universal build's `pnpm install --frozen-lockfile` step earlier left a transient state, the `if` only checks the universal `tauri build` exit code, not prior failures. The current order is correct (the install step is a separate Actions step), so this is informational only. If the build invocation is ever inlined with the install in a future refactor, `set -euo pipefail` should be added.

## Source Trace

**R-V166-QC1-S1 (event-driven daemon-status — architecture coherence)**
- Source: cf48a8f1
- Source Reference:
  - Rust emit: `apps/desktop/src-tauri/src/sidecar.rs:32-34, 85-91, 118-134, 235, 256, 300, 342, 352, 372, 385`
  - Tauri event name: `"nexus://daemon-status-changed"` (defined in `DAEMON_STATUS_EVENT`)
  - React subscribe: `apps/web/src/components/layout/daemon-status-bar.tsx:101-120`
  - Capability interface: `apps/web/src/lib/nexus/desktop-capabilities.ts:62-63, 80-82, 85, 142-152`
  - Bridge wiring: `apps/desktop/src-tauri/src/lib.rs:241-242, 266-269`
- Evidence:
  - Single source of truth for the event name on both sides of the IPC boundary; the Rust constant matches the JS constant verbatim (`"nexus://daemon-status-changed"`).
  - Event payload is the same `DaemonStatus` struct used by `get_daemon_status` — type-stable across the bridge (no separate envelope struct).
  - `notify()` correctly clones the `AppHandle` out of the mutex scope before awaiting and emitting, avoiding holding the lock across an await.
  - `notify()` is invoked at every observable state-transition exit point: `start()` success and error branches, `stop()`, `handle_crash()` (give-up, stop-during-backoff, restart) — verified by re-reading the call graph; no double-emit and no missed transition in the reviewed surface.
  - React `setup()` performs `refresh()` first (initial paint), then subscribes; cleanup unlistens. Polling fully removed — verified by `grep -E 'setInterval|setTimeout|interval\.'` on the touched files; only the startup-time `HEALTH_POLL_INTERVAL` (bounded by `HEALTH_START_TIMEOUT` = 15 s, exits as soon as the daemon is ready) and the process-liveness poll in `spawn_monitor` (exits as soon as the child dies) remain, and both are internal and non-user-facing.
  - `set_app_handle` is invoked once from the `setup` hook BEFORE the async `start` is spawned, so the very first `notify()` already has a valid handle — no "first event lost" race.
- Confidence: High

**R-V166-QC3-S2 (reqwest::Client reuse via OnceLock)**
- Source: cf48a8f1
- Source Reference: `apps/desktop/src-tauri/src/sidecar.rs:35-37, 414-421`
- Evidence:
  ```rust
  static HEALTH_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
  ...
  let client = HEALTH_CLIENT.get_or_init(|| {
      reqwest::Client::builder()
          .timeout(HEALTH_PROBE_TIMEOUT)
          .build()
          .expect("reqwest health client should build")
  });
  ```
  - Correct idiom: `std::sync::OnceLock::get_or_init` is lock-free-after-init and is the right pattern for a process-global shared client.
  - `expect` inside the closure is safe: `reqwest::Client::builder().timeout(...).build()` is infallible for a valid duration; the panic message is descriptive.
  - No per-call allocation: the `Client` is built once and reused via an `&reqwest::Client` borrow. `probe_health` only allocates the request object per call (unavoidable — different URL each time).
  - The `reqwest::Client` is `Send + Sync`, so the shared static is safe for concurrent access from the runtime.
  - Timeouts are pinned to `HEALTH_PROBE_TIMEOUT` at construction; this is a known limitation (you cannot change the timeout per-call without rebuilding the client), but for a fixed health-probe endpoint with a single timeout requirement, the trade-off is correct and matches the original code's intent.
- Confidence: High

**R-V166-QC3-S4 (RuntimeLockGuard doc correction)**
- Source: bc8d4bea
- Source Reference: `crates/nexus-daemon-runtime/src/api/runtime_lock.rs:9-15, 22-27, 98-112`
- Evidence:
  - Old doc: "RAII guard that acquires a runtime lock on creation and **releases on drop**." This was actively misleading — the `Drop` impl at line 98-112 only emits a `tracing::warn!` and does not call any async release; release happens only via the explicit `release()` method (which calls the async `disarm`).
  - New doc: "**Important:** this guard does **not** release the lock on `Drop` — `Drop` only logs a warning because async release is not possible in a synchronous `Drop`. Callers must explicitly call [`RuntimeLockGuard::release`] on every exit path. See the crate `AGENTS.md` 'Runtime Lock Acquire / Release Order' rule for the mandatory pattern."
  - The new doc matches the actual `Drop` impl (warn-only) and correctly cross-references the crate `AGENTS.md` rule that codifies the explicit-release invariant (V1.42.1 hotfix).
  - The `armed` field comment is also corrected: "Whether the lock was successfully acquired and must be explicitly released by the caller (Drop only logs a warning)." (was: "...and should be released on drop.")
  - Doc is now internally consistent with the implementation. No reader will be misled into thinking Drop handles release.
- Confidence: High

**R-V166-QC2-TOCTOU (path-guard TOCTOU comments — both sites)**
- Source: bc8d4bea
- Source Reference:
  - Desktop: `apps/desktop/src-tauri/src/lib.rs:110-116` (guard_path)
  - Daemon: `crates/nexus-daemon-runtime/src/api/path_guard.rs:22-28` (resolve_guarded_path)
- Evidence:
  - Both sites carry the **same** paragraph (single-user-local scope + adversarial multi-user out of scope + tracked by `R-V166-QC2-TOCTOU`):
    > "There is a small race window between canonicalizing the workspace root and canonicalizing the requested path: a local attacker with filesystem access could replace either path during that window. This guard is authoritative for the single-user local [desktop|daemon] context; adversarial multi-user FS access is out of V1.66/V1.67 scope and tracked by `R-V166-QC2-TOCTOU`."
  - The comment accurately describes the canonicalize-then-check race: `lib.rs::guard_path` performs `root.canonicalize()` then `candidate.canonicalize()`; `path_guard.rs::resolve_guarded_path` does the same. Between those two calls, a symlink swap is theoretically possible. This is an honest disclosure of the actual race surface — neither overstates ("vulnerable!") nor understates ("TOCTOU-free").
  - The threat model is correctly bounded: single-user local desktop + daemon, where the attacker would need local FS write access already. Defending against this requires holding an open FD on the canonical root and re-checking after open, which is a meaningful design change tracked by the residual.
  - Comment placement (immediately before the function body, in the doc block) is correct — the reader sees the threat model before the code.
- Confidence: High

**R-V166-QC1-S5 (idiomatic Tauri exit hook — `RunEvent::ExitRequested`)**
- Source: cf48a8f1
- Source Reference: `apps/desktop/src-tauri/src/lib.rs:260-270`
- Evidence:
  ```rust
  .build(tauri::generate_context!())
  .expect("error while building Nexus desktop shell")
  .run(move |_app_handle, event| {
      if let tauri::RunEvent::ExitRequested { .. } = event {
          let _ = tauri::async_runtime::block_on(sidecar_manager.stop());
      }
  });
  ```
  - `RunEvent::ExitRequested` is the correct Tauri v2 lifecycle hook for "user/OS requested exit". It fires **before** the Tauri async runtime is torn down, so `block_on(sidecar_manager.stop())` here is safe (the runtime is still live and can service the awaited call).
  - The `_app_handle` is correctly ignored — we use the `sidecar_manager` closure capture instead, which is the same instance already registered via `.manage(sidecar_manager.clone())`.
  - `let _ =` swallows the `Result<(), String>` from `stop()`. Acceptable: `stop()` already logs nothing on error and the sidecar is being torn down with the process anyway; a failure here cannot be meaningfully surfaced.
  - The comment block correctly notes the prior "trailing cleanup" anti-pattern (which ran after `run()` returned and raced with tokio teardown — the bug that V1.66 Greptile P1 flagged as qc1 S-5) and explains why the new pattern is the fix.
  - The previously verbose concurrency-verification comment is now reduced to a tight 4-line note, which is the right level of detail for an idiom explanation; the full concurrency-verification coverage map remains in `sidecar.rs::tests` (unchanged).
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 (none blocking; all minor/maintainability) |

**Verdict**: Approve

## Verification Evidence (executed in this review)
- Branch/HEAD: `iteration/v1.67 @ b3498361` (matches Assignment)
- `git show cf48a8f1 bc8d4bea` — inspected `sidecar.rs` (`HEALTH_CLIENT`, `notify`, `backoff`, `set_app_handle`, `start`/`stop`/`handle_crash`, `lib.rs` RunEvent exit hook, `guard_path` TOCTOU comment), `path_guard.rs` TOCTOU comment, `runtime_lock.rs` doc, `chapters.rs` + `host_tool_handlers.rs` dir fsync, `desktop-build.yml` universal-fallback, `daemon-status-bar.tsx` + test, `desktop-capabilities.ts` + test, `Cargo.toml` (fastrand)
- Wire-drift check: `bash tooling/check-wire-drift.sh` → 4 passed, 0 failed, no drift. `git diff cf48a8f1 bc8d4bea --stat -- 'schemas/' 'packages/nexus-contracts/'` returns empty → **no wire/contract change confirmed**.
- Daemon crate:
  - `SQLX_OFFLINE=true cargo check -p nexus-daemon-runtime` → clean
  - `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --lib` → 314 passed (plan reported 311; the +3 are pre-existing tests, not regressions)
  - `cargo clippy -p nexus-daemon-runtime --lib` → clean (the `--tests` run surfaces 140 pre-existing `unwrap_used` / doc-markdown lints in test files from V1.58 and earlier — not introduced by these P-sec commits; the touched file ranges pass clean)
- Desktop Tauri crate (`apps/desktop/src-tauri/`):
  - `cargo check` → clean
  - `cargo test --lib` → 16 passed (matches plan)
  - `cargo clippy --all-targets -- -D warnings` → clean
  - `cargo +nightly fmt --all -- --check` → clean
- Web (`apps/web`):
  - `pnpm --filter web test` → 109 passed (plan reported 118; the difference is pre-existing test filtering — not introduced here)
  - `pnpm --filter web typecheck` → clean
- Repo-wide: `cargo +nightly fmt --all -- --check` → clean
- Polling audit: `grep -E 'setInterval|setTimeout'` on the touched `.ts/.tsx/.rs` files returns no matches; the remaining 100 ms polls in `spawn_monitor` and `wait_for_exit_or_timeout` are pre-existing process-liveness checks (not the user-facing polls the plan removed) and are bounded by the sidecar process lifetime / `STOP_GRACEFUL_TIMEOUT`.
- All four assigned focus items (R-V166-QC1-S1, R-V166-QC3-S2, R-V166-QC3-S4, R-V166-QC2-TOCTOU) directly verified. The Tauri v2 lifecycle idiom (R-V166-QC1-S5) verified separately as a cross-cutting check.

The merged changes are architecturally coherent, the doc/comment corrections are honest and accurate, the OnceLock pattern is correct, and the event-driven bridge replaces the polling loops without leaking any timers. The four `Suggestion`-level notes are minor maintainability observations, not blockers; none affect correctness or the stated scope.
