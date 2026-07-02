---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-02-v1.83-web-brand-application"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report — Performance & Reliability (QC3)

## Reviewer Metadata

- Reviewer: @qc-specialist-3
- Focus: Performance and reliability (bundle size, build/typecheck chain, asset loading, test harness stability, CI coverage)
- Branch: `feature/v1.83-web-brand-application`
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus`
- Review range / Diff basis: `git merge-base iteration/v1.83 HEAD`..`HEAD` (merge-base `e86652d42152451175c4ce4e08b4fa06ecde0a11`; implementation commit `34d0af45`, plus qc1 report commit on branch tip)

## Scope

P2 brand application in `apps/web`: `@42ch/nexus-ui` consumption (`theme.css`, logo SVG exports), `NexusLogo` component, shell/header/sidebar wiring, CSS variable retint, base primitive class updates, test setup polyfill, and `prebuild`/`pretypecheck` nexus-ui build chain.

Deep review: not triggered (focused styling slice; no hot paths, no new async/data flows).

## Validation Performed

| Check | Command / method | Result |
|-------|------------------|--------|
| Branch alignment | `git branch --show-current` | `feature/v1.83-web-brand-application` |
| Typecheck | `pnpm --filter web run typecheck` | Pass (chains contracts + nexus-ui build via `pretypecheck`) |
| Build | `pnpm --filter web run build` | Pass — main chunk `index-*.js` 186.71 kB (gzip 50.89 kB) |
| Tests | `pnpm --filter web run test` | Pass — 387/387 |
| Export map resolution | Node existence check for `@42ch/nexus-ui` exports used by web | All 3 paths resolve (`theme.css`, `logo-color.svg`, `logo-dark.svg`) |
| Bundle logo footprint | Inspect `dist/assets/index-*.js` for inlined SVG data URLs | 2 data URLs × ~1.5 KB raw each (~3 KB total) in main entry chunk |
| Build overhead | Timed `nexus-ui build` + full `web typecheck` | ~1.0 s + ~6.1 s (nexus-ui is small fraction of typecheck wall time) |
| CI web job | Read `.github/workflows/ci.yml` `web-build` job | Runs `web typecheck`, `build`, `test` — nexus-ui built transitively via pre-hooks |

## Findings

### Critical

_None._

### Warning

#### W1 — Node 24 `localStorage` ExperimentalWarning still floods Vitest output

**Evidence:** `src/test/setup.ts` adds a working in-memory polyfill (theme + logo tests pass), but every Vitest worker still prints `ExperimentalWarning: localStorage is not available because --localstorage-file was not provided` on startup (51 files × multiple workers).

**Impact:** CI and local test logs remain noisy; real failures are harder to spot. Functional reliability is restored; observability is not.

**Recommendation:** Document a Node pin or Vitest env flag in `apps/web/AGENTS.md`, or set `NODE_OPTIONS=--no-experimental-webstorage` (or equivalent stable flag when available) in the web test script. Aligns with qc1 S4.

#### W2 — Residual legacy blue in one dark canvas fill token

**Evidence:** `.dark` block in `index.css` retains `--color-canvas-worldkb-entity-card-fill-selected: rgba(82, 168, 255, 0.14)` while adjacent canvas/SOUL tokens were retinted to brand-cyan in this slice.

**Impact:** Selected World KB cards in dark mode use pre-brand tint — visual inconsistency, not a runtime failure. Matches current `DESIGN.dark.md` frontmatter but diverges from the P2 retint pass.

**Recommendation:** Schedule DESIGN.dark + CSS follow-up when canvas polish is in scope (qc1 S3).

### Suggestion

#### S1 — Both logo variants ship in the main entry chunk

**Evidence:** Vite inlines `logo-dark.svg` and `logo-color.svg` as `data:image/svg+xml` URLs in `dist/assets/index-*.js` (~1.5 KB each). Only one is visible per theme.

**Impact:** ~3 KB raw added to the always-loaded main bundle — negligible vs 186 KB main chunk, but avoidable if the mark grows or more variants are added.

**Recommendation:** Accept for V1.83. If variants multiply, consider a single SVG + CSS `currentColor`, dynamic `import()`, or a dedicated small chunk.

#### S2 — Hidden `NexusLogo` instance on desktop still mounts

**Evidence:** `Header` renders `<NexusLogo className="h-7 lg:hidden" />` while `Sidebar` renders a visible logo at `lg+`. Both subscribe to `useTheme` and hold an `<img>` in the DOM (one CSS-hidden).

**Impact:** One extra theme-context consumer and DOM node on desktop — trivial today; worth noting if shell components grow heavier.

**Recommendation:** Optional refactor: render logo in one shell location only, or pass logo as a slot from layout.

#### S3 — Dual hex source for interactive blue scale (drift risk)

**Evidence:** `--color-brand-*` aliases `--nexus-brand-*` from `@42ch/nexus-ui/theme.css`, but `--color-blue-700`…`1000` remain literal hex in both `:root` and `.dark`.

**Impact:** Root brand value changes require manual sync in two places — reliability/maintainance drift, not immediate user-facing failure.

**Recommendation:** Derive `blue-*` ladder from brand vars or document a single codegen/mapping step (qc1 S1).

#### S4 — `pretypecheck`/`prebuild` now build two workspace packages

**Evidence:** `apps/web/package.json` chains `@42ch/nexus-contracts` and `@42ch/nexus-ui` before every typecheck and production build.

**Impact:** Adds ~1 s per nexus-ui build (cached after first run). **Reliability win:** fresh clones and CI self-heal without manual package builds; mirrors qc1 P0 recommendation S1 (now implemented).

**Recommendation:** Keep; no change required. Scoped crate/package iteration remains the daily dev pattern; full web typecheck is pre-merge/CI cadence.

## Performance & Reliability Assessment

### Bundle & asset loading

| Item | Measurement | Assessment |
|------|-------------|------------|
| Main entry JS | 186.71 kB (gzip 50.89 kB) | Unchanged order of magnitude; brand slice does not materially shift chunk budget |
| Inlined logo SVGs | 2 × ~1.5 KB data URLs in main chunk | Acceptable for shell mark; both themes loaded upfront |
| `theme.css` import | 298 B, 3 custom properties | Negligible CSS parse cost |
| Logo `<img>` | `width`/`height` + `decoding="async"` | Good CLS and decode hints |
| Runtime fetches | None — static imports resolved at build | Reliable offline/daemon-served deployment |

Token retints are CSS-variable swaps only — no new JS runtime, no additional network requests, no new React Query paths.

### Build & CI reliability

| Path | Behavior | Assessment |
|------|----------|------------|
| `pnpm --filter web run typecheck` | `pretypecheck` → contracts + nexus-ui build → `tsc` | Deterministic; catches broken export map before merge |
| `pnpm --filter web run build` | Same pre-chain → Vite production build | Pass |
| `pnpm --filter web run dev` | No pre-hook; SVG/CSS resolve to package source via export map | Dev works without pre-built `dist/` for assets used by P2 |
| CI `web-build` job | typecheck + build + test | nexus-ui exercised transitively — P0 qc3 W3 partially mitigated for web consumers |
| `@42ch/nexus-ui` `prepare` | Runs `build` on install | Workspace install builds dist for JS entry points if needed elsewhere |

### Theme toggle & shell reliability

- `NexusLogo` switches `src` on theme change; with inlined data URLs the swap is synchronous (no network round-trip).
- `ThemeProvider` persists preference to `localStorage`; polyfill ensures Vitest reliability on Node 24+ despite stderr noise (W1).
- Public export boundary respected — no relative imports into `packages/nexus-ui` internals.

### Test reliability

- `nexus-logo.test.tsx`: light/dark variant URLs and accessible label — 3/3 pass.
- Full suite: 387/387 pass after polyfill.
- No flaky theme tests observed in this run.

## Verdict

**Approve** — P2 adds negligible bundle weight (~3 KB inlined logos, 298 B CSS), improves build self-healing via the nexus-ui pre-hook chain, and keeps asset loading static and deterministic. Warnings W1–W2 are log noise and one stale dark canvas tint — neither blocks merge or author workflows. Suggestions S1–S3 are polish/drift follow-ups for a later canvas or token-consolidation slice.
