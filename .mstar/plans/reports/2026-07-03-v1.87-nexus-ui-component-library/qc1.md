---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-03-v1.87-nexus-ui-component-library"
verdict: "Approve"
generated_at: "2026-07-03"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk (primary: package-boundary / component-API / wire-contract decisions; secondary: P1 delegation shape)
- Report Timestamp: 2026-07-03T20:05:00+08:00

## Scope
- plan_id: `2026-07-03-v1.87-nexus-ui-component-library`
- Review range / Diff basis: `git diff main...iteration/v1.87` (merge-base `ffae19f9` → tip `60916911`; 18 files, +777/-84)
- Working branch (verified): `iteration/v1.87`
- Working branch HEAD (verified): `60916911` (`Merge feature/v1.87-nexus-ui-component-library into iteration/v1.87`)
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel`)
- Files reviewed: 18 (10 source files + 4 test files + pnpm-lock.yaml + 3 harness/docs files)
- Commit range: `ffae19f9..60916911` (6 commits: 4 feat/fix + 1 chore + 1 merge)
- Deep review: triggered (S1: 777 lines / 18 files; S6: multi-module — `packages/nexus-ui/`, `apps/web/`, `crates/nexus-daemon-runtime/`)
- Lenses applied: Modularity Lens, Contract Lens, Input Validation Lens
- Tools run:
  - `pnpm --filter @42ch/nexus-ui run typecheck` → clean (exit 0)
  - `pnpm --filter @42ch/nexus-ui run test` → 7/7 pass (NexusLogo + NexusMark suites)
  - `pnpm --filter web run typecheck` → clean (after `@42ch/nexus-contracts` prebuild)
  - `cargo clippy -p nexus-daemon-runtime --no-deps -- -D warnings` → clean (no warnings)
  - `cargo test -p nexus-daemon-runtime --lib manuscript_read_range` → 4/4 pass (incl. sibling-escape regression)
  - `git diff main...iteration/v1.87 --stat -- schemas/ crates/nexus-contracts/ packages/nexus-contracts/` → empty (wire_contracts_changed confirmed `false`)
  - `rg '\.svg' packages/nexus-ui/src/**/*.{ts,tsx}` → 18 matches, all string-literal variant filenames or JSDoc comments; **zero `import '*.svg'` statements** (rule honored)
  - `rg 'from ["'"'"']@42ch/nexus-ui' apps/web/src` → 1 file (the wrapper); package source has no upward imports

## Findings

### 🔴 Critical
*(none)*

### 🟡 Warning
*(none)*

### 🟢 Suggestion

- **S-001 (R-V187-QC1-S001)** — `Variant` (exported from `packages/nexus-ui/src/components/nexus-logo.tsx`) and `LogoVariantName` (exported from `packages/nexus-ui/src/tokens.ts`) are duplicate type identities over the same four-string union (`'primary' | 'color' | 'white' | 'mono'`). `NexusLogoProps.variant: Variant` could use `LogoVariantName` directly, unifying the type identity with `logoVariants` and shrinking the public export surface. The runtime `VARIANT_FILENAMES` constant could also become a `Record<LogoVariantName, string>` instead of `Record<Variant, string>`.
  - Source Type: deep-lens: Contract Lens
  - Confidence: High
  - Impact: low (cosmetic — no behavior change); maintainability debt is minimal because the two types happen to stay in sync via the same literal values
  - Fix sketch: re-export `LogoVariantName` as `Variant` from `components/nexus-logo.tsx`, or drop the local `Variant` type and reference `LogoVariantName` directly. Optional.
  - Recommend PM register as a Suggestion residual (`R-V187-QC1-S001`, severity `Suggestion`) for a follow-up minor refactor; **not blocking** for V1.87 ship.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| S-001 | deep-lens: Contract Lens | `packages/nexus-ui/src/components/nexus-logo.tsx:9` (`export type Variant = ...`) vs. `packages/nexus-ui/src/tokens.ts:26` (`export type LogoVariantName = keyof typeof logoVariants`); runtime values overlap (`'primary' \| 'color' \| 'white' \| 'mono'`) | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: **Approve**

### Lens walkthrough (no findings, recorded for traceability)

**Modularity Lens** (default for QC1)

- Package boundary is correctly enforced: `@42ch/nexus-ui` source has no imports from `apps/web`, `nexus-platform`, or app routing/state. Grep over `packages/nexus-ui/src/**/*.{ts,tsx}` returns only intra-package relative imports (`./nexus-logo`, `../tokens`).
- The "no `.svg` imports in package source" rule is honored at the implementation level. All 18 `.svg` references in package source are either string-literal variant filenames (`logo-primary.svg`, `logo-color.svg`, etc.) or JSDoc prose. The package's bundler-agnostic portability design (consumer-supplied `src` for `<NexusLogo>`; hand-authored inline SVG JSX for `<NexusMark>`) is a sound architectural choice that prevents `tsup`/`esbuild` SVG-loader coupling.
- No circular dependencies introduced. The package is a pure consumer of its own `tokens.ts` and nothing else.
- Public API surface is appropriately minimal: two components + one constant + pre-existing token re-exports. No accidental bleed of layout primitives, theme-provider, or routing — consistent with `AGENTS.md` § Boundaries.
- Thin-wrapper migration in `apps/web` is clean: `apps/web/src/components/brand/nexus-logo.tsx` is a 35-line presentational wrapper that resolves the SVG via Vite's bundler and threads `theme → variant → src` through `<NexusLogoComponent>`. The two call sites (`sidebar.tsx:48` zero-prop; `header.tsx:20` with className) are unchanged and retain their zero-prop ergonomics.

**Contract Lens** (default for QC1)

- Public `exports` map is explicit and self-documenting: root (`.`), `./tokens`, `./theme.css`, and 4 SVG asset paths. No undocumented paths leak into the public API.
- `<NexusLogo>` props (`variant`, `src`, `label?`, `className?`, `size?`) are well-typed and presentational; no leaky abstractions (no theme context, no platform-gated state, no internal tokens beyond the minimum size).
- `<NexusMark>` props are minimal (`label?`, `className?`, `size?`) and `size` defaulting to `logoMinSizePx` (24) demonstrates internal-contract reuse without hardcoded literals.
- `VARIANT_FILENAMES` is the right export shape (`Record<Variant, string>`): enables consumers to programmatically map a variant to its canonical filename without hardcoding strings, and the type identity prevents drift between variant and filename lists.
- README/AGENTS.md/implementation are aligned: AGENTS.md § Boundaries states the no-`.svg` rule; nexus-logo.tsx and nexus-mark.tsx JSDoc reference it; README § "Why NexusLogo takes a src prop" reinforces it for consumers. Three layers of enforcement, no drift.
- Breaking change surface: `<NexusLogo>` is the only new public component, and version bump 0.1.0 → 0.2.0 is correctly SemVer-compatible (additive only within the package's pre-1.0 window). No changes to existing `@42ch/nexus-contracts` DTOs or `@42ch/nexus-ui/tokens` exports.

**Input Validation Lens** (on demand — P1 touches a path-guard handler)

- The 3-step delegation (`must_exist = abs_body.exists()` → `resolve_guarded_path` → map `BadRequest→InvalidInput`) correctly preserves the **prior** behavior of `execute_manuscript_read_range` (the old two-branch guard also returned `InvalidInput { field: "body_path", reason: ... }`). V1.87 is a refactor that closes `R-V186-QC1-S005` without changing the public error envelope.
- The implementer-flagged inconsistency — "ALL `BadRequest` variants are mapped to `InvalidInput`, which differs from other call sites" — is **intentional and correct**. `execute_read_file` / `execute_write_file` / `validate_file_path` map `BadRequest → Forbidden` because they represent fs/* path-escape attempts (a policy/authorization failure). The manuscript.read_range handler treats a bad body_path as **invalid input** (a corruption/data error from a DB-sourced path), which is a different semantic. The split is sound: each handler maps `BadRequest` to its domain-appropriate envelope. No information loss — the helper's `message` is preserved verbatim in `InvalidInput.reason`; the chapter-internal `code` (`chapter_path_*`) is package-private vocabulary that correctly does **not** leak through the public host-tool envelope.
- TOCTOU race window is documented in `path_guard.rs` (R-V166-QC2-TOCTOU, "racy-correct for single-user local daemon context"). No new TOCTOU surface introduced by the delegation.
- Defense-in-depth preserved: `nexus.manuscript.read_range` and `nexus.manuscript.write` now both delegate to the same canonical helper, eliminating the duplicated string-prefix branch that R-V186-QC1-S005 exploited.
- New regression test `manuscript_read_range_rejects_sibling_escape_body_path` exercises the exact attack vector (a sibling directory whose name extends the workspace root — `workspace-evil/` vs `workspace/`) that would have bypassed the old string-prefix check; it passes alongside the in-bounds happy path (`manuscript_read_range_accepts_in_bounds_body_path`).

## Residual Findings

PM may register the following under `residual_findings["2026-07-03-v1.87-nexus-ui-component-library"]` in `.mstar/status.json`:

| ID | Title | Severity | Decision | Owner | Target |
|----|-------|----------|----------|-------|--------|
| `R-V187-QC1-S001` | `Variant` (nexus-logo) and `LogoVariantName` (tokens) duplicate type identities over the same four-variant union — unify to a single export. | Suggestion | defer | TBD | V1.88+ (low priority — not blocking V1.87 ship) |

## Sign-off
- All static-analysis gates green (`pnpm typecheck` × 2, `cargo clippy -D warnings`, `vitest 7/7`, `cargo test manuscript_read_range 4/4`).
- `wire_contracts_changed: false` confirmed (no diff under `schemas/`, `crates/nexus-contracts/`, `packages/nexus-contracts/`).
- P1 closes `R-V186-QC1-S005` with explicit regression coverage.
- V1.87 ships **cleanly** under the architecture/maintainability lens.