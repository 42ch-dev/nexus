---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-02-v1.84-closure"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report — V1.84 (QC Reviewer #1: Architecture Coherence & Maintainability)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence & maintainability — token-reference discipline, parallel-scale hygiene, CI job house-style, warning-filter narrowness
- Report Timestamp: 2026-07-02T19:30:00+08:00

## Scope
- plan_id: `2026-07-02-v1.84-closure`
- Review range / Diff basis: `merge-base: main (1b19d69c)` … `tip: iteration/v1.84 HEAD (380ae1b6)`. Equivalent to `git diff main...iteration/v1.84`. Covers BOTH P0 (`apps/web/src/index.css`) and P1 (`.github/workflows/ci.yml`, `.github/workflows/desktop-build.yml`, `apps/web/vitest.config.ts`).
- Working branch (verified): `iteration/v1.84`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (HEAD `380ae1b6`, branch `iteration/v1.84`, merge-base with main `1b19d69c`)
- Files reviewed: 4 product/config files; 7 meta `.mstar/` artifacts (plan, status, iterations README — out of architecture scope; documented under Source Trace)
- Commit range: `8c249484` (P0) + `da123db7` (P1) on `iteration/v1.84`, with merge commits `06090e94` + `380ae1b6`
- Tools run: `git diff --stat`, `git diff main...iteration/v1.84 -- <file>`, `rg "#1e3a5f|#25d1e0"`, `rg "rgba\(30, 58, 95|rgba\(37, 209, 224"`, `rg "node-border-selected|worldkb-focus-ring|worldkb-entity-card|drift-band-fill"`, `pnpm exec tsc --noEmit -p apps/web/tsconfig.json`, `git diff --name-only main...iteration/v1.84 | rg "^(schemas/|packages/nexus-contracts/|...)"`. Local lint/typecheck on apps/web green. CI not yet triggered on this branch (no PR open, no `gh run list` hits — pending, not a finding).

## Findings

### 🔴 Critical
- (none)

### 🟡 Warning
- (none)

### 🟢 Suggestion

**S1 — Pre-existing (V1.83) documentation drift between `apps/web/DESIGN*.md` frontmatter and the live CSS variable `canvas-worldkb-relationship-grounded-badge` (deferred to a future iteration).**

`apps/web/DESIGN.md:388` documents `canvas-worldkb-relationship-grounded-badge: "rgba(0,107,255,0.12)"` (azure `#006BFF` @ 12%) and `apps/web/DESIGN.dark.md:364` documents the same key as `"rgba(82,168,255,0.14)"` (azure-light `#52A8FF` @ 14%). After V1.83 commit `34d0af45` ("apply V1.83 brand tokens"), and confirmed again by V1.84 P0 W005, the actual CSS in `apps/web/src/index.css:138 / :root` and `:287 / .dark` resolves to `color-mix(in srgb, var(--color-blue-700) 12%/14%, transparent)` → brand-deep-blue `#1e3a5f` (light) and brand-cyan `#25d1e0` (dark).

Provenance: the V1.83 docs commit `721ea0ac` records the deferred intent ("Re-tint hardcoded legacy blue `rgba(0,107,255,…)` in canvas/SOUL/findings tokens to brand rgba during P2; **token names stay frozen**"). V1.83 did the CSS retint (so the CSS values are correct relative to the brand decision) but did **not** update the `DESIGN.md` frontmatter entry. V1.84 P0 explicitly scopes itself to `apps/web/src/index.css` (the plan declares `apps/web/DESIGN*.md` as a non-goal). The drift is therefore pre-existing, not introduced by V1.84 P0 — but neither does V1.84 P0 close it; the frontmatter remains the only place in the repo that still advertises the V1.74 azure value.

Why this matters for QC1 architecture/maintainability: `DESIGN.md` / `DESIGN.dark.md` are the documented SSOTs for `apps/web/` tokens (Tailwind config at `apps/web/tailwind.config.ts:1-21` reads them as the design contract). A future token-editor who reads `DESIGN.md` and changes `0.12 → 0.13` to bump the badge opacity would touch only docs, leaving the CSS opacity untouched — exactly the dual-source hazard V1.84 P0 W005 was designed to eliminate on the CSS side. The fix exists: align the frontmatter with the post-V1.83 brand-blue path (i.e. `color-mix(in srgb, {colors.blue-700} 12%/14%, transparent)`, mirroring the existing `--color-canvas-write-stale-bg` pattern at `index.css:110/261`).

Suggested treatment: PM registers a `low` residual (e.g. `R-V184DOC-W001` or similar) on `status.json` root `residual_findings[<plan-id>]` for V1.85+ or whenever the next plan touches `apps/web/DESIGN*.md`. Scope is a single YAML frontmatter edit (light + dark); no CSS change required.

**S2 — Vitest warning filter uses a side-effecting IIFE-like block at config-file module load.** (Maintenance note, not a blocker.)

`apps/web/vitest.config.ts:11-25` registers a `process.on('warning')` handler at module load before `defineConfig({...})` exports. The narrow filter (`warning.name === 'ExperimentalWarning' && warning.message.includes('localStorage')`) and the `process.off / nextTick / emitWarning / on` re-entrancy dodge are correct, well-documented (top-of-file W003 comment), and now coexists with `apps/web/src/test/setup.ts:19-49` `ensureLocalStorage()` polyfill (which handles a different facet of the same Node-24+ localStorage issue). The narrowness vs. blanket suppression is the right call; a future noise pattern would naturally extend this block with another `if`. Worth promoting the single pattern to a small named `SUPPRESS_PATTERNS` array when a second filter arrives, but that's a small style choice, not a maintainability hazard today. Comment explicitly cites Node-24+ `ExperimentalWarning` and `--localstorage-file`, so if Node changes the warning shape the degradation is "test output noise returns" — visible, not silent data loss.

## Source Trace

| Finding | Source Type | Source Reference | Confidence |
|---------|-------------|------------------|------------|
| S1 | git-diff + manual-reasoning (historical audit) | `apps/web/src/index.css:138, :287` vs `apps/web/DESIGN.md:388` vs `apps/web/DESIGN.dark.md:364`; V1.83 retint commit `34d0af45`; V1.83 deferred-to-P2 note in `721ea0ac` | High |
| S2 | manual-reasoning + doc-rule | `apps/web/vitest.config.ts:5-25` (W003 comment), `apps/web/src/test/setup.ts:15-49` (node-24+ dual strategy) | High |
| P0 invariants pass | static-analysis (rg) | `rg "#1e3a5f|#25d1e0" apps/web/src/index.css` → 2 lines (canonical defs only); `rg "rgba(30, 58, 95\|rgba(37, 209, 224" apps/web/src/index.css` → 0 lines | High |
| P0 canvas-page sweep passes | static-analysis (rg) | `rg "rgba(30, 58, 95\|rgba(37, 209, 224\|#1e3a5f\|#25d1e0" apps/web/src/components/canvas/ apps/web/src/pages/` → 0 lines | High |
| P0 color-mix alpha faithfulness | manual-reasoning + git diff | `rgba(R,G,B,0.12)` vs `color-mix(in srgb, var(--color-blue-700) 12%, transparent)` — both produce `rgba(R,G,B,0.12)` in sRGB; alphas 12/16/14/14/22 preserved exactly across all 5 tinted tokens | High |
| P0 var resolution `:root` | manual-reasoning (CSS scope cascade) | `--color-blue-700` def line 60 → usages 103, 119, 130, 138, 149 (all within the same `:root` selector; cascade-defined-before-use inside `:root`) | High |
| P0 var resolution `.dark` | manual-reasoning (CSS scope cascade) | `--color-blue-700` def line 209 → usages 252, 266, 268, 279, 287, 298 (all within the same `.dark` selector; cascade-defined-before-use inside `.dark`; correctly overrides `:root` for `.dark`-classed elements) | High |
| P0 house-pattern match | manual-reasoning + git diff | New `color-mix(in srgb, var(--color-blue-700) N%, transparent)` form exactly matches pre-existing `--color-canvas-write-stale-bg` (lines 110/261, amber) — no new shape invented | High |
| P0 non-brand tokens untouched | static-analysis (manual diff review) | Non-changed: `--color-canvas-worldkb-relationship-asserted-badge` (purple-700 tints — non-brand, correctly left as rgba), all `gray`/`red`/`amber`/`green`/`teal`/`purple`/`pink` accent scales, all `elevation`/`gray-alpha`/`background`/`soul-narrative`/`soul-growth-curve` tokens. No accidental retint. | High |
| P1 `nexus-ui` job self-containment | manual-reasoning + file audit | `packages/nexus-ui/package.json:53-58` (`devDependencies`: `tsup` + `typescript` only; zero runtime deps); `packages/nexus-ui/src/{index,tokens}.ts` import nothing from `@42ch/nexus-contracts`; therefore no `needs: verify-codegen` and no `generated-types` artifact download — correct independence, parallels the existing `verify-no-acp-in-daemon` / `schema-consistency-check` jobs | High |
| P1 house-style alignment | manual-reasoning + visual diff | `nexus-ui` job uses `runs-on: ubuntu-latest`, pinned `actions/checkout@34e114876b0b11c390a56381ad16f8d5 # v4` (with `lfs: true`), `./.github/actions/setup-monorepo` (rust-toolchain ""), then `pnpm --filter @42ch/nexus-ui run build` + `pnpm --filter @42ch/nexus-ui run typecheck` — matches `web-build`/`typescript-checks` style; no per-job `paths:` filter (workflow-level `paths-ignore` only), matching ci.yml convention | High |
| P1 LFS scope | manual-reasoning + .gitattributes | `.gitattributes` scopes LFS to `packages/nexus-ui/assets/logos/*.png` only; additions cover `web-build` (transitive consumers), `nexus-ui` (own job), and both `desktop-build.yml` checkouts (aarch64 macOS — package metadata symlink triggers); 8 lint-only/check-only jobs correctly left without `lfs: true`. Bounded cost. | High |
| P1 vitest config side-effect | manual-reasoning | `apps/web/vitest.config.ts:11-25` IIFE-like block; module-load side effect is acceptable for the single-process Vitest runner; no assertion hidden (filter is read by `process.emit` chain, not by Vitest's matchers); narrow (name + substring match) | High |
| P0/P1 file-set isolation | git diff + manual-reasoning | P0 commit `8c249484` touches only `apps/web/src/index.css`. P1 commit `da123db7` touches only `.github/workflows/ci.yml`, `.github/workflows/desktop-build.yml`, `apps/web/vitest.config.ts`. `apps/web/src/test/setup.ts` (P1 carve-out) is unchanged. Zero overlap. | High |
| Contract-unchanged verification | git-diff one-liner from closure plan §Clarify | `git diff --name-only main...iteration/v1.84 \| rg "^(schemas/\|packages/nexus-contracts/\|crates/nexus-contracts/\|crates/nexus-daemon-runtime/\|apps/nexus42/)"` → 0 lines. `git diff main...iteration/v1.84 -- packages/nexus-ui/package.json` → 0 lines (no `exports` change). `wire_contracts_changed: false` confirmed. | High |
| Local typecheck (apps/web) | pnpm exec tsc --noEmit | `pnpm exec tsc --noEmit -p apps/web/tsconfig.json` exit 0, no errors. | High |
| CI status | gh CLI | `gh pr list --head iteration/v1.84` → empty (no PR yet — closure T6 not run); `gh run list --branch iteration/v1.84` → empty. CI pending — not a finding per the assignment note. | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: **Approve**

Justification (per `mstar-review-qc` gate rules): no `Critical`, no unresolved `Warning`. Both suggestions are pre-existing / maintenance observations, not blockers for V1.84. P0 invariants pass mechanically. P1 house-style alignment is faithful. File-set isolation between P0 and P1 is clean. No wire-contract / package-export change. No regen needed.

## Residual suggestions for PM to register

- **R-V184DOC-W001** (severity `low`, owner: `@writing-specialist` or `@architect`): align `apps/web/DESIGN.md:388` and `apps/web/DESIGN.dark.md:364` frontmatter for `canvas-worldkb-relationship-grounded-badge` with the post-V1.83 brand-blue path (i.e. reflect `color-mix(in srgb, {colors.blue-700} 12%/14%, transparent)` rather than the V1.74 azure `#006BFF`). The token name stays frozen per V1.83 docs; only the documented value needs updating. This closes the parallel-source hazard that V1.84 P0 W005 left half-finished (CSS-side only). Source of concern: S1 above. Track under `.mstar/status.json` root `residual_findings[<plan-id>]` for V1.85+ when the next `apps/web/DESIGN*.md` consumer touches the file.
