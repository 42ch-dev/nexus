---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-02-v1.83-closure"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report — V1.83 Iteration Closeout (QC2 Consolidated)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: composer-2.5
- Review Perspective: Security and correctness (package export boundary, SVG/asset safety, token hierarchy integrity, no nexus-platform leakage, build/export determinism)
- Report Timestamp: 2026-07-02

## Scope
- **plan_id**: `2026-07-02-v1.83-closure`
- **Review range / Diff basis**: `main`..`HEAD` on branch `iteration/v1.83` (integrated P0 `@42ch/nexus-ui` + P1 root DESIGN SSOT + P2 `apps/web` brand application)
- **Working branch (verified)**: `iteration/v1.83`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **HEAD**: `85f14b8b` — `chore(v1.83): mark P2 Done after merge and QA Pass`
- **Merge-base with `main`**: `5ed1ab327b44e41179c76f7cc2ec608e040b8589`
- **Files reviewed**: 51 changed paths (+2700 / −62 vs `main`)
- **Tools run**: `git rev-parse`, `git branch`, `git merge-base`, `git diff main...HEAD --stat`, `git lfs ls-files`, `pnpm --filter @42ch/nexus-ui run typecheck`, `pnpm --filter @42ch/nexus-ui run build`, `pnpm --filter web run typecheck`, `pnpm --filter web run test`, `pnpm --filter web run build`, Node export-resolution probe from `apps/web`, `rg` (SVG XSS patterns, secrets, internal package paths, nexus-platform references in product code), cross-read of per-plan QC/QA reports

## V1.83 Closeout Security / Correctness Checklist

| Area | Result | Evidence |
|------|--------|----------|
| **Architecture — OSS vs platform boundary** | **Pass** | No `nexus-platform` imports, types, auth, or cloud-gated features in `apps/web`, `packages/nexus-ui`, or V1.83 product diffs. Boundary mentions appear only in AGENTS/README boundary docs (`apps/web/AGENTS.md`, `packages/nexus-ui/AGENTS.md`). |
| **Architecture — package isolation** | **Pass** | `@42ch/nexus-ui` has zero runtime deps; no imports from `apps/web` or platform paths. `apps/web` consumes package only via documented `@42ch/nexus-ui/*` exports — no relative `packages/nexus-ui` paths. |
| **Package export policy** | **Pass** | `package.json` `exports` documents 7 public entries (root, `./tokens`, `./theme.css`, four SVG subpaths). PNG provenance paths intentionally omitted; Node resolve from `apps/web` returns `ERR_PACKAGE_PATH_NOT_EXPORTED` for `logo_dark.png`. No React/TSX in package tree. |
| **DESIGN layering / SSOT correctness** | **Pass** | Root `DESIGN.md` L119–126 defines four-layer hierarchy (root → `@42ch/nexus-ui` → `apps/web/DESIGN*` → implementation). `apps/web/DESIGN.md` L399 explicitly disclaims brand SSOT and links to root. VI palette `#1E3A5F` / `#25D1E0` / `#FFFFFF` aligned across root DESIGN, `tokens.ts`, `theme.css`, and Web CSS aliases. |
| **Git LFS / asset policy** | **Pass** | `.gitattributes` tracks `packages/nexus-ui/assets/logos/*.png`; `git lfs ls-files` lists 3 PNGs. Four SVGs are normal git text; no `<script>`, event handlers, `foreignObject`, or `javascript:` in logo assets. |
| **SVG XSS / rendering safety** | **Pass** | `NexusLogo` renders logos via `<img src={bundledSvg}>` (not inline SVG / `dangerouslySetInnerHTML`). Production bundle embeds SVG as data URLs under `<img>` — inert script content would not execute. |
| **Secrets / credentials** | **Pass** | No API keys, tokens, private keys, or credential patterns in V1.83 product diff (`packages/nexus-ui/**`, `apps/web/**`, root DESIGN files). |
| **Theme localStorage integrity** | **Pass** | `ThemeProvider` reads only `'light' \| 'dark'` from `nexus-web-theme`; writes controlled by `setTheme` / `toggleTheme`. Test polyfill in `src/test/setup.ts` is Vitest-only. |
| **Build / typecheck determinism** | **Pass** | `@42ch/nexus-ui` `prepare` builds `dist/` on install; `apps/web` `prebuild`/`pretypecheck` chain contracts + nexus-ui. Integrated HEAD: package typecheck/build, web typecheck (387/387 tests), and production build all pass locally. |
| **Auth / data / network boundary** | **Pass** | V1.83 diff is static assets, CSS tokens, shell chrome, and design docs — no new fetch paths, wire-contract changes, or daemon API edits. |
| **Per-plan QC/QA gates** | **Pass** | P0 3/3 Approve + QA Pass; P1 QA Pass (docs); P2 3/3 Approve + QA Pass. No unresolved Critical from per-plan reviews. |

## Findings

### 🔴 Critical
None.

### 🟡 Warning

- **W1 — CI checkout does not fetch Git LFS objects**  
  `.github/workflows/ci.yml` uses plain `actions/checkout` without `lfs: true`. CI clones retain LFS pointer text for PNG provenance files rather than raster bytes.  
  **Impact**: No build/typecheck/test failure — PNGs are provenance-only and not exported or consumed in the build graph. Operational risk for designer/QA workflows opening PNG references inside CI or fresh clones without `git lfs pull`.  
  **Disposition**: Acceptable for V1.83 closeout; recommend adding `lfs: true` to checkout before integration PR or documenting `git lfs install && git lfs pull` in `packages/nexus-ui/README.md`. Not a merge blocker (carried from P0 qc2).

- **W2 — Residual legacy blue rgba in one dark canvas fill token**  
  `--color-canvas-worldkb-entity-card-fill-selected` in `apps/web/src/index.css` (`.dark`, L262) remains `rgba(82, 168, 255, 0.14)` while adjacent canvas/SOUL tokens were retinted to brand-cyan rgba in P2. Matches current `DESIGN.dark.md` frontmatter but diverges from the P2 re-tint pass.  
  **Impact**: Visual/correctness inconsistency on World KB selected cards in dark mode only; no security, data-integrity, or workflow regression.  
  **Disposition**: Acceptable for V1.83 shell/primitives scope; defer to canvas polish iteration (also noted in P2 per-plan QC).

- **W3 — Logo unit tests couple to Vite data-URL encoding**  
  `nexus-logo.test.tsx` asserts `src` contains URL-encoded settings (`%231E3A5F`, `%2325D1E0`). A bundler strategy change (file asset URL instead of inline data URL) would fail tests without a production defect.  
  **Impact**: Test maintainability risk only; not a runtime security issue.  
  **Disposition**: Acceptable for closeout; prefer mocking asset imports or asserting variant selection via module map in follow-up.

### 🟢 Suggestion

- **S1 — Dual maintenance path for interactive blue scale**  
  `index.css` aliases `--color-brand-*` to `--nexus-brand-*` from `@42ch/nexus-ui/theme.css`, but `--color-blue-700`…`1000` remain literal hex in both `:root` and `.dark`. Brand palette changes require updating package theme and Web blue ladder separately. Current values are aligned on integrated HEAD.

- **S2 — Stale SSOT comments in Web implementation headers**  
  `apps/web/src/index.css` and `apps/web/tailwind.config.ts` header comments still describe `apps/web/DESIGN.md` as SSOT. Root `DESIGN.md` is now brand SSOT; Web files are consumption mappings. One-line header updates would reduce agent/developer confusion.

- **S3 — `apps/web/AGENTS.md` prebuild note incomplete**  
  Build/typecheck section documents only `@42ch/nexus-contracts` prebuild; implementation now also builds `@42ch/nexus-ui`. Developers running bare `tsc` without lifecycle hooks may misdiagnose missing types.

- **S4 — Node 24+ Vitest `localStorage` experimental warnings**  
  `src/test/setup.ts` polyfill restores functional tests (387/387 pass), but Node 24 workers log `ExperimentalWarning: localStorage is not available…`. CI uses Node 22; document local Node pin or Vitest env if log noise matters.

- **S5 — Add `prepublishOnly` guard when npm publish is in scope**  
  `@42ch/nexus-contracts` models `prepublishOnly`; `@42ch/nexus-ui` could mirror for parity when publish is planned (explicit non-goal for V1.83).

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| Arch boundary | static-analysis | `rg nexus-platform apps/web packages/nexus-ui DESIGN*.md` — boundary docs only | High |
| Export boundary | static-analysis | `packages/nexus-ui/package.json:15-40`; Node resolve probe from `apps/web` | High |
| PNG blocked | static-analysis | `@42ch/nexus-ui/assets/logos/logo_dark.png` → `ERR_PACKAGE_PATH_NOT_EXPORTED` | High |
| LFS policy | git | `.gitattributes:1-2`; `git lfs ls-files` → 3 PNGs | High |
| SVG safety | static-analysis | `rg -i` script/event/foreignObject across logos + brand component; `nexus-logo.tsx:22-29` | High |
| DESIGN hierarchy | doc-rule | Root `DESIGN.md:119-126`; `apps/web/DESIGN.md:399` | High |
| Token alignment | manual-reasoning | VI hex in root DESIGN + `tokens.ts` + `theme.css` + Web aliases | High |
| W1 | static-analysis | `.github/workflows/ci.yml` checkout; P0 qc2 F-006 | High |
| W2 | manual-reasoning | `apps/web/src/index.css:262`; P2 qc2 W1 | High |
| W3 | static-analysis | `nexus-logo.test.tsx:30,41` | High |
| Build gates | command | typecheck/build/test runs on integrated HEAD | High |

## Validation Summary (Integrated HEAD)

| Gate | Command | Result |
|------|---------|--------|
| Package typecheck | `pnpm --filter @42ch/nexus-ui run typecheck` | Pass |
| Package build | `pnpm --filter @42ch/nexus-ui run build` | Pass |
| Web typecheck | `pnpm --filter web run typecheck` | Pass |
| Web tests | `pnpm --filter web run test` | Pass — 387/387 (51 files) |
| Web production build | `pnpm --filter web run build` | Pass |
| Export resolution (public) | Node `require.resolve` from `apps/web` | Pass — root, tokens, theme.css, 2 SVGs |
| Export resolution (blocked) | `logo_dark.png` | Pass — `ERR_PACKAGE_PATH_NOT_EXPORTED` |
| Internal path scan | `rg 'packages/nexus-ui' apps/web/src` | Pass — no matches |
| Per-plan QC/QA | P0/P2 triple Approve; P0/P1/P2 QA Pass | Pass |

## Per-Plan QC Carry-Forward (Security Lens)

| Plan | Prior QC2 verdict | Closeout disposition |
|------|-------------------|----------------------|
| P0 `2026-07-02-v1.83-nexus-ui-brand-assets` | Approve | W1 (CI LFS) absorbed; export/LFS/SVG/secrets checks re-verified on integrated HEAD |
| P1 `2026-07-02-v1.83-brand-design-system` | QC skipped (docs); QA Pass | Token hierarchy and cyan contrast rules verified via doc cross-check |
| P2 `2026-07-02-v1.83-web-brand-application` | Approve | Export boundary, SVG safety, theme storage, build chain re-verified; W2/W3 carried |

No per-plan Critical or unresolved blocking Warning remains after integrated verification.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 5 |

**Verdict**: Approve

Integrated `iteration/v1.83` HEAD satisfies V1.83 closeout security and correctness gates: framework-neutral `@42ch/nexus-ui` enforces a documented export surface with PNG paths blocked, root DESIGN SSOT layering is preserved with Web as consumption mapping, `apps/web` consumes only public package exports with safe static-asset rendering, and no nexus-platform implementation leakage appears in the product slice. All automated validation passes on integrated HEAD. Open warnings are operational (CI LFS), visual parity (one canvas rgba), and test brittleness — none block iteration closeout or PR readiness to `main`.
