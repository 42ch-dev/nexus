---
report_kind: qa
reviewer: qa-engineer
plan_id: "2026-07-02-v1.83-web-brand-application"
verdict: "Pass"
generated_at: "2026-07-02"
---

# QA Report — P2 V1.83 Web Brand Application

## Verdict

**Pass**

## Reviewer Metadata

- **Agent**: qa-engineer
- **Plan**: `2026-07-02-v1.83-web-brand-application`
- **Assignment Working branch**: `feature/v1.83-web-brand-application`
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Checkout at verification**: `feature/v1.83-web-brand-application` (tip `a6c7ea00`; merged to `iteration/v1.83`)
- **Implementation commit**: `34d0af45` — `feat(web): apply V1.83 brand tokens and logo to shell UI`
- **QC**: triple Approve (`qc1.md`, `qc2.md`, `qc3.md`)

## Scope Tested

P2 acceptance for applying the V1.83 brand foundation to `apps/web`:

1. Public `@42ch/nexus-ui` consumption (theme CSS + logo SVG exports)
2. Nexus mark in shell (header/sidebar) with theme-aware variants
3. Token mapping from P1 DESIGN contract into Tailwind/CSS variables
4. Base primitive retints (button, card, input, select, textarea, states)
5. Automated validation: typecheck, full test suite, production build

Out of scope (per plan): page IA redesign, canvas layout rebuild, backend/schema changes, component-library extraction.

## Acceptance Criteria Matrix

| Criterion | Result | Evidence |
|-----------|--------|----------|
| Web shell uses Nexus mark (light + dark) | **Pass** | `NexusLogo` in `sidebar.tsx` (desktop `lg+`) and `header.tsx` (mobile `<lg`); theme maps `light → logo-dark.svg`, `dark → logo-color.svg` per root `DESIGN.md` § Logo Usage |
| Base primitives use DESIGN tokens (no ad hoc hex in components) | **Pass** | UI primitives use Tailwind keys (`blue-700`, `brand-cyan`, `gray-*`); `rg '#[0-9a-fA-F]{3,8}' apps/web/src/components/ui` → no matches |
| Public `@42ch/nexus-ui` imports only | **Pass** | `nexus-logo.tsx` imports `@42ch/nexus-ui/assets/logos/logo-*.svg`; `index.css` imports `@42ch/nexus-ui/theme.css`; no `packages/nexus-ui` relative paths in `apps/web` |
| Token mapping documented + wired | **Pass** | `apps/web/DESIGN.md` § Brand → Web alias map + § Implementation Mapping (P2); `index.css` aliases `--color-brand-*` → `--nexus-brand-*`; `tailwind.config.ts` references brand keys |
| Prebuild/pretypecheck chains nexus-ui | **Pass** | `apps/web/package.json` `prebuild` / `pretypecheck` build `@42ch/nexus-contracts` + `@42ch/nexus-ui` |
| Logo accessibility | **Pass** | `NexusLogo` uses `role="img"` via `<img alt>`; tests cover default and custom labels (`nexus-logo.test.tsx` 3/3) |
| Existing workflows pass tests | **Pass** | `pnpm --filter web run test` — 387/387 (51 files) |
| Typecheck passes | **Pass** | `pnpm --filter web run typecheck` exit 0 |
| Production build passes | **Pass** | `pnpm --filter web run build` exit 0; main chunk 186.71 kB (gzip 50.89 kB) |
| No backend/schema/contract changes | **Pass** | Diff limited to `apps/web` (+ lockfile); no `schemas/` or Rust crate edits in P2 commit |
| Plan tasks T1–T5 complete | **Pass** | `.mstar/plans/2026-07-02-v1.83-web-brand-application.md` L61–65 all `[x]` |

## Validation Commands

```bash
# Branch alignment
git checkout feature/v1.83-web-brand-application
git merge-base --is-ancestor feature/v1.83-web-brand-application iteration/v1.83

# Public export boundary
rg '@42ch/nexus-ui|packages/nexus-ui' apps/web
node -e "/* resolve theme.css + logo SVG export targets */"

# Shell + brand component
rg 'NexusLogo' apps/web/src/components/layout

# Automated gates
pnpm --filter web run typecheck
pnpm --filter web run test
pnpm --filter web run build
```

### Command Results Summary

| Command | Exit | Output (abbrev.) |
|---------|------|------------------|
| `git branch --show-current` | 0 | `feature/v1.83-web-brand-application` |
| Feature branch ancestor of `iteration/v1.83` | 0 | Merged (same tip `a6c7ea00`) |
| Export resolution (`theme.css`, `logo-color.svg`, `logo-dark.svg`) | 0 | All 3 paths OK on disk |
| `rg 'packages/nexus-ui' apps/web` | 1 | No internal package paths |
| `pnpm --filter web run typecheck` | 0 | Chains contracts + nexus-ui build; `tsc --noEmit` clean |
| `pnpm --filter web run test` | 0 | 387/387 pass |
| `pnpm --filter web run build` | 0 | Vite production build success |

## `@42ch/nexus-ui` Consumption

| Import site | Public export | Purpose |
|-------------|---------------|---------|
| `apps/web/src/index.css` | `@42ch/nexus-ui/theme.css` | `--nexus-brand-deep-blue`, `--nexus-brand-cyan`, `--nexus-brand-white` |
| `apps/web/src/components/brand/nexus-logo.tsx` | `@42ch/nexus-ui/assets/logos/logo-dark.svg` | Light-theme shell mark |
| `apps/web/src/components/brand/nexus-logo.tsx` | `@42ch/nexus-ui/assets/logos/logo-color.svg` | Dark-theme shell mark |
| `apps/web/package.json` | workspace `@42ch/nexus-ui` | Dependency + pre-hook build chain |

## Token Mapping (P1 → P2)

| Layer | Artifact | Status |
|-------|----------|--------|
| Root SSOT | `DESIGN.md` / `DESIGN.dark.md` | Consumed via P1 (not edited in P2) |
| Package mirror | `@42ch/nexus-ui/theme.css` | Imported in `index.css` |
| Web alias map | `apps/web/DESIGN.md` § Brand → Web alias map | Documents `brand-*` ↔ `blue-*` preservation |
| CSS projection | `apps/web/src/index.css` | `--color-brand-*` aliases `--nexus-brand-*`; `blue-700`…`1000` retinted to brand-deep-blue scale |
| Tailwind | `apps/web/tailwind.config.ts` | `brand-deep-blue`, `brand-cyan`, `brand-white` keys added |

## Shell & Primitive Coverage

| Surface | Change |
|---------|--------|
| Sidebar | `NexusLogo` at top of primary nav (`lg+` visible mark) |
| Header | `NexusLogo` for `<lg` breakpoint; theme toggle retained |
| Button | Brand primary (`blue-700` light; `brand-cyan` on deep-blue text dark) |
| Card, Input, Select, Textarea | Focus border uses `blue-700` token |
| States (loading/error/empty) | Spinner/link colors retinted to brand blue scale |

## Findings

### Blocking

_None._

### Informational (carry-forward from QC, not P2 blockers)

- **W-canvas** — One dark canvas fill token (`--color-canvas-worldkb-entity-card-fill-selected`) still uses legacy `rgba(82, 168, 255, 0.14)`; matches current `DESIGN.dark.md` frontmatter but diverges from adjacent P2 retints. Schedule canvas polish follow-up.
- **W-blue-drift** — `--color-blue-700`…`1000` remain literal hex while `--color-brand-*` alias package vars; acceptable for ship; consolidate in a later token-codegen slice (qc1 S1).
- **W-test-noise** — Node 24 Vitest workers emit `ExperimentalWarning: localStorage…` despite `src/test/setup.ts` polyfill; CI uses Node 22; tests pass functionally.
- **W-test-brittleness** — Logo tests assert Vite data-URL hex encoding; bundler strategy change would require test update, not a runtime defect (qc2 W2).

## Not Tested

- Manual light/dark visual checklist (shell hover/focus/disabled states) — deferred to author spot-check; QC reports reviewed architecture and bundle evidence.
- Automated WCAG contrast tooling — P1 documented pairings; P2 inherits.
- Every product page pixel review — explicitly out of P2 scope per plan §2.3.

## Recommended Owners

- **Canvas token parity** (dark World KB selected fill): `@frontend-dev` when canvas polish slice opens
- **Blue scale consolidation / token codegen**: `@frontend-dev` + `@architect`
- **Vitest Node 24 log noise**: `@frontend-dev` or `@ops-engineer` (document Node pin in `apps/web/AGENTS.md`)
