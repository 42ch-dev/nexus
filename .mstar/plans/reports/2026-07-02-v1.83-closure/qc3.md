---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-02-v1.83-closure"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report — V1.83 Iteration Closeout (Performance & Reliability, QC3)

## Reviewer Metadata

- Reviewer: @qc-specialist-3
- Focus: Performance and reliability across integrated P0+P1+P2 on `iteration/v1.83` (bundle/build chain, CI gates, LFS/asset policy, test harness stability, residual risk posture)
- Branch: `iteration/v1.83`
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus`
- Review range / Diff basis: `main`..`HEAD` (`5ed1ab32..85f14b8b`; 14 commits; 51 files, +2700 / −62 lines)
- Integrated HEAD: `85f14b8b` — `chore(v1.83): mark P2 Done after merge and QA Pass`
- Deep review: triggered (multi-plan integration; new package + web consumer chain; cross-surface token/asset coupling)

## Scope

Consolidated closeout review for V1.83 Brand UI Foundation on integrated `iteration/v1.83` HEAD:

| Plan | Status (status.json) | QC3 (plan-level) | QA |
|------|----------------------|------------------|-----|
| P0 `2026-07-02-v1.83-nexus-ui-brand-assets` | Done | Approve | Pass |
| P1 `2026-07-02-v1.83-brand-design-system` | Done | skipped (docs-only) | Pass |
| P2 `2026-07-02-v1.83-web-brand-application` | Done | Approve | Pass |
| P-last `2026-07-02-v1.83-closure` | InProgress | this report | pending |

Out of iteration diff: `schemas/`, `crates/`, `apps/nexus42/` — no changes (brand slice is docs + `packages/nexus-ui` + `apps/web` only).

## Validation Performed (Integrated HEAD)

| Check | Command / method | Result |
|-------|------------------|--------|
| Branch alignment | `git branch --show-current` | `iteration/v1.83` |
| Merge-base vs main | `git merge-base main HEAD` | `5ed1ab327b44e41179c76f7cc2ec608e040b8589` |
| P0 typecheck | `pnpm run typecheck` in `packages/nexus-ui` | Pass |
| P0 build | `pnpm run build` in `packages/nexus-ui` | Pass (tsup CJS+ESM+DTS) |
| P2 typecheck | `pnpm run typecheck` in `apps/web` | Pass (chains contracts + nexus-ui via `pretypecheck`) |
| P2 tests | `pnpm run test` in `apps/web` | Pass — **387/387** (51 files) |
| P2 build | `pnpm run build` in `apps/web` | Pass — main chunk **186.71 kB** (gzip **50.89 kB**) |
| Export map | File existence for all 7 `@42ch/nexus-ui` export targets | All resolve |
| LFS policy | `git lfs ls-files`, `.gitattributes` | 3 PNGs LFS-tracked; 4 SVGs text |
| Public import boundary | Prior QC/QA `rg` + export probe | Web uses only `@42ch/nexus-ui` public paths |
| CI web job | `.github/workflows/ci.yml` `web-build` | typecheck + build + test; nexus-ui built transitively |
| CI Node pin | `.github/actions/setup-monorepo` default | Node **22** (localStorage warning is Node 24+ local-only) |
| Wire / Rust regression | `git diff main..HEAD -- schemas/ crates/` | Empty |

## Findings

### Critical

_None._

### Warning

#### W1 — Git LFS clone ergonomics still undocumented; CI checkout has no `lfs: true`

**Evidence:** `.gitattributes` line 2 tracks `packages/nexus-ui/assets/logos/*.png`. `git lfs ls-files` lists 3 PNGs. No `git lfs` mention in root `CONTRIBUTING.md` or `packages/nexus-ui/README.md`. `.github/workflows/ci.yml` `actions/checkout` steps omit `lfs: true`.

**Impact:** Designers/contributors without LFS get pointer files instead of PNG provenance. **Runtime UI unaffected** — public API and Web consumption use SVG variants only. CI does not validate PNG bytes today.

**Disposition:** Accept for V1.83 ship; register residual for docs + CI LFS when package validation expands. Carried from P0 qc3 W1.

#### W2 — No dedicated CI job for `@42ch/nexus-ui` in isolation

**Evidence:** `web-build` runs `pnpm --filter web typecheck|build|test`, which triggers `pretypecheck`/`prebuild` building `@42ch/nexus-ui`. No path-filtered job runs `pnpm --filter @42ch/nexus-ui run typecheck && build` alone.

**Impact:** Export-map or tsup regressions are caught only when Web pre-hooks run. Acceptable while Web is the sole consumer; gap widens if `nexus-platform` or other surfaces adopt the package without Web changes.

**Disposition:** Accept for closeout; recommend lightweight path-filtered CI step before multi-consumer adoption. Carried from P0 qc3 W3; partially mitigated by P2 pre-hook chain (P2 qc3 S4).

#### W3 — Node 24+ Vitest `localStorage` ExperimentalWarning floods logs locally

**Evidence:** `apps/web/src/test/setup.ts` polyfill restores functional tests (387/387 pass on Node 26.3.1), but workers still emit `ExperimentalWarning: localStorage is not available because --localstorage-file was not provided` per file.

**Impact:** Local/CI-on-Node-24+ log noise; failure triage harder. **CI default Node 22** avoids this in GitHub Actions. No functional regression.

**Disposition:** Accept for ship; document Node pin or test-script flag in `apps/web/AGENTS.md`. Carried from P2 qc3 W1.

#### W4 — One dark canvas fill token retains legacy blue rgba

**Evidence:** `apps/web/src/index.css` L262: `--color-canvas-worldkb-entity-card-fill-selected: rgba(82, 168, 255, 0.14)` in `.dark` while adjacent canvas/SOUL tokens were retinted to brand-cyan in P2.

**Impact:** Visual inconsistency on World KB selected cards in dark mode only; no performance or reliability defect.

**Disposition:** Accept per iteration boundary (shell/primitives, not canvas polish). Carried from P2 qc1 S3 / qc2 W1 / QA informational.

#### W5 — V1.83 carry-forward warnings not yet in `status.json` `residual_findings`

**Evidence:** `grep` for `2026-07-02-v1.83` under `residual_findings` returns no V1.83 plan keys. W1–W4 and token-drift items exist only in plan-level QC/QA reports.

**Impact:** Closeout plan §7 requires structured residuals for accepted leftovers; narrative-only carry-forward risks audit drift.

**Disposition:** **PM closure task** — register accepted low/medium residuals before Profile B compaction. Does not block integrated HEAD technical gates.

### Suggestion

#### S1 — Dual hex source for interactive `blue-*` scale (drift risk)

`--color-brand-*` aliases `--nexus-brand-*` from `@42ch/nexus-ui/theme.css`, but `--color-blue-700`…`1000` remain literal hex in `index.css`. Root brand changes require manual sync in two places. Carried from P2 qc1 S1 / qc3 S3.

#### S2 — Both logo SVG variants inlined in main entry chunk (~3 KB)

Vite embeds `logo-dark.svg` and `logo-color.svg` as `data:image/svg+xml` URLs in `index-RkOVLpeK.js` (~3 KB total raw). Negligible vs 186 KB main chunk; consider single-SVG + CSS or dynamic import if variants multiply. Carried from P2 qc3 S1.

#### S3 — PNG provenance included in npm pack tarball (~97% of dry-run size)

`files: ["assets"]` ships ~600 kB PNG provenance when published; SVGs are the consumer path. Publish explicitly deferred; narrow `files` before first npm release. Carried from P0 qc3 W2.

#### S4 — Hidden `NexusLogo` on desktop still mounts (trivial DOM overhead)

Header renders CSS-hidden logo at `lg+` while sidebar shows visible mark — one extra theme subscriber. Carried from P2 qc3 S2.

## Performance & Reliability Assessment (Integrated)

### Bundle & runtime

| Item | Measurement | Assessment |
|------|-------------|------------|
| Main entry JS | 186.71 kB (gzip 50.89 kB) | No material regression vs pre-P2 baseline |
| Inlined logo SVGs | ~3 KB data URLs in main chunk | Acceptable shell cost |
| `theme.css` | 298 B, 3 custom properties | Negligible |
| Token retints | CSS-variable swaps only | No new JS runtime or network paths |
| Rust/daemon surface | Unchanged in iteration diff | Zero blast radius to CLI/daemon hot paths |

### Build & CI reliability chain

```
apps/web pretypecheck/prebuild
  → @42ch/nexus-contracts build
  → @42ch/nexus-ui build (tsup)
  → web tsc / vite
```

| Path | Behavior | Assessment |
|------|----------|------------|
| Fresh clone + `pnpm --filter web run typecheck` | Self-heals missing `dist/` for both workspace packages | Strong reliability win |
| `@42ch/nexus-ui` `prepare` | Runs `build` on install | Workspace install builds JS entry points |
| CI `web-build` | typecheck + build + test on Node 22 | Exercises full consumer chain |
| P0 standalone CI | None | Gap W2; mitigated while Web-only |

### Test reliability

- **387/387** web tests pass on integrated HEAD.
- Brand-specific: `nexus-logo.test.tsx` (3/3) — theme variant selection.
- No flaky failures observed in this run.
- Logo tests couple to Vite data-URL encoding (maintainability, not runtime) — P2 qc2 W2.

### Asset & LFS policy

| Asset class | Storage | Consumer path | Assessment |
|-------------|---------|---------------|------------|
| PNG provenance | Git LFS (3 files) | Not exported | Correct separation |
| SVG logos | Git text | `@42ch/nexus-ui/assets/logos/*.svg` | ~1.3 KB each; static imports |
| theme.css | Git text | `@42ch/nexus-ui/theme.css` | 298 B |

## Tri-Review Consolidation (QC3 lens)

| Source | QC3 verdict | Key performance/reliability notes |
|--------|-------------|-----------------------------------|
| P0 qc3 | Approve | Sub-kB JS, correct exports; W1 LFS docs, W2 pack bloat, W3 CI gap |
| P2 qc3 | Approve | ~3 KB logos, pre-hook self-heal; W1 test noise, W2 canvas tint |
| Integrated re-run | Pass | All gates green on `iteration/v1.83` HEAD; no new regressions |

P1 (design docs) has no runtime performance surface; QA Pass sufficient for closeout performance posture.

## Residual Risk Summary

| ID | Severity | Topic | Closeout disposition |
|----|----------|-------|----------------------|
| W1 | medium | LFS docs / CI fetch | Accept; PM register residual |
| W2 | low | Dedicated nexus-ui CI job | Accept; before multi-consumer |
| W3 | low | Node 24+ Vitest log noise | Accept; CI on Node 22 |
| W4 | low | Dark canvas legacy blue fill | Accept; canvas polish slice |
| W5 | medium | Structured residuals missing in status.json | PM closure action |
| S1–S4 | low | Token drift, bundle polish, publish pack | Track in roadmap |

No open **Critical** findings. No performance or reliability defect blocks iteration PR readiness.

## Verdict

**Approve** — Integrated `iteration/v1.83` HEAD passes package and web build/typecheck/test gates, adds negligible bundle weight (~3 KB logos, 298 B CSS), and improves clone/CI self-healing via the nexus-ui pre-hook chain. Warnings W1–W5 are documented carry-forward items (LFS ergonomics, CI coverage shape, local test log noise, one dark canvas tint, residual registration pending) that do not block merge or author workflows within the V1.83 scope boundary. PM should register accepted residuals in `status.json` during P-last T5/T6 before Profile B compaction.
