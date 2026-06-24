---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-24-v1.64-web-app-scaffold"
verdict: "Request Changes"
generated_at: "2026-06-25"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-25

## Scope
- plan_id: `2026-06-24-v1.64-web-app-scaffold`
- Review range / Diff basis: `c8f93e18..0afa42b2`
- Working branch (verified): `iteration/v1.64`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 28 (diff scope: `apps/web/{AGENTS.md,README.md,DESIGN.md,package.json,tsconfig.json,vite.config.ts,tailwind.config.ts,postcss.config.js,index.html,.gitignore}`; `apps/web/src/{main.tsx,App.tsx,index.css,vite-env.d.ts}`; `apps/web/src/lib/{client-context.tsx,utils.ts}`; `apps/web/src/lib/nexus/{index.ts,types.ts,browser-client.ts,errors.ts,tauri-client.ts}`; `apps/web/src/components/{theme-provider.tsx,daemon-health-indicator.tsx,screen-placeholder.tsx}`; `apps/web/src/components/layout/{header.tsx,root-layout.tsx,sidebar.tsx}`; `apps/web/src/components/ui/{badge.tsx,button.tsx,card.tsx}`; `apps/web/src/pages/screens.tsx`; `packages/nexus-contracts/src/generated/local-api/{common/ErrorResponse.ts,kb/PaginationInfo.ts,works/ListWorksQuery.ts,works/ListWorksResponse.ts,findings/ListFindingsQuery.ts,findings/ListFindingsResponse.ts}`; `pnpm-workspace.yaml`, `package.json`, `.github/workflows/ci.yml`)
- Commit range (if not identical to Review range line, explain): `c8f93e18..0afa42b2` (matches Review range)
- Tools run: `git diff c8f93e18..0afa42b2 --stat`, `pnpm --filter web typecheck`, `pnpm --filter web build`, plus targeted reads of `apps/web/src/lib/nexus/*`, `apps/web/src/components/{layout,ui}/*`, `apps/web/{tailwind.config.ts,src/index.css,vite.config.ts,tsconfig.json}`, `apps/web/{AGENTS.md,DESIGN.md,README.md}`

## Findings

### 🔴 Critical
- _(none)_

### 🟡 Warning

**W-1 (cross-track drift): `BrowserClient` parses the wrong layer of the F-E1 wire envelope; structured error codes are silently dropped at the UI boundary.**

This is the same finding as in `qc1.md` for `2026-06-24-v1.64-local-api-hardening` — recorded here too because the bug is in P1's client (`apps/web/src/lib/nexus/errors.ts`) and the consequence falls on P2's screens. PM should treat the two as one residual.

Evidence trail (P1-side):

1. `apps/web/src/lib/nexus/errors.ts:45-53` — `fromBody`:

   ```ts
   static fromBody(status: number, body: unknown): NexusClientError {
     const parsed = (body ?? {}) as Partial<NexusErrorBody>;
     return new NexusClientError(
       status,
       parsed.code ?? `http_${status}`,
       parsed.message ?? `Request failed with status ${status}`,
       parsed.details,
     );
   }
   ```

   Reads `code`, `message`, `details` from the **top level** of `body`. For a real daemon error response (P0's runtime envelope), the body is `{ success: false, error: { code: "INVALID_INPUT", message: "...", details: {...}, request_id: "req_..." }, request_id: "..." }` (verified by reading `crates/nexus-daemon-runtime/src/api/errors.rs:55-73` and `api/middleware.rs:111-147`). `parsed.code` is `undefined` → fallback `http_400`. `parsed.message` is `undefined` → fallback `Request failed with status 400`. `parsed.details` is `undefined` → structured details dropped.

2. The doc comment at `errors.ts:1-11` correctly says F-E1 has landed and the parsing can be tightened once the generated `ErrorResponse` type is available — and it is now available (the contracts package is 0.5.0 and `apps/web` typechecks against it via the `web-build` CI leg). The tightening was simply never done in the merge.

3. `BrowserClient.request` (`apps/web/src/lib/nexus/browser-client.ts:152-193`) calls `NexusClientError.fromBody(response.status, errorBody)` directly with the raw response body. The fix is local: unwrap `body.error` first when present.

4. `apps/web/AGENTS.md` "Pending contracts alignment" row for ErrorResponse says "PM re-verifies `pnpm --filter web typecheck` after the Wave-1 merge reconciles these" — the typecheck passes (verified; build green), but the **runtime behavior** (not just types) was not verified.

Effect: Every error toast/notification in the Web UI MVP — from Works CRUD, Findings list/create/update/delete, and any future handler routed through `NexusApiError` — will show generic "Request failed with status 400" instead of actionable "INVALID_INPUT: World binding is required for new Works" with details. The MVP screen list in `web-ui.md` §6.1 includes Sessions, Schedule, Capabilities, Presets which go through un-migrated orchestration handlers (`R-V164-FE1-ORCH` deferral) — those endpoints return **plain-string bodies** (`(StatusCode, String)` tuples), so neither the current parser nor a fixed parser can recover a code from them. The MVP READ experience is degraded for ~5 of the 7 MVP screen groups.

**Fix (proposed; small; should not block merge but must not be silently dropped):**

```ts
// apps/web/src/lib/nexus/errors.ts:45-53 — proposed
static fromBody(status: number, body: unknown): NexusClientError {
  // Daemon runtime wraps the canonical ErrorResponse under `error`:
  //   { success: false, error: { code, message, details?, request_id? }, request_id? }
  // Some endpoints still emit ad-hoc (StatusCode, String) (R-V164-FE1-ORCH deferral).
  const parsed = (body ?? {}) as Partial<NexusErrorBody>;
  const inner = (parsed as { error?: Partial<NexusErrorBody> }).error ?? parsed;
  return new NexusClientError(
    status,
    inner.code ?? parsed.code ?? `http_${status}`,
    inner.message ?? parsed.message ?? `Request failed with status ${status}`,
    inner.details ?? parsed.details,
  );
}
```

(~6 lines; defensive fallback preserves the ad-hoc path; `inner.code` is the daemon runtime's stable code.)

Severity mapping for `status.json.residual_findings`: `high` (correctness on MVP UX; not data loss; UI still works for 2xx).

---

### 🟢 Suggestion

**S-1: `apps/web` ships zero test files — `BrowserClient`/`TauriClient`/`NexusClientError` parsing is the exact surface where the F-E1 wire envelope bug should have been caught.**

`apps/web` has no `*.test.ts`, no `vitest`, no test script in `package.json`. The P1 deliverable accepted this as "no screens yet" but the transport adapter — the most architectural part of the scaffold — is precisely where the contract pays off. `apps/web/package.json:9-14` only defines `dev` / `build` / `preview` / `typecheck`. P2 (screens) should add at least:

- `NexusClientError.fromBody` round-trip tests against the actual daemon envelope (`{ success: false, error: { code, message, details, request_id }, request_id }`), the ad-hoc envelope (`"plain string"`), the success envelope, the transport-failure path, and the 204-no-content path.
- `BrowserClient.query serialization` (empty values, undefined, null, booleans, integers — already in `browser-client.ts:55-64`).
- `TauriClient` stub behavior (each method should throw `not_implemented_in_browser_build` with the documented message).

Adding these tests in P2 would have caught W-1 before it reached QC; adding them now is the cheapest mitigation.

**S-2: `NexusClient` interface is genuinely transport-agnostic; the boundary is clean and frozen.**

- `apps/web/src/lib/nexus/types.ts:68-108` — the interface uses only TypeScript types from `@42ch/nexus-contracts`. No `fetch`, no `Response`, no `URL`, no `RequestInit` leak into the interface.
- `apps/web/src/lib/nexus/index.ts:10-13` — three exports: `BrowserClient`, `NexusClientError`, `TauriClient` + types. Clean barrel.
- `client-context.tsx:19` — `useMemo(() => client ?? new BrowserClient(), [client])`; V1.65 swap is a one-line factory change.
- The V1.63 contract base means `listFindings` is correctly omitted (typed against the F-P2 type that didn't exist at base). Pending methods (`listWorks` taking cursor `ListWorksResponse`, `getWork`/`patchWork` typed against `WorkDetailResponse`) all import the generated types — no handwritten wire shapes.

Architecture: clean. The interface passes the web-ui.md §5 "screens must not call `fetch`/`invoke` directly" invariant at the type level.

**S-3: DESIGN.md token consumption is clean, light + dark both populated identically, focus-ring + reduced-motion handled.**

- `apps/web/tailwind.config.ts:40-167` — every DESIGN.md color scale is mapped to a CSS var; type scale, motion tokens, shape radii, breakpoint tokens all mapped. No invented tokens.
- `apps/web/src/index.css:19-201` — `:root` (light) and `.dark` carry the same token names with different values (matches DESIGN.md §Colors tables verbatim). Two-layer `:focus-visible` ring at lines 184-188 matches DESIGN.md §Component Primitives. `prefers-reduced-motion` override at lines 191-200 matches DESIGN.md §Motion.
- `theme-provider.tsx:29-48` — `useEffect` swaps the `.dark` class on `<html>` + sets `data-theme` + persists to `localStorage`; initial theme respects `prefers-color-scheme`. Clean.
- `darkMode: 'class'` in `tailwind.config.ts:28` matches the provider's `.dark` class. Correct wiring.

Caveat (not a finding, just a note for V1.65): DESIGN.md is the SSOT for both light and dark, but the file is one monolithic document. The convention `apps/web/DESIGN.md` line 7 hints at splitting out `DESIGN.dark.md` in V1.65+ — fine; this is a deliberate deferral.

**S-4: `apps/web/AGENTS.md` is well-structured for the new-package policy and the cross-track reconciliation work.**

- Identity & placement section (lines 3-9) clearly establishes `apps/web` as OSS-local, not cloud-SaaS; cites web-ui.md §2.2 invariant.
- SSOT & authority section (lines 11-21) names DESIGN.md, web-ui.md, and the `NexusClient` interface as the three design authorities.
- **Pending contracts alignment** table (lines 27-43) is excellent — explicitly lists `listFindings`, Works cursor, preset full CRUD, `getWork`/`patchWork` drift, ErrorResponse parsing, with the P0 plan as the unblocker. This is exactly what the cross-track reconciliation needs.
- Build/typecheck contract section (lines 45-50) is clear: contracts package must build first; CI does this in the `web-build` leg.
- Conventions section (lines 52-65) covers TypeScript strict, Tailwind utilities, a11y (WCAG 2.1 AA), voice & content, and the daemon port — all aligned with DESIGN.md and web-ui.md.

This is a high-quality AGENTS.md for a brand-new package; the "new-package AGENTS.md rule" in root `AGENTS.md` is fully satisfied.

**S-5: `apps/web/src/components/ui/{badge,button,card}.tsx` and `components/layout/{header,root-layout,sidebar}.tsx` are minimal primitives that match DESIGN.md §Component Primitives without over-abstracting.**

- The three `ui/*` components (~55 lines each) are shadcn-style copy-in primitives with `cn()` composition (`apps/web/src/lib/utils.ts:1-13`).
- `sidebar.tsx:28-35` — `NAV_ITEMS` matches web-ui.md §6 MVP surface (6 screen groups; presets is the 7th — note: presets appears as a Setup screen in §6.2 but is also in the sidebar at line 34 — verify the side nav covers all 7 MVP groups; currently the sidebar has 6 items, missing Presets from §6.2 — but Presets IS at `sidebar.tsx:34`, so all 7 are present. Good.)
- `root-layout.tsx` and `header.tsx` (76 + 30 lines) use DESIGN.md tokens throughout.
- `screen-placeholder.tsx` (44 lines) — placeholder for the 7 screens; consistent Title Case + heading-32 per DESIGN.md §Voice & Content.
- `daemon-health-indicator.tsx` (64 lines) — reads `client.health()` via TanStack Query; appropriate for the header.

**S-6: Vite dev-proxy + CI `web-build` leg are wired correctly; same-origin semantics verified.**

- `vite.config.ts:36-47` — proxies `/v1/local` to `http://127.0.0.1:8420` (the daemon default). `changeOrigin: false` keeps the same-origin semantics (cookie/auth not needed; keyless loopback per V1.20). `VITE_DAEMON_URL` override supported.
- `BrowserClient` defaults `baseUrl: ''` (`browser-client.ts:37-46`) — same-origin relative paths; matches the proxy in dev and the embedded-SPA in release.
- `.github/workflows/ci.yml` new `web-build` job (diff lines 168-194): `needs: verify-codegen` (correct ordering; consumes the `generated-types` artifact), `pnpm install --frozen-lockfile`, `pnpm --filter @42ch/nexus-contracts run build`, `pnpm --filter web typecheck`, `pnpm --filter web build`. Clean ordering; matches the apps/web/AGENTS.md "Build/typecheck contract" section.
- pnpm workspace `apps/*` registration at `pnpm-workspace.yaml` (1-line diff) is correct; root `package.json` workspace declaration matches.

**S-7: F-P3 deferral (works/sessions/etc. → items rename) is acknowledged in `types.ts:14-22` — confirmed.**

- The `NexusClient` interface has no handwritten wire shapes for `ListWorksResponse` (imports the generated type which still uses `works`).
- `apps/web/AGENTS.md` Pending contracts alignment table does not call out F-P3 directly, but the Pending contracts row for "Works cursor list" implicitly acknowledges the legacy `{ works, total }` shape and that the new shape arrives with P0 (now landed).
- P2 (screens) will need a TanStack Query transformer that maps `response.works` → a normalized `works` array (or to a generic `items` array). No current scaffold code does this — P2 implementer should own. Compass §5 item #2 says "UI maps them via a thin TanStack Query transformer"; documented ownership boundary is fine.

**S-8: `package.json` `private: true, version: "0.0.0"` is correct for a workspace app.**

- `apps/web/package.json:2-4` — `private: true` is correct (the app is a workspace consumer of `@42ch/nexus-contracts`, not a publishable package).
- Engines constraint `node: ">=20.0.0", pnpm: ">=8.0.0"` is consistent with root `package.json` and CI's `setup-node@v4` + `node-version: 22` + `pnpm@9`.
- No `version` bump is required (workspaces don't use the per-app version for `@42ch/nexus-contracts` resolution — that comes from `workspace:*`).

---

## Source Trace
- Finding ID: W-1 (F-E1 wire envelope mismatch — P1 side)
- Source Type: manual-reasoning + git-diff
- Source Reference:
  - `apps/web/src/lib/nexus/errors.ts:45-53` (parser)
  - `apps/web/src/lib/nexus/browser-client.ts:152-193` (caller)
  - `apps/web/src/lib/nexus/types.ts:1-111` (interface contract — no leak)
  - `apps/web/AGENTS.md:35-40` (Pending contracts alignment — ErrorResponse row)
  - `crates/nexus-daemon-runtime/src/api/errors.rs:55-73,248-277` (runtime envelope definition)
  - `crates/nexus-daemon-runtime/src/api/middleware.rs:111-147` (request_id injection)
  - `schemas/local-api/common/README.md:13` (envelope documentation)
  - `.mstar/knowledge/specs/local-api-surface-conventions.md` §3.1 (misleading bare-shape example)
- Confidence: High

- Finding ID: S-1 (missing app-side tests)
- Source Type: manual-reasoning + git-diff
- Source Reference: `apps/web/package.json:9-14` (no test script), no `*.test.ts` files in `apps/web/src/**`
- Confidence: High

- Finding ID: S-2 (transport-agnostic interface)
- Source Type: manual-reasoning
- Source Reference: `apps/web/src/lib/nexus/{types.ts,index.ts,browser-client.ts,tauri-client.ts}`, `apps/web/src/lib/client-context.tsx`
- Confidence: High

- Finding ID: S-3 (DESIGN.md token consumption)
- Source Type: manual-reasoning
- Source Reference: `apps/web/tailwind.config.ts`, `apps/web/src/index.css`, `apps/web/src/components/theme-provider.tsx`, `apps/web/DESIGN.md`
- Confidence: High

- Finding ID: S-4 (apps/web/AGENTS.md adequacy)
- Source Type: manual-reasoning + doc-rule
- Source Reference: `apps/web/AGENTS.md:1-65`
- Confidence: High

- Finding ID: S-5 (UI primitive components)
- Source Type: manual-reasoning
- Source Reference: `apps/web/src/components/{ui/*,layout/*,daemon-health-indicator.tsx,screen-placeholder.tsx}`
- Confidence: High

- Finding ID: S-6 (Vite proxy + CI)
- Source Type: git-diff + linter (pnpm build/typecheck PASS)
- Source Reference: `apps/web/vite.config.ts:36-47`, `.github/workflows/ci.yml:168-194`, `pnpm-workspace.yaml`
- Confidence: High

- Finding ID: S-7 (F-P3 deferral acknowledgment)
- Source Type: manual-reasoning + doc-rule
- Source Reference: `apps/web/src/lib/nexus/types.ts:14-22`, web-ui.md §10 + compass §5 item #2
- Confidence: High

- Finding ID: S-8 (package.json hygiene)
- Source Type: git-diff + manual-reasoning
- Source Reference: `apps/web/package.json:1-42`, root `package.json`, root `AGENTS.md`
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 8 |

**Verdict**: Request Changes

Rationale: One unresolved `Warning` (W-1: `BrowserClient.fromBody` reads the wrong layer of the F-E1 wire envelope). The cross-track nature of this issue is shared with the P0 qc1 report — same root cause, two surfaces. The fix is small and the underlying runtime design is intentional, so this is not a redesign or merge blocker. PM should ensure W-1 is resolved in this Wave (or registered explicitly with `severity: high` and a clear V1.65+ target) — silently shipping the bug means the Web UI MVP's unified-error-handling promise is not delivered for ~5 of the 7 MVP screen groups.

CI status: `pnpm --filter web typecheck` PASS, `pnpm --filter web build` PASS (1647 modules transformed, 232.62 kB JS / 15.65 kB CSS gzip-compressed). No CI failures; the Warning is an architectural coherence gap rather than a build/lint/test failure.

Per-finding machine severity (for PM residual registration):
- W-1 → `high` (substantive MVP UX; non-blocking for compile/CI but undecided)
- S-1 → `medium` (missing test coverage at the most architectural surface)
- S-2 → `nit` (confirmation only; no action)
- S-3 → `nit` (confirmation only; no action)
- S-4 → `nit` (confirmation only; no action)
- S-5 → `nit` (confirmation only; no action)
- S-6 → `nit` (confirmation only; no action)
- S-7 → `low` (P2 ownership boundary for F-P3 adapter needs to be assigned)
- S-8 → `nit` (confirmation only; no action)