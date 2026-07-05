---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-02-v1.83-nexus-ui-brand-assets"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report — Performance & Reliability (QC3)

## Reviewer Metadata

- Reviewer: @qc-specialist-3
- Focus: Performance and reliability (bundle size, export map, build/typecheck, asset loading, LFS clone ergonomics)
- Branch: `feature/v1.83-nexus-ui-brand-assets`
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus`
- Review range / Diff basis: `git merge-base iteration/v1.83 HEAD`..`HEAD` (merge-base `25e126e124b442696dfed1161fc9abbaefe0cea5`, 1 commit `bce85d6d`)

## Scope

P0 scaffold of `@42ch/nexus-ui`: package metadata, Git LFS for PNG provenance, canonical SVG logo family, token/theme exports, README/AGENTS. No `apps/web` consumer wiring in this diff (deferred to P2).

Deep review: not triggered (single-package scaffold, no hot paths or multi-module coupling).

## Validation Performed

| Check | Command / method | Result |
|-------|------------------|--------|
| Branch alignment | `git branch --show-current` | `feature/v1.83-nexus-ui-brand-assets` |
| Typecheck | `pnpm --filter @42ch/nexus-ui run typecheck` | Pass |
| Build | `pnpm --filter @42ch/nexus-ui run build` | Pass (tsup CJS+ESM+DTS) |
| Export map resolution | Node path existence for all `exports` targets | All 7 entries resolve |
| Bundle / asset sizes | `wc -c`, `npm pack --dry-run` | See findings |
| LFS tracking | `git lfs ls-files`, `git show HEAD:…/logo_dark.png` | 3 PNGs LFS-tracked; git stores 131-byte pointers |
| Import without dist | `rm -rf dist && node -e "import('@42ch/nexus-ui')"` | Fails until `build` (expected for tsup dist pattern) |

## Findings

### Critical

_None._

### Warning

#### W1 — LFS clone ergonomics undocumented; CI does not fetch LFS objects

**Evidence:** `.gitattributes` correctly tracks `packages/nexus-ui/assets/logos/*.png`. Git objects are LFS pointers (~131 B). `git lfs ls-files` confirms three PNGs. No `lfs: true` on `actions/checkout` in `.github/workflows/*.yml`. No `git lfs` mention in root `CONTRIBUTING.md`.

**Impact:** Contributors or CI runners without Git LFS installed receive pointer files instead of PNG provenance references. **Product UI is unaffected** — README and exports steer consumers to SVG variants (~1.3 KB each). Designer/provenance workflows break silently on clone.

**Recommendation:** Add a short LFS prerequisite to `packages/nexus-ui/README.md` or root contributor docs (`git lfs install && git lfs pull`). When CI starts validating this package, enable `lfs: true` on checkout for jobs touching `packages/nexus-ui/**`.

#### W2 — `files` field ships PNG provenance in npm tarball (~97% of pack size)

**Evidence:** `npm pack --dry-run` reports package size **589.8 kB** (unpacked 622 kB). PNGs account for ~603 kB; JS dist totals ~6.6 kB; SVGs ~5 kB. `files` includes `"assets"` wholesale; PNGs are excluded from `exports` (correct) but still packed.

**Impact:** No immediate runtime impact — V1.83 defers npm publish and README directs SVG consumption. Future publish would ship ~600 kB of non-consumer PNG provenance unless `files` is narrowed (e.g. `assets/logos/*.svg` only) or PNGs are repo-only via `.npmignore`.

**Recommendation:** Document publish-time exclusion intent in AGENTS.md or add a follow-up before first npm release.

#### W3 — No CI gate for `@42ch/nexus-ui` build/typecheck

**Evidence:** `.github/workflows/ci.yml` builds `@42ch/nexus-contracts` for web jobs but has no path filter or job for `packages/nexus-ui/**`. Root `pnpm -r run build` would build nexus-ui, but CI does not invoke it today.

**Impact:** Regressions in export map, tsup config, or TypeScript could merge undetected until P2 wires `apps/web` or manual verification.

**Recommendation:** Add a lightweight CI step (or path-filtered job) running `pnpm --filter @42ch/nexus-ui run typecheck && build` when `packages/nexus-ui/**` changes.

### Suggestion

#### S1 — Mirror `nexus-contracts` consumer build contract before P2

`@42ch/nexus-contracts` uses `prebuild` / `pretypecheck` in `apps/web` so fresh clones self-heal. `apps/web` does not yet depend on `@42ch/nexus-ui`; when P2 adds the dependency, add the same lifecycle hooks so `pnpm --filter web run typecheck` does not fail on missing `dist/`.

#### S2 — Add `prepublishOnly` when npm publish is in scope

`packages/nexus-contracts` runs `build && typecheck` on publish. `@42ch/nexus-ui` lacks this guard — low risk while publish is explicitly out of scope.

#### S3 — README `ReactComponent` SVG example may mislead P2 integrators

README shows `import { ReactComponent as NexusMark } from '…logo-mono.svg'` without SVGR wiring. Vite consumers typically need `?react` or raw SVG imports. Clarify in P2 or add a one-line caveat to avoid failed builds.

## Performance & Reliability Assessment

### Bundle size (web consumer)

| Asset / module | Size | Assessment |
|----------------|------|------------|
| ESM `index.js` + shared chunk | 191 B + 668 B | Excellent — tree-shakeable constants only |
| ESM `tokens.js` (direct entry) | 191 B + shared chunk | Same shared chunk; no meaningful duplication risk |
| Each SVG logo | ~1.27 KB | Excellent for inline or URL import |
| `theme.css` | 298 B | Negligible |
| PNG provenance (not exported) | ~608 KB on disk / in pack | Out of runtime path; pack bloat only (W2) |

Token imports add **zero runtime dependencies**. SVG asset imports are static URLs — no JS bundle inflation beyond the imported module graph.

### Export map correctness

All seven documented `exports` entries resolve to existing files. Conditional `import`/`require` + `types` mirrors `nexus-contracts` dual-format pattern. Asset subpaths (`./theme.css`, `./assets/logos/*.svg`) are string mappings appropriate for bundler URL resolution. PNGs correctly **omitted** from public exports.

### Build / typecheck reliability

`typecheck` and `build` pass cleanly. `dist/` is gitignored (repo root `.gitignore`); consumers must run `build` before importing JS entry points — same pattern as `@42ch/nexus-contracts`. Verified: import fails without `dist/`, succeeds after `pnpm --filter @42ch/nexus-ui run build`.

### Asset loading implications for web consumer (P2)

- **SVG URL imports** (`import logo from '@42ch/nexus-ui/assets/logos/logo-dark.svg'`) — supported via export map; Vite/webpack resolve to static assets (~1.3 KB each).
- **CSS** (`@import '@42ch/nexus-ui/theme.css'`) — supported; 3 custom properties, no cascade risk.
- **Token constants** — compile-time constants; no network or async loading.
- **No side effects** in package.json — safe for bundler dead-code elimination.

## Verdict

**Approve** — P0 performance profile is strong (sub-kilobyte JS, sub-2 KB SVGs, zero runtime deps). Export map is correct and complete for the documented public API. Build/typecheck are reliable at package scope. Warnings W1–W3 are forward-looking reliability gaps (LFS docs/CI, publish tarball shape, CI coverage) that do not block P0 acceptance or P2 SVG/token consumption; track as residuals or P2/closure follow-ups.
