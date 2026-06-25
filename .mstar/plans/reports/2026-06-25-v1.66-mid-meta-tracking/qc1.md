---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-25-v1.66-mid-meta-tracking"
verdict: "Request Changes"
generated_at: "2026-06-26"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Review Perspective: Architecture coherence and maintainability risk (reviewer 1 of 3)
- Report Timestamp: 2026-06-26T01:25:00Z

## Scope
- plan_id: 2026-06-25-v1.66-mid-meta-tracking (P-mid umbrella — multi-plan single tri-review per compass §3)
- Feature / scope label: V1.66 iteration integration — P0 desktop shell core + P-sec hygiene (3 residuals) + P1 sidecar lifecycle + macOS CI
- Review range / Diff basis: `merge-base: 6e1f18e0 (origin/main) + tip: c8d22976 (iteration/v1.66 HEAD)` — equivalent to `git diff 6e1f18e0...c8d22976` (118 files, +10020/-572)
- Working branch (verified): iteration/v1.66
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (repo root)
- Files reviewed: 118 changed; deep-read apps/desktop/src-tauri/{lib.rs,sidecar.rs,main.rs,tauri.conf.json,capabilities/main.json}, apps/web/src/lib/nexus/{tauri-client.ts,detect.ts,desktop-capabilities.ts,types.ts,browser-client.ts}, apps/web/src/components/{path-context-menu.tsx,layout/daemon-status-bar.tsx}, crates/nexus-daemon-runtime/src/api/{path_guard.rs,runtime_lock.rs,handlers/*}, .github/workflows/desktop-build.yml, scripts/fetch-sidecar.sh
- Tools run: git diff/log/show, cargo clippy --tests -p nexus-daemon-runtime, cargo test (chapters_api, pagination_info_parity), pnpm vitest (38 V1.66 web tests pass), cargo build smoke in src-tauri (reproduces W-1)

## Findings

### 🔴 Critical
(none)

### 🟡 Warning

**W-1 — `cargo build`/`test`/`clippy` fails on fresh checkout (sidecar binary gitignored but required at compile time).** Tauri v2 `tauri-build` enforces `externalBin` target-triple files resolve at compile time; on fresh clone `binaries/nexus42-aarch64-apple-darwin` is absent → any `cargo` command fails with opaque `resource path doesn't exist`. CI unaffected (beforeBuildCommand runs `pnpm -w run sidecar` first), but local `cargo clippy --all` (root AGENTS.md gate) fails. **Fix**: "Development prerequisites" block in `apps/desktop/AGENTS.md` + fail-fast pre-check pointing to `pnpm -w run sidecar`. Confidence: High (reproduced locally).

**W-2 — `apps/desktop/package.json::_p1_runtime_deps_note` is misleading.** Claims future `@tauri-apps/plugin-shell` + `@tauri-apps/api` JS deps; reality: plugin-shell is a Rust crate (Cargo.toml), and §5 #7 LOCKED uses `window.__TAURI__` global (no JS deps). **Fix**: rewrite note to as-built design. Confidence: High.

**W-3 — `apps/desktop/AGENTS.md` scope table stale.** Lists bundled sidecar lifecycle as "Out (V1.67+)" but P1 shipped it (`sidecar.rs`, `externalBin`, `shell:allow-execute`). **Fix**: move to "In"; note in-process lib link is the actual V1.67+ deferral. Confidence: High.

### 🟢 Suggestion
- **S-1** — `sidecar.rs::spawn_monitor` 100ms poll + `daemon-status-bar.tsx` 5s poll = two independent loops, UI can lag 5s. Future: event-driven via `AppHandle::emit`. (V1.67+)
- **S-2** — `sidecar.rs:198` `inner.detail.clone().unwrap()` safe today but brittle; use `unwrap_or_else(|| "Daemon did not start")`. (V1.67+)
- **S-3** — `daemon-status-bar.tsx` hard-codes "Port unavailable" label for all `error` states; Rust produces 2 distinct detail strings. Split label or use spec wording. **(promoted to fix F8 — one-line UX)**
- **S-4** — `desktop-build.yml` no §5 #10 conditional fallback to per-arch artifacts. (V1.67+)
- **S-5** — `lib.rs::run` trailing `block_on(stop)` after `Builder::run()`; idiomatic Tauri `on_window_event`/`RunEvent::ExitRequested` hook preferred. (V1.67+)

## Source Trace
- F-VALID-1 (§5 conformance): all 10 §5 LOCKED decisions verified faithful — `TauriClient extends BrowserClient` (21 methods, transport-parity test pins 21 `/v1/local/*` paths); externalBin + shell:allow-execute + sidecar:true; port 8420 + NEXUS_DAEMON_PORT + health probe; frontendDist; wire_contracts_changed:false; DESIGN.md supplement; NEXUS_DESKTOP + __TAURI_INTERNALS__ + withGlobalTauri; runtime canonicalize authoritative + custom commands; pnpm sibling + standalone src-tauri; macos-13 + universal + 90-day + path filter. Confidence: High.
- F-VALID-2 (hygiene closure): R-V165-QC1-W2 (chapters_api.rs 12 tests PASS); R-V165-QC-SUGG-DEFENSE (path_guard.rs + runtime_lock.rs dedup + 4 fsync sites); R-V164-QC1-S1-P0 (PaginationInfo consolidation across 5 handlers + parity test 3 PASS). Confidence: High.
- F-VALID-3: pnpm vitest 38 PASS; cargo test 15 PASS. F-VALID-4: clippy 0 warnings attributable to V1.66.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 5 |

**Verdict**: **Request Changes** — 3 unresolved Warnings (W-1 intersects the `cargo clippy --all` CI gate; W-2/W-3 are doc drift). All fixable in <30 LoC doc + 5-line pre-check. Architecture itself is sound (clean `TauriClient extends BrowserClient`, single capability-detection seam, authoritative runtime path guard, zero-behavior-change PaginationInfo extraction).
