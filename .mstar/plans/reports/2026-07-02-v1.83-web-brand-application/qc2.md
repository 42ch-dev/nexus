---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-02-v1.83-web-brand-application"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: composer-2.5
- Review Perspective: Security and correctness (package export boundary, SVG safety, theme/localStorage handling, build integrity, accessibility labels, token-application correctness)
- Report Timestamp: 2026-07-02

## Scope
- **plan_id**: `2026-07-02-v1.83-web-brand-application` (P2)
- **Review range / Diff basis**: `e86652d4..34d0af45` on branch `feature/v1.83-web-brand-application` (P2 commit `34d0af45`)
- **Working branch (verified)**: `feature/v1.83-web-brand-application`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 16 changed paths under `apps/web/` (+178 / −34 lines) plus lockfile
- **Tools run**: `git diff` / `git log`, `pnpm --filter web run typecheck`, `pnpm --filter web run test` (387/387), `pnpm --filter web run build`, Node `exports` resolution probe, `rg` (SVG XSS patterns, secrets, internal package paths), production bundle spot-check for SVG embedding, DESIGN.md / `apps/web/DESIGN*.md` cross-check
- **Deep review**: triggered (new brand asset consumption path; theme-aware shell wiring; CSS variable retint affecting canvas/SOUL surfaces)

## P2 Security / Correctness Checklist

| Area | Result | Evidence |
|------|--------|----------|
| Public `@42ch/nexus-ui` export boundary | **Pass** | `nexus-logo.tsx` imports only declared `./assets/logos/logo-*.svg` and `index.css` imports `./theme.css`; `logo_dark.png` resolves `ERR_PACKAGE_PATH_NOT_EXPORTED`; no `packages/nexus-ui` relative paths in `apps/web` |
| SVG XSS surface | **Pass** | Logos rendered via `<img src={bundledSvg}>` (not inline SVG / `dangerouslySetInnerHTML`); `rg` finds no `<script>`, event handlers, `foreignObject`, or `javascript:` in brand component tree or logo assets |
| Secrets / credentials | **Pass** | No API keys, tokens, or private-key patterns in P2 diff |
| Theme `localStorage` integrity | **Pass** | `ThemeProvider` reads only `'light' \| 'dark'` from `nexus-web-theme`; other values fall through to OS preference; writes are controlled by `setTheme` / `toggleTheme` only |
| Build / typecheck determinism | **Pass** | `prebuild` / `pretypecheck` chain `@42ch/nexus-contracts` + `@42ch/nexus-ui` builds; `typecheck`, full test suite, and production `build` succeed locally |
| Logo variant correctness | **Pass** | `NexusLogo` maps `light → logo-dark.svg`, `dark → logo-color.svg` per root `DESIGN.md` § Logo Usage; tests assert encoded brand hex in bundled data URLs |
| Shell accessibility | **Pass** | Logo exposes `role="img"` + configurable `alt`; responsive placement avoids duplicate visible marks (sidebar `lg+`, header `<lg`); theme toggle retains `aria-label` |
| Auth / data / network boundary | **Pass** | No new fetch paths, auth flows, or wire-contract changes; static workspace assets only |
| Downstream regression | **Pass** | 387/387 web tests pass; no route or business-logic edits |

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W1 — One dark canvas fill token still uses legacy blue rgba**  
  `--color-canvas-worldkb-entity-card-fill-selected` in `.dark` remains `rgba(82, 168, 255, 0.14)` while adjacent canvas/SOUL tokens were retinted to brand-cyan rgba. Matches current `DESIGN.dark.md` frontmatter but is inconsistent with the P2 re-tint pass documented in `apps/web/DESIGN.md` §Implementation Mapping.  
  **Impact**: Visual/correctness inconsistency on World KB selected cards in dark mode only; no security or data-integrity risk.  
  **Disposition**: Acceptable for P2 shell/primitives scope; schedule DESIGN.dark + CSS follow-up when canvas polish is in scope (also noted as qc1 S3).

- **W2 — Logo unit tests couple to Vite data-URL encoding**  
  `nexus-logo.test.tsx` asserts `src` contains URL-encoded hex (`%231E3A5F`, `%2325D1E0`). Correct today (Vite inlines SVGs as `data:image/svg+xml,...`), but a bundler strategy change (file asset URL) would fail tests without a production defect.  
  **Disposition**: Test maintainability risk only; not a runtime security issue. Prefer mocking asset imports or asserting variant selection via mocked module map in a follow-up.

### 🟢 Suggestion
- **S1 — Document trusted `label` contract on `NexusLogo`**  
  `label` flows to `alt` without extra sanitization (unnecessary in React). If future callers pass dynamic/user-derived text, constrain to trusted product copy. Today shell uses the default `"Nexus"` only.

- **S2 — Node 24+ Vitest `localStorage` experimental warnings**  
  `src/test/setup.ts` polyfill restores functional tests, but worker startup still logs `ExperimentalWarning: localStorage is not available…` on Node 24 locally. CI uses Node 22 (`setup-monorepo` default), so CI is unaffected; document local Node pin or Vitest env if log noise becomes distracting (qc1 S4).

- **S3 — Refresh `apps/web/AGENTS.md` prebuild note**  
  AGENTS still describes only `@42ch/nexus-contracts` in the prebuild chain; implementation now also builds `@42ch/nexus-ui`. Stale docs could cause a developer running bare `tsc` to misdiagnose missing types — operational correctness, not production runtime risk.

## Source Trace
- **F-001** (export boundary): `packages/nexus-ui/package.json:36-40`; Node resolve probe; `nexus-logo.tsx:1-2`
- **F-002** (SVG safety): `rg -i` across `apps/web/src/components/brand/` and `packages/nexus-ui/assets/logos/*.svg` → no script/event patterns; `nexus-logo.tsx:22-28` uses `<img>`
- **F-003** (theme storage): `theme-provider.tsx:20-27,36`; `test/setup.ts` polyfill for Vitest only
- **F-004** (logo mapping): root `DESIGN.md:236-244`; `nexus-logo.tsx:17-19`; `nexus-logo.test.tsx:22-41`
- **F-005** (build chain): `apps/web/package.json:11-14`; successful `typecheck` / `test` / `build` runs
- **F-006** (residual rgba): `apps/web/src/index.css:262`
- **F-007** (bundle embedding): production `dist/assets/index-*.js` contains `data:image/svg+xml` logo payloads (scripts inert under `<img>`)

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (visual/test maintainability; acceptable for P2) |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

P2 introduces brand consumption through the documented public package surface, uses a safe static-asset rendering pattern, preserves theme/localStorage invariants, and passes full web validation. Open warnings are visual parity and test-brittleness follow-ups, not security or correctness blockers for merge.
