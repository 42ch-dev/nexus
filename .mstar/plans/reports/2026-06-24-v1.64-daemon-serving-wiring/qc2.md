---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-24-v1.64-daemon-serving-wiring"
verdict: "Approve"
generated_at: "2026-06-25"
---

# Code Review Report — qc2 (Security & Correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (primary: P3 SPA fallback + daemon UI launch surface)
- Report Timestamp: 2026-06-25

## Scope
- plan_id: 2026-06-24-v1.64-daemon-serving-wiring
- Review range / Diff basis: 56bf917a..4dd8cbb1 — V1.64 Wave 2 (P2 + P3 + status)
- Working branch (verified): iteration/v1.64
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (`git rev-parse --short HEAD=4dd8cbb1`)
- Files reviewed: `crates/nexus-daemon-runtime/src/static_assets.rs`, `crates/nexus-daemon-runtime/src/api/mod.rs`, `crates/nexus42/src/commands/daemon/mod.rs` (+ cross-cut P2 web sources)
- Commit range: 56bf917a..4dd8cbb1
- Tools run: `git diff 56bf917a..4dd8cbb1 -- 'crates/nexus-daemon-runtime/src/static_assets.rs' 'crates/nexus42/src/commands/daemon/mod.rs' 'crates/nexus-daemon-runtime/src/api/mod.rs'`, router mount inspection, `cargo test -p nexus-daemon-runtime --test works_api`

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None (mandatory pre-Approve items resolved or not present).

### 🟢 Suggestion
- Add a short unit test for `serve_embedded_app` covering: non-GET/HEAD → 405, `/v1/local/*` prefix never reaches fallback (router ordering), missing asset → index.html fallback (only for GET).
- Consider documenting in `daemon-runtime.md` §4.4 that the SPA fallback explicitly excludes any path starting with `/v1/local/` (defense-in-depth note for future router changes).
- The axum-test workaround in `works_api.rs` is acceptable for the test surface; ensure the real `create_router` path (with fallback) is exercised in a separate integration test before release if possible.

## Source Trace
- Finding ID: (no blocking findings)
- Source Type: manual diff review + code inspection + test execution
- Source Reference: `static_assets.rs` (full), `api/mod.rs` (router composition lines ~392-405), `daemon/mod.rs` (open_ui ~784-826), `cargo test -p nexus-daemon-runtime --test works_api` (34 passed)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 (non-blocking) |

**Verdict**: Approve

## Evidence Citations (security / correctness focus — P3 primary surface)

### SPA fallback security & correctness (static_assets.rs + api/mod.rs)
- **Router mount order (correct priority)**: In `api/mod.rs`:
  ```rust
  Router::new()
      .fallback(static_assets::serve_embedded_app)   // added FIRST
      .merge(runtime_routes)
      .merge(protected_routes)                       // explicit /v1/local/* win
  ```
  Explicit routes take precedence over fallback. `/v1/local/*` data paths never reach the SPA handler.

- **Method restriction**: `serve_embedded_app` returns `405 Method Not Allowed` for anything other than `GET`/`HEAD`. POST/PUT/DELETE to client routes are rejected at the shell layer.

- **Path handling & SPA fallback**:
  - Strips leading `/`, defaults empty → `index.html`.
  - If exact asset missing → serves `index.html` (for GET/HEAD) so React Router can handle client routes (`/works`, `/presets`, etc.).
  - Explicit comment: "The function is designed as a **fallback** inside the axum router — it only receives requests that were NOT matched by any `/v1/local/*` or other explicit route."

- **No data leakage**: The embedded shell (`index.html` + hashed assets) carries **no data**. All data access is via the protected `/v1/local/*` routes (keyless on loopback per V1.20 model). Matches `daemon-runtime.md` §4.4 and `web-ui.md` §4.1/4.2.

- **Cache headers (correct)**:
  - `/assets/*` → `public, max-age=31536000, immutable`
  - `index.html` and other entry points → `no-cache`
  Hard-coded via `parse().expect(...)` on compile-time literals (documented as never-panicking in practice).

- **Path traversal / embed safety**: `rust-embed` macro at build time embeds only the contents of `../../apps/web/dist` relative to the crate. Runtime lookup is by exact embedded key; no filesystem walk or user-controlled path concatenation occurs. No opportunity for `../` escape at serve time.

- **Release binary embedding**: Only `apps/web/dist` (Vite build output) is embedded. No secrets, PII, source maps (unless intentionally built in), or build artifacts containing credentials. Dist is gitignored; release CI runs `pnpm --filter web build` before `cargo build --release`.

### `daemon ui` / `web` browser-open command construction (daemon/mod.rs)
- URL is constructed as `format!("http://127.0.0.1:{port}/")` — port is `u16` CLI arg (default `DAEMON_PORT`); no user-supplied host or path fragment.
- Platform commands:
  - macOS: `open <url>`
  - Linux: `xdg-open <url>`
  - Windows: `cmd /c start <url>`
- No shell interpolation, no `sh -c`, no `exec` of unsanitized input. The URL is passed as a single argument to the platform launcher. Command-injection risk is negligible (port is numeric; host is hard-coded loopback).

### Auth surface & test workaround verification
- P3 adds no new auth paths. The SPA shell is intentionally unauthenticated; data endpoints remain behind the existing `require_api_key` middleware (except documented unguarded runtime health/status).
- `cargo test -p nexus-daemon-runtime --test works_api` (34 tests) passed, including multiple `*_returns_401_without_creator` and creator-isolation tests. The axum-test workaround (direct handler calls) does not mask auth on data paths — the tests continue to assert 401/404 isolation behavior.

### Cross-cut with P2 (for context)
- P2 BrowserClient and screen surfaces were reviewed in the sibling report (`2026-06-24-v1.64-control-room-and-setup-screens/qc2.md`). No new injection sinks, no `dangerouslySetInnerHTML`, no credential leakage, and no non-loopback surfaces introduced.

## Revalidation notes (none — initial wave)
This is the initial QC for this plan. No prior `qc2.md` existed for revalidation.

**Verdict**: Approve
