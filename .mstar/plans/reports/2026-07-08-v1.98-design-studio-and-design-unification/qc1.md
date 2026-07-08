---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-08-v1.98-design-studio-and-design-unification"
verdict: "Approve"
generated_at: "2026-07-08"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence + maintainability risk (SSOT integrity, boundary integrity, token parity, architecture fit, maintainability, Done Definition)
- Report Timestamp: 2026-07-08T13:30Z

## Scope

- plan_id: `2026-07-08-v1.98-design-studio-and-design-unification`
- Review range / Diff basis: `merge-base: 908de272 (iteration/v1.98 fork point) … tip: c35c3200 (HEAD)`. Equivalent to `git diff 908de272...c35c3200`. **7 commits**, **44 files** (+5113/-2639).
- Working branch (verified): `feature/v1.98-design-studio-and-design-unification`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (root via `git rev-parse --show-toplevel`)
- Files reviewed: 44 (full branch diff)
- Commit range verified: `git log 908de272..c35c3200 --oneline` returns the expected 7-commit impl chain (T1 `55dd06cc` → T2 `c974e4f7` → T3 `979e4ad5` → T4 `780cbcb8` → T5 `8055bf24` → T6 `5c085c7a` → T7 `c35c3200`).
- **Out-of-scope note (precision)**: Working-branch HEAD today is `c2a1706a` (qc2 report commit, +1 beyond `c35c3200`). This review honors the assigned diff basis `908de272...c35c3200`; the qc2 commit (`c2a1706a`) is out of range and not reviewed here. The plan-files-and-reports layout places qc reports under `plans/reports/<plan-id>/` and the peer reviewer (qc2) has independently committed its report there. No boundary violation.
- Tools run: `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD`, `git log … --oneline`, `git diff … --stat`, `git diff …` (targeted hunks), `git ls-files DESIGN*.md`, `git ls-files 'apps/design-studio/*'`, `git grep` (boundary checks), `pnpm --filter design-studio test` (11/11 PASS, 923ms), `pnpm --filter design-studio build` (PASS, 1660 modules, 2.33s), `pnpm --filter web test` (548/548 PASS, 9.68s), `grep`/`rg` (class verification, side-by-side token lookups), CSS bundle inspection.

## Findings

### 🔴 Critical

None.

### 🟡 Warning

**F-QC1-W001 — `bg-gray-alpha-150` is not defined in the shared token preset, so the studio's active-top-nav link has no background highlight.**

- **Trigger**: `apps/design-studio/src/components/nav.tsx:28` uses `'bg-gray-alpha-150 text-gray-1000 font-medium'` for the active `NavLink`. The shared `@nexus/design-tokens/tailwind.preset.ts` (lines 46–53) only registers `gray-alpha.{100, 200, 300, 400, 500, 600}`; **150 is not a defined shade**. Cross-validated against the production CSS bundle (`apps/design-studio/dist/assets/index-*.css`) — the bundle contains `bg-gray-alpha-100`, `bg-gray-alpha-200`, and `bg-gray-alpha-400` (used elsewhere in studio sources) but **no** `bg-gray-alpha-150` rule. Tailwind 3 JIT does not generate rules for shades not in the preset.
- **Impact**: When the user navigates to any gallery section (Tokens / Brand / Components / Voice / Surfaces), the corresponding top-nav link is meant to be visually distinguished as "active" by a background fill. Because the class is a no-op, the active link reads identically to the default state (`text-gray-700`); the `font-medium` weight bump survives but is subtle against `text-gray-1000`. **UX defect**: users lose the primary "where am I?" affordance on persistent chrome. Visible in every page because the top nav is rendered in `App.tsx`. Confidence: **High** (production bundle evidence + source-vs-preset lookup).
- **Fix suggestion** (one-line): replace `'bg-gray-alpha-150 text-gray-1000 font-medium'` with a defined shade consistent with existing chrome — e.g. `'bg-gray-alpha-100 text-gray-1000 font-medium'` (matches `border-gray-alpha-200` chrome used elsewhere in the same component style) or `'bg-gray-alpha-200 …'`. Verify by `pnpm --filter design-studio build` and visually hover-test. **No token or schema drift**; this is a one-line studio-local fix that does not require an iteration re-spec.
- **Why Warning, not Critical**: defect is localized (one chrome selector), no behavioral or data-integrity impact, no schema/wire impact, and no critical-path users are affected (the studio is a non-author dev-only gallery; compass PS-8 boundary still holds). Easy to fix in commit scope.
- **Source Type**: manual-reasoning + production-bundle evidence + design-token-preset cross-check.

### 🟢 Suggestion

**F-QC1-S001 — Surfaces page renders a third fixture (DaemonStatusStrip) beyond the two listed in the IA guide (§4.5) — minor scope creep, not a defect.**

- **Observation**: IA guide §4.5 names only "Setup — Step card" and "App shell chrome" as surface fixtures. `apps/design-studio/src/pages/surfaces.tsx` adds a third "Daemon status strip" sub-section (line ~361). Reading IA guide §4.5 closely, the daemon status strip is referenced *as part of* the App shell chrome fixture ("slim daemon status strip — chrome only"), not as a separate fixture. The studio author pulled it out into its own subsection for visual emphasis — a stylistic choice that improves gallery readability. Not a defect, but worth noting that consumers reading the IA guide as normative inventory will see 3 sections vs 2.
- **Why low**: Compass scope (§4 "Surfaces") doesn't constrain studio-side surface count — only that surfaces compose `@web-ui/*` fixtures without crossing into `components/layout/`. The studio's three subsection structure still respects the boundary (the DaemonStatusStrip fixture is composed from `@web-ui/badge` + inline SVG, no live `NexusClient`).
- **Action**: Optional — if PM wants strict IA-guide parity, fold the strip back into AppShellFixture. Current state is acceptable; record for future iteration.

**F-QC1-S002 — T7 `tsconfig.json` exact-match `@/lib/utils` path resolves a brief ambiguity — keep as-is.**

- **Observation**: The T7 carry-forward `tsconfig.json` uses `@/lib/utils: ["../web/src/lib/utils.ts"]` (exact match) rather than `@/lib/utils/*: ["../web/src/lib/*"]` (wildcard). This avoids the double-nesting bug (`../web/src/lib/lib/utils`) and has been flagged by the L2 reviewer as a correct improvement (see `task-7-review.md` §5). `vite.config.ts` mirrors with alias order `@/lib/utils` before `@`. Verified: `pnpm --filter design-studio build` and `pnpm --filter design-studio test` both pass cleanly with the two-tier resolution.
- **Why recorded**: it is a maintainability improvement; future implementers should preserve the exact-match pattern.

### Notes (not findings — observations recorded for the consolidated report)

- **SSOT integrity (TC-1, TC-2)**: PASS. `git ls-files DESIGN*.md` returns only `DESIGN.md` + `DESIGN.dark.md` at repo root. Web-side consumers (`apps/web/src/index.css`, `apps/web/tailwind.config.ts`, `apps/web/AGENTS.md`) and studio-side consumers (`apps/design-studio/src/index.css`, `apps/design-studio/tailwind.config.ts`) both consume `@nexus/design-tokens/tokens.css` + the shared preset. Neither per-app `tailwind.config.ts` carries a duplicate `theme.extend` block — both end at the same `theme.extend.screens` table (px-based breakpoints only). Compass TC-1 and TC-2 satisfied.
- **Token parity (TC-3, drift register §9)**: PASS. Per T1 review (`task-1-review.md`) and confirmed by side-by-side spot-check:
  - Neutrals `background-{100,200,300}`, `gray-{100…1000}` light values match pre-merge shipped values (e.g. `--color-gray-1000: #111111` light / `#f5f5f5` dark; `--color-background-100: #ffffff` light / `#0a0a0a` dark).
  - Interactive blue-700 light `#1e3a5f` / dark `#25d1e0` preserved verbatim per `design-unification.md` §3.1 (apps/web wins; doc says these alias `brand-deep-blue` and `brand-cyan` respectively). All other accent scales (red/amber/green/teal/purple/pink at 700/800/900/1000) match shipped. Drift register empty.
  - Root-only brand extended tokens (`brand-deep-blue-{800,900,1000}`, `brand-cyan-{800,900,1000}`, alpha variants) preserved per spec §3.1 even though they are not consumed by any CSS var.
- **Boundary integrity (TC-4, TC-5)**: PASS.
  - Studio only imports `@web-ui/*`, `@web-lib/utils`, `@42ch/nexus-ui`, `@nexus/design-tokens`. Searched: zero matches for `apps/web/src/lib/nexus`, `apps/web/src/components/layout`, `apps/web/src/pages` as imports inside `apps/design-studio/src/**` (the few hits inside `surfaces.tsx:403/438/474` are descriptive `<code>` text, not actual code imports).
  - Studio is daemon-independent: no `NexusClient`, no `@42ch/nexus-contracts`, no `nexus42` embed; `apps/nexus42/`, `apps/desktop/src-tauri/`, and root `crates/` carry zero references to `design-studio`. The studio is a `pnpm` workspace member served by its own Vite dev server on port **5174** (IA guide §8.4), distinct from web's `5173`.
- **Architecture fit (compass architecture hierarchy)**: PASS. The diff cleanly implements the post-V1.98 hierarchy `repo DESIGN → @nexus/design-tokens → apps/web + apps/design-studio` with `@42ch/nexus-ui` as a brand-only node (logos, theme.css, no migration of shadcn primitives). The `@web-ui` alias is scoped exactly to `apps/web/src/components/ui/*` and `@web-lib/utils` to `apps/web/src/lib/utils.ts` (transitional coupling per compass §7 boundary table). No imports leak from `tabs.tsx`'s contained barrel gap fix (T1 added `export * from './tabs'` to `apps/web/src/components/ui/index.ts`).
- **Done Definition (product-facing + process)**:
  - **DESIGN SSOT unified** ✓ (only root pair exists; F-1 doc-only references in `CONCEPTS.md`, `apps/desktop/AGENTS.md`, `packages/nexus-ui/README.md`, `packages/nexus-ui/theme.css`, `apps/web/src/components/reading/reading-prose.test.tsx` cleaned up; the single intentional `"former apps/web/DESIGN*.md retired"` historical note in `apps/desktop/AGENTS.md:51` is allowed per `design-unification.md` §8.4).
  - **Studio dev UX** ✓ (`pnpm --filter design-studio dev` works; README + AGENTS.md list commands and dev port).
  - **P0 gallery** ✓ (Tokens, Brand, Components sections match IA guide §4.3/4.1/4.2 with full variant/state matrices).
  - **P1 gallery** ✓ (Voice has 7 writing-pattern specimens from canonical IA guide §4.4 + DESIGN.md §Voice & Content; Surfaces has 3 fixtures per studio-author stylistic choice — see F-QC1-S001).
  - **Contributor loop** ✓ (README documents the 6-step token tuning workflow matching studio spec §4.2).
  - **Product parity** ✓ (`pnpm --filter web test` 548/548 PASS, `pnpm --filter design-studio build` clean, drift register empty).
  - **Boundaries** ✓ (no `nexus42` embed; `wire_contracts_changed: false` — zero files in `schemas/` changed).
  - **Process** (this branch) ✓ (7 SDD task reviews merged; tri-review in flight).
- **Carry-forward cleanups (T7)**:
  - **F-1 (stale `apps/web/DESIGN*` refs)**: **PASS** (`task-7-review.md` §4 — all 6 active-source references fixed; only intentional historical retention in `apps/desktop/AGENTS.md:51`).
  - **T5 M1 (tsconfig)** : **PASS** (exact-match `@/lib/utils` is a correct improvement; builds and tests pass).

## Source Trace

- **F-QC1-W001** (active-nav background missing):
  - Source Type: manual-reasoning + production-bundle evidence
  - Source Reference:
    - `apps/design-studio/src/components/nav.tsx:28` — class string `bg-gray-alpha-150`
    - `tooling/design-tokens/tailwind.preset.ts:46-53` — `gray-alpha: {100, 200, 300, 400, 500, 600}` (no 150)
    - `apps/design-studio/dist/assets/index-vbuikuWy.css` — contains `bg-gray-alpha-100/200/400` but NOT `bg-gray-alpha-150`
    - `git grep "gray-alpha-150"` — single source-code hit (`nav.tsx:28`); zero hits in preset or any other source
  - Confidence: **High**

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 2 |

**Verdict**: **Request Changes** *(see `## Revalidation` below — current verdict after fix dispatch is **Approve**; the body above captures the pre-fix state.)*

The branch cleanly delivers the V1.98 architecture: single root DESIGN SSOT, shared `@nexus/design-tokens` package consumed by both web and studio via the same preset and `tokens.css`, no duplicate `theme.extend`, no schema/wire drift, no daemon coupling, no `nexus42`/`components/layout`/`pages/lib/nexus` leak, all tests green, build green, drift register empty. Architecture coherence, boundary integrity, token parity, and Done Definition coverage all PASS.

The lone Warning (F-QC1-W001) is a one-line studio-local defect: `bg-gray-alpha-150` in `TopNav` resolves to no CSS rule because the color shade is not registered in the shared preset (gray-alpha scale is `{100, 200, 300, 400, 500, 600}`). Production CSS bundle confirms the rule is missing. The active top-nav link therefore lacks the visual highlight users expect when navigating gallery sections. This is a localized chrome defect (no data, security, or wire impact), but per the reviewer verdict rules an unresolved Warning → Request Changes. Fix path is a single string substitution to a defined shade (e.g. `bg-gray-alpha-100` or `bg-gray-alpha-200`) — no token or spec change required, fits within a follow-up mini-commit on this branch before merge.

---

## Revalidation

**Scope of this section**: targeted re-review of F-QC1-W001 after the fix dispatch. Diff basis: `c35c3200..522cc467` on top of the original branch diff `908de272...c35c3200`. Working branch (verified): `feature/v1.98-design-studio-and-design-unification`. Working-tree HEAD (verified): `522cc4673a8e7a3afcb74aa22976a10663f21783`. Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (root via `git rev-parse --show-toplevel`). QC mode: targeted re-review of qc1's own finding (single-seat, this report file only — `## Revalidation` appended, no new file created per `qc-specialist-shared.md` §"Targeted re-review").

**Commits in scope**: `git log c35c3200..522cc467 --oneline` returns 4 commits — `522cc467` (fix), plus the three QC report commits (`718b2f93` qc1, `5e87bb58` qc3, `c2a1706a` qc2 already noted in original review). Only `522cc467` is a source commit; the others touch `plans/reports/...` only (out of QC source-review scope).

### F-QC1-W001 — RESOLVED

**Evidence chain**:

1. **Source fix** — `apps/design-studio/src/components/nav.tsx:28` now reads:

   ```ts
   ? 'bg-gray-alpha-200 text-gray-1000 font-medium'
   ```

   `git show 522cc467 -- apps/design-studio/src/components/nav.tsx` confirms the single-line replacement `bg-gray-alpha-150 → bg-gray-alpha-200`. No other class strings changed. Confidence: **High**.

2. **Shade registration** — `tooling/design-tokens/tailwind.preset.ts:46-53` registers `gray-alpha: { 100, 200, 300, 400, 500, 600 }`. `200` is a registered shade, so Tailwind 3 JIT will emit a rule. Confidence: **High**.

3. **No leftover 150 references** — `git grep -n "gray-alpha-150"` across the working tree (excluding `.md`, `dist/`, `node_modules/`) returns **zero matches**. The orphan shade is fully gone. Confidence: **High**.

4. **Production CSS bundle** — `pnpm --filter design-studio build` ran green (1660 modules, 1.19s, `dist/assets/index-vbuikuWy.css` 33.65 kB). Grepping the bundle for `.bg-gray-alpha-` rules yields:

   ```text
   .bg-gray-alpha-100 { background-color: var(--color-gray-alpha-100) }
   .bg-gray-alpha-200 { background-color: var(--color-gray-alpha-200) }
   .bg-gray-alpha-400 { background-color: var(--color-gray-alpha-400) }
   ```

   `.bg-gray-alpha-200` is now present, which the previously-missing active-nav selector will match. `--color-gray-alpha-200` is defined in both `:root` and `.dark` blocks (`rgba(0,0,0,.06)` / `rgba(255,255,255,.08)`), so light and dark themes both resolve. Confidence: **High**.

5. **Tests** — `pnpm --filter design-studio test` reports `11 passed (11)` in 829 ms (Vitest 3.2.6). No test changes were needed; the existing App smoke suite already exercises the TopNav component indirectly. Confidence: **High**.

### Architecture / maintainability check on the fix commit (qc1 scope guardrail)

The fix commit `522cc467` is titled *"fix(design-studio): QC W001 nav active shade + rAF cleanup (tokens swatches)"* and touches 2 files, 5 insertions, 3 deletions (`apps/design-studio/src/components/nav.tsx` +1/-1; `apps/design-studio/src/pages/tokens.tsx` +4/-2). For the qc1 architecture/maintainability lens:

- **nav.tsx delta**: a single class-string substitution to a token already defined in `tooling/design-tokens/tailwind.preset.ts`. No new patterns, no new dependencies, no import drift, no boundary change. Maintains the "use preset shade verbatim, no inline hex" rule.
- **tokens.tsx delta**: the qc3 rAF cleanup (ColorSwatch + ElevationCard `useEffect`) is **qc3's scope** and I leave the verdict there to qc3. From qc1's lens I only confirm: (a) the cleanup is local to two presentation components and adds no new abstraction or file; (b) the pattern (`const rafId = requestAnimationFrame(...); return () => cancelAnimationFrame(rafId)`) is identical across both components — consistent, no invented helper; (c) no schema/wire/SSOT/theme.controller boundary is touched; (d) the existing `eslint-disable-next-line react-hooks/exhaustive-deps` comment is preserved. **No qc1 architecture/maintainability regression**.

### Updated verdict

| Field | Pre-fix | Post-fix |
|-------|---------|----------|
| 🔴 Critical | 0 | 0 |
| 🟡 Warning (unresolved) | 1 | **0** |
| 🟢 Suggestion | 2 | 2 (carried forward, unchanged) |
| **Verdict** | Request Changes | **Approve** |

**Updated Verdict**: **Approve**

Rationale: F-QC1-W001 is the only blocker on this branch and it has been resolved with surgical precision (one-line class-string fix to a registered shade). Evidence chain complete (source fix + preset registration + zero orphan-150 refs in tree + production CSS rule present + tests green + no architecture/maintainability regression in scope). Two unrelated Suggestions (F-QC1-S001 Surfaces third fixture, F-QC1-S002 tsconfig exact-match alias) remain open as minor maintainability records but neither triggers a Warning per verdict rules (`Approve` requires only Critical = 0 and Warning = 0 unresolved, which is satisfied). Branch is cleared for merge per `qc-specialist-shared.md` §Verdict rules.

### Source Trace (revalidation)

- **F-QC1-W001 (resolved)**:
  - Source Type: source diff + production-bundle re-inspection
  - Source Reference:
    - `git show 522cc467 -- apps/design-studio/src/components/nav.tsx` — one-line substitution
    - `tooling/design-tokens/tailwind.preset.ts:48` — `200: cv('gray-alpha-200')`
    - `apps/design-studio/dist/assets/index-vbuikuWy.css` — `.bg-gray-alpha-200{background-color:var(--color-gray-alpha-200)}` rule present
    - `git grep -n "gray-alpha-150"` (excl. `.md`, `dist`, `node_modules`) — zero matches
    - `pnpm --filter design-studio test` — 11/11 PASS
  - Confidence: **High**
