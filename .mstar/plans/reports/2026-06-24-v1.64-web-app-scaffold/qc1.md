---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-24-v1.64-web-app-scaffold"
verdict: "Approve"
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

---

## Revalidation (qc1, W-1 fix-wave)

**Targeted re-review** of W-1 only. No new full-tri-review required. The other two QC seats (qc2, qc3) returned Approve in Wave-1 and raised no blocking findings — only qc1 raised W-1 (cross-track: bug is in P1's `errors.ts`; consequence falls on P2's screens).

### Scope of this re-review

- **plan_id**: `2026-06-24-v1.64-web-app-scaffold`
- **Review range / Diff basis**: `0afa42b2..94a570f6` — the qc1 W-1 fix-wave (`fix/v1.64-error-envelope-parse` merged + status residual registration).
- **Working branch (verified)**: `iteration/v1.64` (`git branch --show-current` = `iteration/v1.64`)
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel`)
- **HEAD at re-review**: `94a570f6`
- **Tools run**: `git diff 0afa42b2..94a570f6 --stat`, `git diff 0afa42b2..94a570f6 -- apps/web/src/lib/nexus/errors.ts .mstar/knowledge/specs/local-api-surface-conventions.md`, `pnpm --filter web typecheck`, `pnpm --filter web build`, `cargo clippy -p nexus-daemon-runtime --no-deps -- -D warnings`, plus targeted reads of `apps/web/src/lib/nexus/errors.ts`, `apps/web/src/lib/nexus/browser-client.ts` (caller), and `crates/nexus-daemon-runtime/src/api/errors.rs` + `middleware.rs` (runtime envelope).

### What was re-checked (only W-1)

1. **Does `apps/web/src/lib/nexus/errors.ts` `fromBody` now correctly extract `code`/`message`/`details` from the wrapped `{success, error:{...}, request_id?}` body?** — **Yes (RESOLVED).**
   - Current implementation (lines 47–63): unwraps `body.error` first with defensive top-level fallback. Verified by manual trace against the runtime `ApiErrorResponse { success: false, error: ApiErrorDetail { code, message, details?, request_id? } }` (`crates/nexus-daemon-runtime/src/api/errors.rs:55-73,196-218`) and the middleware-injected `error.request_id` (`api/middleware.rs:111-147`).
   - `BrowserClient.request` at `apps/web/src/lib/nexus/browser-client.ts:185` calls `NexusClientError.fromBody(response.status, errorBody)` — the fix flows through the single parser site without changes to the caller, exactly as proposed in Wave-1's "Fix" snippet.

2. **Are the defensive fallbacks sound for ad-hoc `(StatusCode,String)` emitters (`R-V164-FE1-ORCH` deferral)?** — **Yes (graceful).**
   - Three paths verified:
     - **Wrapped envelope** (P0 handlers): `inner.code` wins → returns daemon-stable code.
     - **Ad-hoc bare-object** (`{ code: ..., message: ... }`): `inner.code` undefined, falls through to `parsed.code` → still extracts structured code.
     - **Ad-hoc plain string** (`R-V164-FE1-ORCH` orchestration handlers): `inner` and `parsed` both lack `code`/`message` → falls back to `http_<status>` / `"Request failed with status <status>"`. No crash, no `Cannot read properties of undefined`.
   - The proposal I made in Wave-1 (`inner.code ?? parsed.code ?? http_<status>`) is faithfully implemented at lines 59–61 with the exact same fallback chain.

3. **Does `local-api-surface-conventions.md` §3.1 correction now match runtime reality?** — **Partially (structural layer YES; casing example NO — separate drift).**
   - §3.1 now correctly documents the wrapped envelope, the inner-detail schema model boundary, the `request_id` placement under `error` (not top-level), and the implementation note that consumers MUST read from `body.error`. All structurally aligned with `ApiErrorResponse` + middleware behavior.
   - Caveat (F-2 below): the §3.1 example uses `code: "work_not_found"` (and §3.2's table is fully lowercase snake_case), but the runtime emits UPPER_SNAKE_CASE. The implementer correctly identified this as a separate drift and registered `R-V164-QC1-CASING` (low, defer V1.65+, owner `@architect`) in commit `94a570f6`. **qc1 concurs with this disposition.**

4. **Did the fix stay surgical (only the 2 claimed files)?** — **Yes.**
   - Implementation commit `41a11887` (merged via `3ac68224`) touched exactly 2 files:
     - `apps/web/src/lib/nexus/errors.ts` (28 lines diff)
     - `.mstar/knowledge/specs/local-api-surface-conventions.md` (34 lines diff)
   - `git diff 0afa42b2..41a11887 --name-only` confirms only those 2 implementation files. No edits to `browser-client.ts`, no edits to handlers, schemas, or generated code. Commit `94a570f6` registered the §3.2 casing residual in `.mstar/status.json` (expected; required by residual lifecycle).
   - Matches `mstar-coding-behavior` surgical-change discipline: every hunk traces to the W-1 fix or the registered residual note.

5. **CI / build verification:** — **PASS.**
   - `pnpm --filter web typecheck` → PASS (exit 0; no output).
   - `pnpm --filter web build` → PASS (1647 modules transformed; 232.67 kB JS / 15.65 kB CSS gzip-compressed; matches Wave-1 baseline `232.62 kB` within normal jitter).
   - `cargo clippy -p nexus-daemon-runtime --no-deps -- -D warnings` → PASS (`Finished dev profile`; no warnings).
   - No new CI failures; nothing the gate treats as `>= Warning` introduced.

### Per-finding disposition

| Wave-1 finding | Disposition | Evidence |
| --- | --- | --- |
| **W-1** (F-E1 wire envelope — P1 side, parser) | **RESOLVED** | `fromBody` unwraps `body.error` defensively; `BrowserClient` caller unchanged; CI green. |
| S-1 (apps/web ships zero test files — `BrowserClient`/`TauriClient`/`NexusClientError` parsing untested) | Open — unchanged from Wave-1 (registered as `R-V164-P1-QC1-NO-TESTS`); out of scope for W-1 fix-wave; P2 implementer should pick up. | — |
| S-2 (transport-agnostic interface) | Open — unchanged from Wave-1; informational. | — |
| S-3 (DESIGN.md token consumption clean) | Open — unchanged from Wave-1; informational. | — |
| S-4 (apps/web/AGENTS.md quality) | Open — unchanged from Wave-1; informational. | — |
| S-5 (UI primitive components) | Open — unchanged from Wave-1; informational. | — |
| S-6 (Vite dev-proxy + CI web-build) | Open — unchanged from Wave-1; informational. | — |
| S-7 (F-P3 deferral acknowledged in `types.ts`) | Open — unchanged from Wave-1; informational. | — |
| S-8 (package.json hygiene) | Open — unchanged from Wave-1; informational. | — |

### New finding from fix-wave

**F-2 (NEW, low, Suggestion tier; concur with `R-V164-QC1-CASING`):** `local-api-surface-conventions.md` §3.1 example uses `code: "work_not_found"` and §3.2 table examples are lowercase snake_case, while the runtime emits `UPPER_SNAKE_CASE` (verified at `crates/nexus-daemon-runtime/src/api/errors.rs:196-218,338-362,384,539` — `INVALID_INPUT`, `INTERNAL`, `AUTH_REQUIRED`, `NOT_FOUND`, `FORBIDDEN`, `INVALID_TRANSITION`). This is a latent doc-vs-runtime casing drift that was **pre-existing** in V1.63 (the convention was newly introduced; the runtime code strings were unchanged) and was correctly surfaced by the fix-wave implementer. Properly registered as `R-V164-QC1-CASING` (low, defer, `@architect`, V1.65+) in commit `94a570f6`. **qc1 concurs**: right disposition. Not a Wave-1 blocker; architect decision in V1.65+ (either align runtime to lowercase convention per §3.2, or update §3.2 to UPPER_SNAKE_CASE to match runtime). **Not registered against the P1 plan** — it is a documentation drift that the P0 implementer flagged, owned by `@architect`, scoped against `local-api-surface-conventions.md` (a Master-spec under `knowledge/specs/`, not a P1 surface).

### Updated Summary

| Severity | Wave-1 | This re-review |
|----------|--------|----------------|
| 🔴 Critical | 0 | 0 |
| 🟡 Warning | 1 (W-1) | **0 (W-1 RESOLVED)** |
| 🟢 Suggestion | 8 (S-1..S-8) | 9 (S-1..S-8 + new F-2) |
| Open residuals | S-1 (`R-V164-P1-QC1-NO-TESTS`, P2 ownership) + new `R-V164-QC1-CASING` (low, defer, `@architect`) + P0-shared `R-V164-FE1-ORCH` | — |

**Verdict**: **Approve** (updated from Wave-1's `Request Changes`).

Rationale: W-1 is structurally resolved. The fix-wave is surgical (2 implementation files only; expected status.json residual registration), the new `fromBody` parser correctly unwraps the daemon runtime's wrapped envelope with defensive top-level fallback for `R-V164-FE1-ORCH` ad-hoc emitters, the §3.1 convention doc correction matches runtime reality for the structural layer (envelope vs bare; `request_id` placement), and `pnpm --filter web typecheck/build` + `cargo clippy -p nexus-daemon-runtime` all pass. The §3.2 casing drift (`R-V164-QC1-CASING`) is a pre-existing latent doc-vs-runtime mismatch that the implementer correctly separated and registered at `low` with `@architect` / V1.65+ — not a regression, not blocking. No new Critical/Warning introduced.

PM should be ready to consolidate Wave-1 once qc2 and qc3 either re-affirm their Wave-1 Approve verdicts or have no fresh input (targeted re-review was qc1 only).