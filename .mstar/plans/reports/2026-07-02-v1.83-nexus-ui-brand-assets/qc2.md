---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-02-v1.83-nexus-ui-brand-assets"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: composer-2.5
- Review Perspective: Security and correctness (Git LFS, package exports surface, secrets, SVG safety, dependency footprint, build scripts)
- Report Timestamp: 2026-07-02

## Scope
- **plan_id**: `2026-07-02-v1.83-nexus-ui-brand-assets` (P0)
- **Review range / Diff basis**: `git merge-base iteration/v1.83 HEAD`..`HEAD` on branch `feature/v1.83-nexus-ui-brand-assets`
- **Working branch (verified)**: `feature/v1.83-nexus-ui-brand-assets`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Commit range**: `25e126e1..bce85d6d` (1 commit: `bce85d6d feat(nexus-ui): scaffold @42ch/nexus-ui brand asset package (V1.83 P0)`)
- **Files reviewed**: 17 changed paths (`.gitattributes`, `packages/nexus-ui/**`, `pnpm-lock.yaml`, plan checkbox updates)
- **Tools run**: `git diff` / `git log`, `git lfs ls-files`, `file` / pointer inspection on PNGs, `pnpm install`, `pnpm --filter @42ch/nexus-ui run build`, `pnpm --filter @42ch/nexus-ui run typecheck`, `grep` (SVG XSS patterns, secrets), Node `exports` resolution probe, CI workflow scan
- **Deep review**: not triggered (asset scaffold; no auth/data/concurrency surface)

## P0 Focus Checklist

| Area | Result | Evidence |
|------|--------|----------|
| Git LFS setup | **Pass** | `.gitattributes` tracks `packages/nexus-ui/assets/logos/*.png`; `git lfs ls-files` lists 3 PNGs; local blobs are 2048×2048 PNG (not pointer stubs) |
| Package exports surface | **Pass** | `package.json` `exports` documents root, `./tokens`, `./theme.css`, four SVG subpaths; PNG paths intentionally omitted |
| No accidental secrets | **Pass** | No API keys, tokens, private keys, or credential patterns in package tree |
| SVG safety | **Pass** | No `<script>`, event handlers, `foreignObject`, `javascript:`, or external `xlink:href`; static vector geometry only |
| Dependency footprint | **Pass** | Zero runtime deps; devDeps limited to `tsup` + `typescript` (lockfile adds workspace importer only) |
| Build scripts | **Pass** | `build`/`dev` use `tsup` only; `typecheck` uses `tsc --noEmit`; `clean` is local `rm -rf dist`; no `postinstall`/`preinstall` hooks |

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **CI checkout does not enable Git LFS fetch**: `.github/workflows/ci.yml` and `.github/actions/setup-monorepo/action.yml` use plain `actions/checkout` with no `lfs: true`. CI clones will retain LFS pointer text for PNG provenance files rather than raster bytes.  
  **Impact**: No build/typecheck failure today — PNGs are documented as provenance-only and are not exported or consumed in the build graph. Risk is operational: designer/QA workflows that open PNG references inside CI or fresh clones without `git lfs pull` see pointer files.  
  **Disposition**: Acceptable for P0 scope; recommend P-last add `lfs: true` to checkout or document `git lfs install && git lfs pull` in package README before integration PR. Not a merge blocker.

- **P2 consumer must build `@42ch/nexus-ui` before TS imports**: `exports["."].import.default` resolves to `./dist/index.js`, which is gitignored and produced by `tsup`. Root `pnpm run typecheck` (`pnpm -r`) will exercise this package once linked from `apps/web`, but `apps/web` today has no `@42ch/nexus-ui` dependency (expected — P2 wires consumption).  
  **Disposition**: Expected pattern (mirrors `@42ch/nexus-contracts` prebuild hooks). P2 should add `prebuild`/`pretypecheck` filter build for `@42ch/nexus-ui` when dependency lands. Not a P0 defect.

### 🟢 Suggestion
- Add `prepublishOnly` (`build` + `typecheck`) when npm publish is in scope — `nexus-contracts` already models this; out of scope for V1.83 per plan.
- Add `packages/nexus-ui` to root `AGENTS.md` subdirectory index for discoverability (package-local `AGENTS.md` is present and complete).
- When P2 lands, extend CI `web-build` job to build `@42ch/nexus-ui` before `web typecheck`/`build` (parallel to contracts prebuild).

## Source Trace
- **F-001** (LFS): `.gitattributes:1-2`, `git lfs ls-files` → `9d4452cb`, `72628f54`, `35aa5797`
- **F-002** (exports boundary): `packages/nexus-ui/package.json:15-40`; Node resolve `@42ch/nexus-ui/assets/logos/logo_dark.png` → `MODULE_NOT_FOUND` (undeclared subpath blocked)
- **F-003** (SVG safety): `grep -i` across `packages/nexus-ui` for script/event/foreignObject patterns → no matches; manual read of all four SVGs
- **F-004** (secrets): `grep` for key/secret/password/private-key patterns → only design-token identifiers
- **F-005** (build): `pnpm --filter @42ch/nexus-ui run build` → success; `pnpm --filter @42ch/nexus-ui run typecheck` → success
- **F-006** (CI LFS): `.github/workflows/ci.yml` checkout steps lack `lfs: true`

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (both acceptable for P0; tracked for P-last / P2) |
| 🟢 Suggestion | 3 |

**Verdict**: Approve
