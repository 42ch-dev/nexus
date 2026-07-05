---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-02-v1.83-web-brand-application"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: composer-2.5
- Review Perspective: Architecture coherence and maintainability (qc1)
- Report Timestamp: 2026-07-02T18:20:00Z

## Scope
- plan_id: `2026-07-02-v1.83-web-brand-application`
- Review range / Diff basis: `merge-base iteration/v1.83..HEAD`
- Working branch (verified): `feature/v1.83-web-brand-application`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 16 (+178 / −34 lines)
- Commit range: `e86652d4..34d0af45` (1 commit: `34d0af45 feat(web): apply V1.83 brand tokens and logo to shell UI`)
- Deep review: triggered (UI/design-system slice; token contract consumption; plan-declared rename/churn risk)
- Lenses applied: Architecture/Maintainability, Contract-boundary
- Tools run: `git rev-parse`, `git branch`, `git merge-base`, `git diff`, `git log`, `pnpm --filter web run typecheck`, `pnpm --filter web run test`, `pnpm --filter web run build`, grep (legacy hex / internal import paths), DESIGN.md cross-check

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None blocking approval.

### 🟢 Suggestion

- **S1 — Dual token source for interactive blue scale**  
  `index.css` imports `@42ch/nexus-ui/theme.css` and aliases `--color-brand-*` to `--nexus-brand-*`, but `--color-blue-700`…`1000` remain literal hex in both `:root` and `.dark` blocks. If root brand values change in P0/P1, Web must update two places. Consider deriving the preserved `blue-*` alias ladder from `--color-brand-deep-blue` / `--color-brand-cyan` (e.g. documented `color-mix` steps) or a single generated mapping step documented in `apps/web/DESIGN*.md`.

- **S2 — Redundant per-control focus border classes**  
  `input.tsx`, `select.tsx`, and `textarea.tsx` add `focus-visible:border-blue-700` while `index.css` already applies the DESIGN two-layer focus ring to `:where(input, select, textarea, …):focus-visible`. The border tweak is visually subtle and duplicates the global contract. Prefer relying on the global ring alone unless DESIGN explicitly requires border-color shift on controls.

- **S3 — Residual legacy blue in one dark canvas fill token**  
  `--color-canvas-worldkb-entity-card-fill-selected` in `.dark` still uses `rgba(82, 168, 255, 0.14)`, matching current `DESIGN.dark.md` frontmatter but diverging from the P2 re-tint pass applied to adjacent canvas/SOUL tokens. Schedule a DESIGN.dark + CSS follow-up for full brand-cyan parity when canvas polish is in scope.

- **S4 — Node 24 `localStorage` test shim noise**  
  `src/test/setup.ts` adds a sensible in-memory polyfill, yet Vitest workers still emit `ExperimentalWarning: localStorage is not available…` on startup. Consider a Vitest `poolOptions` / env flag or documented Node version pin in `apps/web/AGENTS.md` to keep CI logs clean.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| S1 | manual-reasoning | `apps/web/src/index.css` brand alias vs `--color-blue-*` hex blocks; `packages/nexus-ui/theme.css` | High |
| S2 | doc-rule | `apps/web/src/index.css` focus-visible rule vs `input.tsx`/`select.tsx`/`textarea.tsx` | High |
| S3 | manual-reasoning | `apps/web/src/index.css` `.dark` canvas-worldkb fill; `apps/web/DESIGN.dark.md` | Medium |
| S4 | static-analysis | Vitest stdout during `pnpm --filter web run test` | High |

## Architecture Assessment (qc1 focus)

| Criterion | Status | Notes |
|-----------|--------|-------|
| Plan scope discipline (shell + primitives, no page IA / backend) | Pass | Diff limited to `apps/web` styling, layout chrome, one brand component, test setup, lockfile |
| Public `@42ch/nexus-ui` boundary | Pass | Logo SVGs and `theme.css` imported via documented `exports`; no relative `packages/nexus-ui` paths |
| Token-name preservation / alias strategy | Pass | Existing `blue-*` and canvas token names frozen; values retinted per `apps/web/DESIGN.md` §Implementation Mapping (P2) |
| Centralized brand surface | Pass | `NexusLogo` encapsulates theme-aware mark selection aligned with root `DESIGN.md` §Logo Usage |
| Build/typecheck contract | Pass | `prebuild` / `pretypecheck` now chain `@42ch/nexus-ui` build alongside contracts |
| Downstream churn containment | Pass | Canvas/SOUL rgba retints reuse existing CSS var names; no route or data-flow edits |
| Test coverage for brand wiring | Pass | `nexus-logo.test.tsx` covers light/dark variants and custom accessible label |
| Dependency justification | Pass | `@42ch/nexus-ui` workspace dep matches P0/P1 deliverable consumption model |

## Checklist (shared baseline)

### Code quality
- [x] Naming clear (`NexusLogo`, `brand-*` Tailwind keys)
- [x] Responsibilities separated (brand component vs shell layout vs token layer)
- [x] Comments reference DESIGN sections where non-obvious

### Security & correctness
- [x] No new auth/data paths; static SVG imports only
- [x] Accessible logo labels (`role="img"`, configurable `label`)

### Performance & reliability
- [x] Logo uses `decoding="async"`; no runtime fetch of package internals
- [x] Build chain remains deterministic via workspace `prepare` on `@42ch/nexus-ui`

### Maintainability
- [x] Consumption mapping documented in existing `apps/web/DESIGN.md` (P2 section)
- [ ] Token hex duplication (S1) — acceptable for ship; track for consolidation
- [x] No component-library extraction scope creep

### Tests
- [x] New logo tests; full web suite 387/387 pass
- [x] `typecheck` and production `build` pass

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

P2 delivers a focused, architecture-sound brand application pass: public package exports, preserved Web token names, a reusable `NexusLogo`, shell integration, and primitive retints without backend or IA churn. Open suggestions (S1–S4) are maintainability and polish follow-ups, not merge blockers.
