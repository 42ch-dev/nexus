---
iteration_id: V1.86
start_date: 2026-07-03
end_date: 2026-07-03
status: completed
iteration_base_branch: main
spec_integration_branch: iteration/v1.86
target_branch: main
plans:
  - 2026-07-03-v1.86-local-api-trust-hardening
---

# V1.86 — Local API Trust Boundary Hardening — Delivery Compass V1

**Status**: active (draft, pre Review & Edit chain). Three Cursor security reports verified against source and confirmed to compound into a single attack chain on the Local API trust boundary. `wire_contracts_changed: false` (pure runtime/security). PR to `main` after Phase 3.

## 0. Context

Three Cursor security reports (commit `49a14b2e`) were submitted against the daemon Local API. All three were verified by the PM against source — none is a false positive — and they **compound into one chain**: the CORS report is the *remote-reach* vector; the fs/* and path-traversal reports are the *impact amplifiers*.

| # | Severity | Location (verified) | Class |
|---|----------|---------------------|-------|
| 1 | High | `api/mod.rs:515` `CorsLayer::permissive()` over the whole router + `auth_middleware.rs:201-248` keyless-localhost accepts any loopback | Permissive CORS + keyless reach |
| 2 | High | `host_tool_handlers.rs:82-94` fs/* permission + `validate_file_path` skipped entirely when `workspace_path()` is `None`; `execute_read_file`/`execute_write_file` (534-591) operate on the raw path with no secondary guard | Trust-boundary bypass |
| 3 | Medium | `host_tool_handlers.rs:738-739` uses string-prefix `abs_requested_str.starts_with(&workspace_path_str)` for non-existing write targets; `path_guard.rs:60-62` itself documents this exact anti-pattern | Path-traversal (sibling escape) |

**Attack chain**: with the default `NEXUS42_DAEMON_API_KEY` unset (keyless-localhost), any website the user visits can send a cross-origin `fetch` to `http://127.0.0.1:8420`. Permissive CORS lets the preflight through. The browser TCP connection is loopback → `is_loopback_request()` returns true → full protected API access is granted. Finding 2 then reads or writes arbitrary user files (`~/.nexus42/auth.json`, `~/.ssh/id_rsa`, …) and Finding 3 widens writes to sibling directories.

This directly undermines **STRATEGY.md Guiding Principle #1 — "Local-first privacy"**. It is squarely on-strategy and on-theme with prior hardening iterations (V1.58 workspace OCC hardening, V1.63/V1.67 local-API foundation/convergence). The daemon-runtime spec §4.4.3/§4.5 already normatively require `require_api_key` on data routes and a "W-002-style workspace path guard" on file routes — so normative authority for all three fixes already exists; `path_guard.rs` already implements the correct component-wise comparison that Finding 3 should reuse.

The residual landscape confirms the class is real and recurring: five historical residuals are the **same bug class** — `R-V157P1-W001`, `R-V156P0-M004`, `R-V156P0-M002`, `R-V166-QC2-TOCTOU`, `R-V165-QC-SUGG-DEFENSE`. All five are marked `lifecycle: resolved` in `status.json` (resolved in V1.58 / V1.66 / V1.67), **yet the class clearly persists** — Finding 2 went undetected precisely because `R-V157P1-W001`'s coverage resolution was insufficient, and `execute_read_file` still uses blocking `std::fs::read_to_string` despite `R-V156P0-M004` being "resolved". P1 therefore re-verifies those prior resolutions against current `main` and re-hardens where they did not hold; where a gap is confirmed, V1.86 opens a **fresh** residual documenting the regression-of-resolution rather than reopening the historical row.

## 1. Locked Decisions

| Decision | Resolution |
|---|---|
| Iteration direction | **Core 3 findings (P0, must-fix) + same-class path-guard/coverage defense-in-depth (P1).** Non-goal: CSRF framework, keyless-mode deprecation, Unix-socket listener. A single coherent "Local API trust-boundary hardening" iteration. |
| Branch policy | `iteration_base_branch=main` (HEAD post-V1.85, PR #112); `spec_integration_branch=iteration/v1.86`; final `target_branch=main`. Matches documented project convention (`.mstar/AGENTS.md` multi-plan branch model + V1.39–V1.85 history). |
| Plan structure | **Single plan, single feature branch `feature/v1.86-local-api-trust-hardening`, two sequential batches.** Batch 1 = P0 (the 3 holes + their regression tests); Batch 2 = P1 (same-class defense-in-depth + prior-resolution verification). No parallelism: all work touches the same files (`api/mod.rs`, `api/handlers/host_tool_handlers.rs`, `api/path_guard.rs`, `workspace/`), so parallel branches would only add merge overhead. |
| CORS design (Finding 1) | **Origin allowlist gate, not a token framework.** Reject any request whose `Origin` header is not in the allowlist; allow requests with **no** `Origin` header (non-browser clients — CLI, workers, `curl` — already authenticated by loopback/key; browsers always emit `Origin` on cross-origin requests, so no-Origin ≡ non-browser). Allowlist = the daemon's own listening origin(s) (`http://127.0.0.1:<port>`, port from `NEXUS_DAEMON_PORT`/default 8420) + Tauri webview origins (`tauri://localhost` macOS, `http://tauri.localhost` Windows — required: `TauriClient` reuses the identical HTTP transport to the daemon) + Vite dev origin (`http://localhost:5173`) + optional `NEXUS_DAEMON_ALLOWED_ORIGINS` env override. Replace `CorsLayer::permissive()` with a configured `CorsLayer` (explicit `allowed_origin` set) **plus** an Origin-reject middleware as defense-in-depth. No CSRF tokens (non-goal). |
| keyless-localhost mode (Finding 1 enabler) | **Keep as default** (deprecation is a non-goal). The Origin allowlist gate is what now makes keyless-localhost safe against the browser cross-origin vector: the malicious site's `Origin` (`https://evil.com`) is not in the allowlist → rejected even though the TCP connection is loopback. Defense-in-depth: Finding 2's deny-without-workspace bounds the fs surface regardless. |
| fs/* without workspace (Finding 2) | **Deny unconditionally.** When `workspace_path()` is `None`, fs/* tools return a clear `403`/`BadRequest` ("fs/* tools require an active workspace with defined bounds") before `execute_read_file`/`execute_write_file` run. No sandbox-dir fallback (YAGNI; deny-by-default is the safe primitive). |
| Path validation (Finding 3) | **Replace the string-prefix comparison with component-wise `Path::starts_with`** by delegating `validate_file_path` to the canonical `resolve_guarded_path` / `canonicalize + Path::starts_with` pattern already in `path_guard.rs` (which itself documents the anti-pattern at lines 60-62). Eliminates the duplicated logic and the sibling-escape. |
| Contract impact | **None.** No `schemas/` change, no `@42ch/nexus-contracts` bump, no Local API DTO change, no `schema_version` bump. `wire_contracts_changed: false`. Pure runtime/security behavior change. Pre-1.0 → no deprecation period. |
| Regression coverage | Each P0 fix lands **with its regression test in the same task**: (1) cross-origin `Origin` request rejected / same-origin + Tauri + no-Origin allowed; (2) fs/* denied when no active workspace; (3) sibling-prefix escape rejected. Plus a dedicated coverage task that replaces the `#[ignore]` privileged-path smoke tests with real automated coverage of the fs/* admission + path-guard surface — this backfills the coverage gap that `R-V157P1-W001`'s V1.58 resolution failed to close (Finding 2 slipped through it). |
| Spec amendment | Add a **security section** to `specs/daemon-runtime.md` codifying the Local API trust-boundary contract (Origin allowlist, deny-fs-without-workspace, component-wise path guard). Minimal; the normative hooks already exist in §4.4.3/§4.5. Architect-owned during Prepare. |

## 2. Scope

This iteration locks two delivery spec points plus closure:

- **SP-1: Local API Trust Boundary Hardening — P0 (security-urgent headline).** Close the three verified findings: (a) replace `CorsLayer::permissive()` with an Origin allowlist CORS layer + Origin-reject defense-in-depth; (b) deny fs/* tools unconditionally when no active workspace is configured; (c) replace the string-prefix path comparison in `validate_file_path` with component-wise `Path::starts_with` via `resolve_guarded_path`. Each fix ships with a regression test. Closes the remote-reach + arbitrary-file-R/W + sibling-escape attack chain.
- **SP-2: Same-Class Defense-in-Depth + Prior-Resolution Verification — P1 (companion).** Five historical same-class residuals are all marked `lifecycle: resolved` in earlier iterations, but the class persists, so P1 **re-verifies each prior resolution against current `main` and re-hardens where it did not hold**: `R-V157P1-W001` (fs/* privileged-path coverage — resolved V1.58 but Finding 2 slipped through it; backfill real automated coverage), `R-V156P0-M004` (blocking sync I/O in the async fs handler — resolved V1.58 but `execute_read_file` still blocking; move behind `spawn_blocking`), `R-V166-QC2-TOCTOU` (path-guard TOCTOU — resolved V1.67; verify the handling still holds, document/refresh), `R-V156P0-M002` (session/hash-walk path boundary canonicalization — resolved V1.58 per spec overlay; verify it is actually enforced, no-op if so), `R-V165-QC-SUGG-DEFENSE` (write-path guard parity + post-rename fsync — resolved V1.66; parity mostly subsumed by the Finding 3 rewrite, verify the fsync). Where verification confirms a prior resolution was insufficient, V1.86 opens a **fresh** residual documenting the regression-of-resolution (historical rows are not reopened). Same files, same reviewer mental model.
- **SP-3: Closure.** QC tri-review (security lens via qc-specialist-2) + QA + compound + Profile B compaction + PR to `main`.

## 2.1 Architecture Hierarchy and Ownership

- **All implementation lives in `crates/nexus-daemon-runtime/src/`**: `api/mod.rs` (CORS layer), `api/auth_middleware.rs` (no behavior change, but the Origin gate may layer here or as a sibling middleware), `api/handlers/host_tool_handlers.rs` (fs/* deny, `validate_file_path` rewrite), `api/path_guard.rs` (canonical helper reused/extended), `workspace/mod.rs` (workspace_path semantics, unchanged).
- **Single owner, single feature branch.** No parallel tracks — every task edits overlapping files. The implementor works sequentially: Batch 1 (P0) → Batch 2 (P1), each task commit → Completion Report v2 → PM status sync.
- **Spec owner**: `@architect` amends `specs/daemon-runtime.md` with the trust-boundary security section during Prepare (Phase 1 Review & Edit chain).
- **Out of bounds**: `apps/web/**`, `apps/desktop/**` runtime code (the CORS fix must keep Tauri + web working, but no client-side change is required — clients already send appropriate Origins). The only `apps/` touch is verification (dev proxy / Tauri webview still connect).

## 2.2 Product Success Criteria

- **The three attack paths are demonstrably closed** by automated regression tests: (1) a cross-origin request (attacker `Origin`) is rejected while same-origin, Tauri-webview, and no-Origin requests succeed; (2) fs/* tools are denied with a clear error when no active workspace is configured; (3) a sibling-prefix write target (e.g. `/home/user/my-novel-evil/x` vs workspace `/home/user/my-novel`) is rejected.
- `CorsLayer::permissive()` is gone from `api/mod.rs`; replaced by an explicit allowlist + Origin-reject defense-in-depth.
- Desktop app still connects (Tauri webview origin allowlisted) and the web dev flow (Vite proxy) still works — verified, not just asserted.
- **Non-browser and direct-navigation flows remain working**: CLI host-call, `curl`, worker IPC, and `nexus42 daemon ui` (direct browser tab navigation to the Local API) all succeed. These paths send either no `Origin` header or an allowlisted origin.
- **Trust boundary is observable**: when keyless-localhost is active, the daemon logs the effective Origin allowlist at INFO level on startup (or on first protected request) so the user can see exactly which origins are trusted.
- The five P1 prior resolutions are each **verified against current `main`**: where they hold, recorded as confirmed (no-op); where they are insufficient, the gap is re-hardened and a **fresh V1.86 residual** documents the regression-of-resolution (historical rows are not reopened). No narrative-only deferral.
- No wire/schema/contract change (`wire_contracts_changed: false`).
- `specs/daemon-runtime.md` carries a codified Local API trust-boundary security section.
- QC tri-review consolidated Approve; QA verifies the regression tests reproduce the original attack paths as failing-without-the-fix.

## 3. Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-03-v1.86-local-api-trust-hardening` | Local API Trust-Boundary Hardening (P0 holes + P1 same-class sweep) | Done | Single plan, single feature branch, 2 batches. Impl `ec54e15a` (merge `b2cdcfd6`); QC 3/3 Approve (`9347d337`); QA Pass (`b830b540`). 5 residuals recorded (2 regress-of-resolution resolved, 1 medium `R-V186-QC1-S005` deferred, 2 low perf accepted). |

## 4. Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Compass + plan locked (Phase 1 Review & Edit chain done) | 2026-07-03 | in progress |
| P0 (3 holes) implemented + regression tests green | 2026-07-04 | pending |
| P1 defense-in-depth + prior-resolution verification implemented | 2026-07-04 | pending |
| QC tri-review Approve | 2026-07-05 | pending |
| QA + iteration-close + PR to `main` | 2026-07-05 | pending |

## 5. Acceptance Criteria

- The verified three-finding attack chain is closed end-to-end (each link fixed + regression-tested).
- No regression in existing Local API / CLI / desktop flows (`cargo test -p nexus-daemon-runtime`, desktop dev connect, web dev proxy).
- `cargo clippy --all -- -D warnings` and `cargo +nightly-2026-06-26 fmt --all --check` pass (CI gate).
- P1 prior resolutions verified against current `main`; where insufficient, re-hardened and a fresh V1.86 residual recorded (historical rows not reopened).
- Spec amendment landed; compass `status: completed` at Phase 3.

## 6. Non-Goals

- **CSRF token framework** for state-changing operations (the Origin allowlist is the chosen defense; a token framework is a separate, larger design).
- **Deprecating keyless-localhost mode** or auto-generating/forcing an API key (product-level UX decision; out of scope).
- **Switching the listener to Unix-socket-only** (architectural; out of scope).
- **Broader auth-mode redesign**, OAuth/token rotation for the local API, or `~/.nexus42/auth.json` hardening beyond what the fs deny directly protects.
- **Unrelated low residuals** (cron/kb/findings/etc.) — only the five named same-class prior resolutions are re-verified in P1; no other residuals are pulled in.
- **Startup banner / modal for security posture** — the daemon already logs the Local API URL; the new Origin allowlist (and keyless-localhost default) will be observable via the logged allowlist and the trust-boundary spec section. A user-facing "security posture" notification at startup is deferred (would be a product surface change across CLI/desktop).

## 7. Roadmap Position

- **Current iteration (V1.86)** — **delivered**: closed the 3-finding Local API trust-boundary attack chain exposed by external Cursor review (permissive CORS + keyless-localhost remote reach; fs/* bypass without workspace; string-prefix path traversal) as P0, plus a same-class P1 sweep. QC 3/3 Approve (qc2 deep security lens: all 3 attack paths CLOSED, no bypass); QA Pass. `wire_contracts_changed: false`. One latent same-class instance remains in a non-fs/* tool path (`R-V186-QC1-S005`) → fast-follow.
- **Next iteration**: close `R-V186-QC1-S005` (manuscript body read string-prefix path-traversal — apply the same `resolve_guarded_path` delegation as V1.86 T3), then return to the feature cadence. Owner: PM at next iteration-start; trigger: this residual or the next product-completeness gap. A broader "defense-in-depth pass" (CSRF tokens / auth-mode maturity) remains explicitly deferred unless the threat model grows.
- **Long-term goal**: a local-first creative-writing tool whose localhost trust boundary is auditable and default-safe (STRATEGY Principle #1).

## 8. Delivery Branch Policy

> Mirror of frontmatter; kept in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.86` |
| `target_branch` | `main` |

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Origin allowlist too narrow → breaks Tauri desktop or web dev | Medium | High | Allowlist derived from the codebase-verified client origins (own port + Tauri webview + Vite dev); QA explicitly verifies desktop connect + web dev proxy + direct `http://127.0.0.1:<port>` navigation; env override for custom setups. |
| Origin allowlist too broad → leaves a gap | Low | High | Defense-in-depth: explicit Origin-reject middleware in addition to configured CORS; qc-specialist-2 security lens in tri-review. |
| User behind reverse proxy or custom Origin (e.g. `http://nexus.local:8420`, corporate proxy) now blocked | Medium | Medium | `NEXUS_DAEMON_ALLOWED_ORIGINS` env var is the documented escape hatch; error response is clear ("Origin '<x>' not allowed for this Local API"); documented in the trust-boundary section of `daemon-runtime.md`. |
| `R-V156P0-M002` prior resolution already sufficient → no-op effort | Medium | Low | All five P1 items are already `lifecycle: resolved`; implementor verifies each against current `main` HEAD first (per `.mstar/AGENTS.md` "pre-existing claim" protocol). Where a resolution holds → record confirmed, no code change; where insufficient (suspected for `R-V157P1-W001` and `R-V156P0-M004`) → re-harden + open a fresh V1.86 residual. |
| fs/* deny breaks a legitimate no-workspace flow | Low | Medium | grep all fs/* call sites; the only legitimate callers (CLI host-call, worker IPC, schedule) require a workspace context already; deny error is clear. |
| Blocking-sync-I/O fix (`spawn_blocking`) introduces a behavior change | Low | Low | P1; scoped to the fs read/write handlers; covered by the new regression tests. |

## 10. Compound Round Summary

- 结晶文档数：1 — `knowledge/architecture-patterns/resolved-residual-verification.md` (Knowledge track: `lifecycle: resolved` is a claim, not a guarantee — verify the class on current `main`; V1.86 found 2 of 5 same-class "resolved" residuals were insufficient)
- 新增 CONCEPTS.md 条目：0 (term is general enough; the protocol lives in the knowledge doc + `.mstar/AGENTS.md`)
- 触发 compound-refresh：否 (no stale docs contradicted)

## 11. Iteration Retrospective (minimal)

- 做得好的：PM 直接核码验证三份外部报告（零误报）；QC2 深度安全透镜对三条攻击路径逐条 adversarial probing 确认无 bypass；async 转换（T5）的 spawn_blocking 边界经 QC3 确认无 async-state 跨界。
- 可改进的：Phase 1 初稿把 5 个已 `resolved` 的同类残余误当 "open residuals to close" —— 被 writing-specialist 交叉验证捕获并纠正为 "verify-and-reharden"（已结晶为上述知识文档）；regression-of-resolution 模式值得在 residual 流程里前置检查。
- 下迭代建议：先关 `R-V186-QC1-S005`（manuscript body 同类 string-prefix），并考虑对全仓 `Path::new(..).starts_with(string)` 反模式做一次 grep sweep（path_guard.rs 注释已警告，但 qc1 发现第二处）。
