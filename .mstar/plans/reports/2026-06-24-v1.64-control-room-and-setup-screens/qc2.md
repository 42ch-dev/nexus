---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-24-v1.64-control-room-and-setup-screens"
verdict: "Approve"
generated_at: "2026-06-25"
---

# Code Review Report — qc2 (Security & Correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (primary: P2 BrowserClient + form paths; cross-check P3 shell surface)
- Report Timestamp: 2026-06-25

## Scope
- plan_id: 2026-06-24-v1.64-control-room-and-setup-screens
- Review range / Diff basis: 56bf917a..4dd8cbb1 — V1.64 Wave 2 (P2 + P3 + status)
- Working branch (verified): iteration/v1.64
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (`git rev-parse --short HEAD=4dd8cbb1`)
- Files reviewed: P2 frontend surface (apps/web/src/pages/**, queries.ts, lib/nexus/**) + cross-cut with P3 daemon wiring
- Commit range: 56bf917a..4dd8cbb1
- Tools run: `git diff 56bf917a..4dd8cbb1 --stat`, targeted `git diff` on static_assets.rs + daemon/mod.rs + api/mod.rs + web sources; `grep` for injection sinks; `cargo test -p nexus-daemon-runtime --test works_api` (34 passed)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None (mandatory pre-Approve items resolved or not present).

**Notes (non-blocking context for correctness):**
- P2 scope gaps (CreateWorkRequest lacks `work_profile`; preset surface limited to list/scaffold/validate/reload; CapabilityInfo lacks admission gates) are explicitly documented in the plan stub and surfaced to the user rather than silently faked. No silent wrong behavior introduced.
- msw mocks cover UI contract shape; they do not replace the daemon-side `works_api` auth tests (see P3 verification).

### 🟢 Suggestion
- Consider adding a small runtime guard or comment in `BrowserClient` constructor noting that `baseUrl` override is only for diagnostics and must remain loopback in production usage (defense-in-depth documentation).
- The three documented scope gaps are tracked as residuals — ensure they are carried in `status.json` or the plan residual list for V1.65+ follow-up.

## Source Trace
- Finding ID: (no blocking findings)
- Source Type: manual code review + diff inspection + test run
- Source Reference: `git diff 56bf917a..4dd8cbb1 -- 'apps/web/src/**'`, `grep -r dangerouslySetInnerHTML apps/web/src` (clean), BrowserClient.ts (no credential headers, keyless loopback model), works_api.rs test run
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 (non-blocking) |

**Verdict**: Approve

## Evidence Citations (security / correctness focus)
- **BrowserClient trust model**: `apps/web/src/lib/nexus/browser-client.ts` — only `fetch` to `/v1/local/*` (same-origin or explicit loopback), no `Authorization` header, errors mapped through `NexusClientError.fromBody` (consumes shared `ErrorResponse`). Matches V1.20 keyless-loopback model and `web-ui.md` §4.2.
- **No XSS surface**: Full-tree grep for `dangerouslySetInnerHTML`, `innerHTML`, `__html`, raw HTML injection sinks returned zero matches in `apps/web/src`. All rendering goes through typed React components consuming contract summaries.
- **Form correctness**: Create/Patch Work and preset dialogs use typed request shapes from `@42ch/nexus-contracts`. Client-side validation exists; server validation (P0) is the authority. Mutations flow through the client; no direct `fetch` bypass.
- **msw fidelity**: Test setup (`src/test/msw-server.ts`) provides contract-shaped responses for UI layer tests. Does not mask daemon authz (daemon tests remain the source of truth).
- **Keyless loopback integrity**: No path in P2 opens a non-loopback listener or injects credentials. All data access remains behind the existing `require_api_key` middleware on loopback.

**Cross-cut with P3 (for context)**: See sibling `qc2.md` for `2026-06-24-v1.64-daemon-serving-wiring` (SPA fallback router ordering, GET/HEAD-only, rust-embed path handling, `open_ui` command construction, test workaround verification).
