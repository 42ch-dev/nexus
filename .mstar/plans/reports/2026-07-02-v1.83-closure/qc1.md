---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-02-v1.83-closure"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report — V1.83 Iteration Closeout (QC1 Consolidated)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: composer-2.5
- Review Perspective: Architecture coherence and maintainability (qc1); integrated P0+P1+P2 on `iteration/v1.83`
- Report Timestamp: 2026-07-02T18:30:00Z

## Scope
- plan_id: `2026-07-02-v1.83-closure`
- Review range / Diff basis: `main`..`HEAD` on branch `iteration/v1.83` (integrated V1.83 brand foundation: P0 `@42ch/nexus-ui`, P1 root DESIGN SSOT, P2 `apps/web` brand application)
- Working branch (verified): `iteration/v1.83`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- HEAD: `85f14b8b` — `chore(v1.83): mark P2 Done after merge and QA Pass`
- Merge-base with `main`: `5ed1ab327b44e41179c76f7cc2ec608e040b8589`
- Commits in range: 15
- Files reviewed: 51 changed paths (+2700 / −62 vs `main`)
- Deep review: triggered (multi-plan integration; token hierarchy; new publishable package; UI blast radius)
- Lenses applied: Architecture/Maintainability, Contract-boundary
- Tools run: `git rev-parse`, `git branch`, `git merge-base`, `git diff main...HEAD --stat`, `git lfs ls-files`, `git check-attr`, `pnpm --filter @42ch/nexus-ui run typecheck`, `pnpm --filter @42ch/nexus-ui run build`, `pnpm --filter web run typecheck`, `pnpm --filter web run test`, `pnpm --filter web run build`, Node export-resolution probe, `rg` (internal package paths, legacy hex, SSOT disclaimers), cross-read of per-plan QC/QA reports

## Integrated Architecture Assessment

| Layer | Criterion | Status | Evidence |
|-------|-----------|--------|----------|
| **P0 — Package** | `@42ch/nexus-ui` assets/tokens/theme only; no React | **Pass** | No `.tsx`; zero runtime deps; `exports` = 7 entries (root, tokens, theme.css, 4 SVGs); PNGs not exported |
| **P0 — LFS** | PNG provenance under Git LFS; SVG as text | **Pass** | `.gitattributes` pattern; `git lfs ls-files` → 3 PNGs; 4 SVG XML files |
| **P1 — DESIGN SSOT** | Root brand contract; Web as consumption mapping | **Pass** | Root `DESIGN.md` L119–126 four-layer hierarchy; `apps/web/DESIGN.md` L399 consumption disclaimer + links to root |
| **P1 — Token values** | VI palette frozen across layers | **Pass** | `#1E3A5F`, `#25D1E0`, `#FFFFFF` in root DESIGN, `tokens.ts`, `theme.css`, Web CSS aliases |
| **P2 — Public imports** | No relative `packages/nexus-ui` paths | **Pass** | `rg 'packages/nexus-ui' apps/web` → no matches; `@42ch/nexus-ui` export paths only |
| **P2 — Shell brand** | Theme-aware logo in header/sidebar | **Pass** | `NexusLogo` in `header.tsx` (mobile) and `sidebar.tsx` (desktop); light→`logo-dark.svg`, dark→`logo-color.svg` |
| **P2 — Build chain** | Web prebuild/pretypecheck chains package | **Pass** | `apps/web/package.json` builds contracts + nexus-ui; CI `web-build` job succeeds via lifecycle hooks |
| **Cross-plan** | Dependency order respected on integrated HEAD | **Pass** | P0 exports stable before P1 mapping notes and P2 consumption; no package API drift from P2 |
| **Per-plan QC** | P0 3/3 Approve; P2 3/3 Approve; P1 QA Pass (docs) | **Pass** | Reports under `.mstar/plans/reports/2026-07-02-v1.83-*/` |
| **Per-plan QA** | P0/P1/P2 QA Pass | **Pass** | QA reports verified on integrated branch |

## Findings

### 🔴 Critical
- None.

### 🟡 Warning

- **W1 — CI checkout does not fetch Git LFS objects**  
  `.github/workflows/ci.yml` and `.github/actions/setup-monorepo` use plain `actions/checkout` without `lfs: true`. Fresh CI clones retain LFS pointer text for PNG provenance files.  
  **Impact**: No build/test failure — PNGs are provenance-only and not in the export/build graph. Operational risk for designer/QA workflows opening PNG references without `git lfs pull`.  
  **Disposition**: Acceptable for V1.83 closeout; recommend adding `lfs: true` to checkout before integration PR or documenting LFS pull in `packages/nexus-ui/README.md`. Not a merge blocker.

- **W2 — Residual legacy blue rgba in one dark canvas token**  
  `--color-canvas-worldkb-entity-card-fill-selected` in `apps/web/src/index.css` (`.dark`) remains `rgba(82, 168, 255, 0.14)` while adjacent canvas/SOUL tokens were retinted to brand-cyan rgba in P2. Matches current `DESIGN.dark.md` frontmatter but diverges from the P2 re-tint pass.  
  **Impact**: Visual inconsistency on World KB selected cards in dark mode only; no security, data, or workflow regression.  
  **Disposition**: Acceptable for V1.83 shell/primitives scope; defer to canvas polish iteration (also noted in P2 per-plan QC).

- **W3 — Dual maintenance path for interactive blue scale**  
  `index.css` imports `@42ch/nexus-ui/theme.css` and aliases `--color-brand-*`, but `--color-blue-700`…`1000` remain literal hex in both `:root` and `.dark`. Brand palette changes require updating package theme **and** Web blue ladder separately.  
  **Impact**: Maintainability / drift risk across future brand tweaks; current values are aligned on integrated HEAD.  
  **Disposition**: Acceptable for ship; track for token-generation or documented `color-mix` derivation in a follow-up.

### 🟢 Suggestion

- **S1 — Stale SSOT comments in Web implementation files**  
  `apps/web/src/index.css` L4–7 and `apps/web/tailwind.config.ts` L4 still state `apps/web/DESIGN.md` is the SSOT. Root `DESIGN.md` is now brand SSOT; Web files are consumption mappings (`apps/web/AGENTS.md` L19 was corrected). One-line header updates would reduce agent/developer confusion.

- **S2 — Root `AGENTS.md` subdirectory index omits `packages/`**  
  `packages/nexus-ui/AGENTS.md` satisfies new-package policy, but root `AGENTS.md` indexes `apps/` and `crates/` only. Add a row for `@42ch/nexus-ui` (and optionally `@42ch/nexus-contracts`) for discoverability.

- **S3 — `apps/web/AGENTS.md` prebuild note incomplete**  
  Build/typecheck section (L43–48) documents only `@42ch/nexus-contracts` prebuild; implementation now also builds `@42ch/nexus-ui`. Developers running bare `tsc` without lifecycle hooks may misdiagnose missing types.

- **S4 — PNG vs SVG provenance naming asymmetry**  
  LFS PNGs use `logo_dark.png` / `logo_light.png`; public SVGs use `logo-dark.svg` / `logo-color.svg`. README documents mapping; a cross-reference table in `packages/nexus-ui/AGENTS.md` would reduce P1/P2 naming drift.

- **S5 — Logo unit tests couple to Vite data-URL encoding**  
  `nexus-logo.test.tsx` asserts URL-encoded hex in `src`. Bundler strategy change would fail tests without production defect. Prefer mocking asset imports or asserting variant selection via module map.

- **S6 — Node 24+ Vitest `localStorage` experimental warnings**  
  `src/test/setup.ts` polyfill works (387/387 pass), but Node 24 workers log `ExperimentalWarning: localStorage is not available…`. CI uses Node 22; document local Node pin or Vitest env if log noise matters.

- **S7 — Add `prepublishOnly` guard when npm publish is in scope**  
  `@42ch/nexus-contracts` models `prepublishOnly`; `@42ch/nexus-ui` could mirror for parity when publish is planned (explicit non-goal for V1.83).

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W1 | static-analysis | `.github/workflows/ci.yml` checkout steps; P0 qc2 F-006 | High |
| W2 | manual-reasoning | `apps/web/src/index.css:262`; P2 qc1 S3 / qc2 W1 | High |
| W3 | manual-reasoning | `apps/web/src/index.css` brand alias vs `--color-blue-*` blocks; `packages/nexus-ui/theme.css` | High |
| S1 | doc-rule | `apps/web/src/index.css:4-7`, `tailwind.config.ts:4` vs root `DESIGN.md` L119 | High |
| S2 | doc-rule | Root `AGENTS.md` index vs `packages/nexus-ui/AGENTS.md` | High |
| S3 | doc-rule | `apps/web/AGENTS.md:41-48` vs `apps/web/package.json:11-14` | High |
| S4 | manual-reasoning | `assets/logos/logo_*.png` vs `logo-*.svg` | Medium |
| S5 | static-analysis | `nexus-logo.test.tsx:30,41` | High |
| S6 | static-analysis | Vitest stdout during `pnpm --filter web run test` | High |
| S7 | manual-reasoning | Compare `packages/nexus-contracts/package.json` scripts | High |

## Validation Summary (Integrated HEAD)

| Gate | Command | Result |
|------|---------|--------|
| Package typecheck | `pnpm --filter @42ch/nexus-ui run typecheck` | Pass |
| Package build | `pnpm --filter @42ch/nexus-ui run build` | Pass |
| Web typecheck | `pnpm --filter web run typecheck` | Pass |
| Web tests | `pnpm --filter web run test` | Pass — 387/387 (51 files) |
| Web production build | `pnpm --filter web run build` | Pass |
| LFS tracking | `git lfs ls-files` + `git check-attr filter` | Pass — 3 PNGs |
| Token alignment probe | Node script: VI hex in root DESIGN + package + theme | Pass |
| Export boundary | No `packages/nexus-ui` relative imports in `apps/web` | Pass |
| Per-plan QC/QA | P0/P2 triple Approve; P0/P1/P2 QA Pass | Pass |

## Checklist (shared baseline)

### Code quality
- [x] Naming clear across package/Web layers (minor PNG/SVG asymmetry — S4)
- [x] Responsibilities separated (package tokens vs Web mapping vs shell components)
- [x] Comments reference DESIGN sections where non-obvious

### Security & correctness
- [x] Public export boundary enforced; PNG paths not exported
- [x] SVG rendered via `<img>` (no inline XSS surface)
- [x] No secrets in brand assets or token files
- [x] Theme localStorage reads controlled enum values only

### Performance & reliability
- [x] Lightweight package; deterministic build chain via `prepare` + Web prebuild hooks
- [x] Logo uses `decoding="async"`; no runtime fetch of package internals

### Maintainability
- [x] Four-layer token hierarchy documented and reflected in implementation
- [ ] Header comment SSOT drift in `index.css` / `tailwind.config.ts` (S1)
- [ ] Root AGENTS index missing `packages/` (S2)
- [x] No React component-library scope creep in `@42ch/nexus-ui`

### Tests
- [x] `nexus-logo.test.tsx` covers light/dark variants and custom label
- [x] Full web suite passes on integrated HEAD
- [ ] No package-level smoke import test (acceptable for V1.83 asset scaffold)

## Per-Plan QC Carry-Forward

| Plan | Prior QC verdict | Open items absorbed into closeout |
|------|------------------|-----------------------------------|
| P0 `2026-07-02-v1.83-nexus-ui-brand-assets` | 3/3 Approve | W1 (CI LFS), S2/S4/S7 from per-plan suggestions |
| P1 `2026-07-02-v1.83-brand-design-system` | QC skipped (docs); QA Pass | Neutral gray scale divergence documented as acceptable |
| P2 `2026-07-02-v1.83-web-brand-application` | 3/3 Approve | W2/W3, S1/S5/S6 from per-plan findings |

No per-plan Critical or unresolved blocking Warning remains after integration verification.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 7 |

**Verdict**: Approve

Integrated `iteration/v1.83` HEAD delivers a coherent V1.83 brand foundation: framework-neutral `@42ch/nexus-ui` with LFS-managed PNG provenance and canonical SVGs, root DESIGN SSOT with Web consumption mappings, and a focused `apps/web` shell/primitives application pass consuming public package exports. All automated gates pass on integrated HEAD. Open warnings are operational (CI LFS), visual parity (one canvas rgba), and maintainability (dual blue-scale path) — none block iteration closeout or PR readiness to `main`.
