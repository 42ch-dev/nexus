---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-02-v1.83-nexus-ui-brand-assets"
verdict: "Request Changes"
generated_at: "2026-07-02"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: composer-2.5
- Review Perspective: Architecture coherence and maintainability (qc1)
- Report Timestamp: 2026-07-02T00:00:00Z

## Scope
- plan_id: `2026-07-02-v1.83-nexus-ui-brand-assets`
- Review range / Diff basis: `git merge-base iteration/v1.83 HEAD`..`HEAD` on branch `feature/v1.83-nexus-ui-brand-assets`
- Working branch (verified): `feature/v1.83-nexus-ui-brand-assets`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 17 (1 commit: `bce85d6d`)
- Commit range (if not identical to Review range line, explain): `25e126e1..bce85d6d` (single implementation commit atop merge-base with `iteration/v1.83`)
- Tools run: `git rev-parse`, `git branch`, `git diff`, `git lfs ls-files`, `pnpm --filter @42ch/nexus-ui run typecheck`, `pnpm --filter @42ch/nexus-ui run build`, `xxd` (SVG byte inspection)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning

- **W1 — Corrupted control character in SVG `<desc>` (all four logo variants)**  
  Each canonical SVG (`logo-color.svg`, `logo-dark.svg`, `logo-white.svg`, `logo-mono.svg`) uses `aria-labelledby="nexus-logo-title nexus-logo-desc"`, but the `<desc>` text contains ASCII `0x14` (DC4) between “mark” and “rounded” instead of punctuation (e.g. em dash or hyphen). Screen readers that honor `aria-labelledby` may announce garbled content alongside the valid `<title>`.  
  **Evidence:** `xxd` on `logo-color.svg` shows bytes `6d 61 72 6b 14 20 72 6f 75 6e 64` (“mark\x14 rounded”).  
  **Fix:** Replace `0x14` with `—` or ` - ` in all four SVG `<desc>` elements; re-verify with `xxd` or `rg` that no control bytes remain.

- **W2 — TypeScript entry points require pre-build; no install-time hook**  
  `package.json` `main` / `types` / `exports["."].*` resolve to `./dist/*`, but `dist/` is gitignored (consistent with `@42ch/nexus-contracts`). There is no `prepare` / `prepublishOnly` script (contracts has `prepublishOnly` only). P2 `apps/web` consumers will fail type resolution until `pnpm --filter @42ch/nexus-ui run build` is run locally or wired into CI/turbo.  
  **Fix:** Add a documented build prerequisite for P2 (root turbo pipeline or `prepare` script), or commit built `dist/` if the monorepo policy for publishable packages changes.

### 🟢 Suggestion

- **S1 — Root `AGENTS.md` subdirectory index omits `packages/`**  
  New-package policy is satisfied via `packages/nexus-ui/AGENTS.md`, but root `AGENTS.md` indexes `apps/` and `crates/` only. Adding a `packages/` row (at least `@42ch/nexus-ui`, `@42ch/nexus-contracts`) improves discoverability for agents.

- **S2 — PNG vs SVG provenance naming asymmetry**  
  LFS PNGs use `logo_dark.png` / `logo_light.png` while public SVGs use `logo-dark.svg` (deep blue, light surfaces) and `logo-color.svg` (cyan, dark chrome). README documents the mapping; a one-line cross-reference table in `AGENTS.md` would reduce P1/P2 naming drift risk.

- **S3 — Align publish guard with `@42ch/nexus-contracts`**  
  Consider adding `prepublishOnly: "pnpm run build && pnpm run typecheck"` for parity, even though npm publish is out of scope for V1.83.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W1 | manual-reasoning | `xxd packages/nexus-ui/assets/logos/logo-color.svg`; all four `assets/logos/*.svg` | High |
| W2 | git-diff | `packages/nexus-ui/package.json` exports → `./dist/*`; `.gitignore:83:dist` | High |
| S1 | doc-rule | Root `AGENTS.md` subdirectory index vs `packages/nexus-ui/AGENTS.md` | High |
| S2 | manual-reasoning | `assets/logos/logo_*.png` vs `logo-*.svg` naming | Medium |
| S3 | manual-reasoning | Compare `packages/nexus-contracts/package.json` scripts | High |

## Architecture Assessment (qc1 focus)

| Criterion | Status | Notes |
|-----------|--------|-------|
| Package boundary (assets/tokens/theme only, no React) | Pass | `src/` has tokens only; no React deps; `AGENTS.md` and README state boundaries clearly |
| Compass / plan alignment (V1.83 SP-1) | Pass | `@42ch/nexus-ui` scaffold, LFS PNG policy, SVG family, documented exports match plan §2 and compass §2.1 hierarchy |
| Public export contract | Pass | `package.json` `exports` matches README table; PNGs intentionally not exported (provenance only) |
| Git LFS policy | Pass | `.gitattributes` pattern + `git lfs ls-files` shows 3 PNG pointers |
| Framework neutrality | Pass | Zero runtime deps; `theme.css` uses generic custom properties |
| Workspace integration | Pass | Registered under `packages/*` in root `package.json`; lockfile importer `packages/nexus-ui` present |
| Token palette vs compass VI | Pass | `#1E3A5F`, `#25D1E0`, `#FFFFFF` in `tokens.ts` and `theme.css` |
| Downstream API stability for P1/P2 | Pass with W2 | Export names are stable; build prerequisite must be explicit before P2 consumption |

## Checklist (shared baseline)

### Code quality
- [x] Naming clear and consistent (minor PNG/SVG asymmetry — S2)
- [x] Responsibilities not over-mixed (tokens, assets, theme separated)
- [x] No runtime error paths in scope (static assets/tokens)
- [x] Comments state intent (P1 DESIGN SSOT deferral documented)

### Security & correctness
- [x] No injection/path traversal surfaces in package API
- [x] LFS pointers for binaries; SVG as text
- [x] No secrets in package

### Performance & reliability
- [x] Lightweight package; no hot-path concerns
- [x] Asset paths stable for bundler URL imports

### Maintainability
- [x] `AGENTS.md` documents boundaries per new-package policy
- [x] Public API documented in README + `exports`
- [ ] Install/build ergonomics for workspace consumers (W2)

### Tests
- [ ] No automated tests (acceptable for P0 asset scaffold; P-last may add smoke import test in closure plan)

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

P0 architecture is coherent and aligns with the V1.83 compass: framework-neutral `@42ch/nexus-ui` with explicit exports, Git LFS for PNG provenance, canonical SVGs, and clear no-React boundary. Resolve W1 (SVG accessibility text corruption) and document or automate the W2 build prerequisite before P2 integration.
