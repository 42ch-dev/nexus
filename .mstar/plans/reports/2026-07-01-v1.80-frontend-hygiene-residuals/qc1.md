---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-01-v1.80-frontend-hygiene-residuals"
verdict: "Approve"
generated_at: "2026-07-01"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence + maintainability risk (Reviewer #1)
- Report Timestamp: 2026-07-01

## Scope
- plan_id: `2026-07-01-v1.80-frontend-hygiene-residuals` (P1)
- Review range / Diff basis: `merge-base: ed5c6074fdcd66fe71dad922c0c30edc11a6e417 (main) + tip: 0851e2ccbe9982e3661fd2f262698a85e73adcd0 (iteration/v1.80 HEAD)`; equivalent to `git diff ed5c6074...0851e2cc`
- Working branch (verified): `iteration/v1.80`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed (P1 subset): `apps/web/src/pages/memory-page.tsx`, `apps/web/src/components/memory/pending-reviews-section.tsx`, `apps/web/src/components/memory/fragments-section.tsx`, `apps/web/src/components/memory/soul-section.tsx`, `apps/web/src/pages/chapter-page.tsx`, `apps/web/src/components/reading/chapter-keyboard-nav.ts`, `apps/web/src/components/reading/chapter-nav.tsx`, `apps/web/src/components/reading/reading-hooks.ts`, `apps/web/src/components/reading/reading-progress.tsx`, `apps/web/src/components/reading/chapter-keyboard-nav.test.ts`, `apps/web/src/components/reading/chapter-nav.test.tsx`, `apps/web/src/components/soul/temporal-drift.tsx`, `apps/web/DESIGN.md`, `apps/web/DESIGN.dark.md`, `apps/web/src/index.css`
- Commit range: `83091c8d..0851e2cc` (P1 merge: `0851e2cc`)
- Tools run: `pnpm --filter @42ch/nexus-contracts run build` (clean), `pnpm --filter web exec tsc --noEmit` (clean), `cargo +nightly-2026-06-26 fmt --all --check` (clean)

### Deep review trigger
- Deep review: **triggered** (≥2 signals)
  - Signal: cross-file module boundary change (memory-page split into 3 components + state lift).
  - Signal: behavioral hook extraction with reusable predicate surface (`useChapterKeyboardNav`, `isEditable`, `hasOpenOverlay`).
  - Signal: design-system token change (new BAND_PALETTE ordered token family, mirrored light/dark).
- Lenses applied: **Module-Boundary Lens** (split seam + lifted-state coordination), **Hook-Extraction Lens** (behavior preservation + role-pattern assumption), **Design-Token Lens** (DESIGN.md ↔ index.css bridge coherence, light/dark parity), **Defensive-Coding Lens** (legitimate vs. unnecessary `??` guards).

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None.

### 🟢 Suggestion

**F-P1-1 — `fragmentKeyword` lifted-state coordination is sound but the lifting has a second consumer pattern.** (Source: deep-lens module-boundary)
- `MemoryPage` lifts `fragmentKeyword` so `SoulSection` → `setFragmentKeyword` → `FragmentsSection` flows unidirectionally. The `onFilterFragments: (kw) => setFragmentKeyword(kw ?? '')` callsite coerces `null` → `''`, which is correct but worth a one-line comment explaining why `SoulPanel` may emit `null` (clear filter) — this would help the next maintainer trace why the coercion is needed. Source: `apps/web/src/pages/memory-page.tsx:55-66`, `apps/web/src/components/memory/soul-section.tsx:18`.
- Severity: `nit`.

**F-P1-2 — `chapter-keyboard-nav.ts` extraction is behavior-preserving but the predicate surface is broader than necessary.** (Source: deep-lens hook-extraction)
- The new module exports `useChapterKeyboardNav`, `isEditable`, and `hasOpenOverlay`. Both `isEditable` and `hasOpenOverlay` are reasonable to export (the role-pattern assumption is documented; tests cover both predicates). However, only `useChapterKeyboardNav` is currently consumed by `chapter-page.tsx`. The two predicates are exported for the test file to assert the role-pattern assumption directly. That's a defensible design — flag only as something to revisit if/when a second consumer appears, at which point the role-pattern guard could be generalized into a shared "overlay detection" hook in `src/lib/dom/`.
- Severity: `nit`.

**F-P1-3 — `hasOpenOverlay` role-pattern assumption is documented in code AND locked by test.** (Source: deep-lens hook-extraction)
- Good — the JSDoc explicitly says "if a future overlay uses `role="alertdialog"` or signals visibility by class/ARIA state, extend the selector set here". `chapter-keyboard-nav.test.ts` covers the `[role=menu]:not([hidden])` and `[role=dialog]:not([hidden])` paths. The comment is on the predicate itself (not buried in a CHANGELOG), so the next maintainer reads it before adding a new overlay role. This is the right shape for a role-pattern guard. No action needed.

**F-P1-4 — DESIGN.md token structure is coherent with the existing token family.** (Source: deep-lens design-token)
- `soul-viz-drift-band.fill` was already present (slot 0); `fill-2`..`fill-6` extend it as slots 1..5. The slot numbering is consistent with the rest of the design system (slot 0 = bare name, slots 1..N = `-N` suffix). The CSS bridge in `src/index.css` adds `--color-soul-viz-drift-band-fill-2`..`-6` for both `:root` (light) and the dark mode block. Light and dark values differ but share the same token names — correct per `mstar-design-md` light/dark dual-theme rules.
- The `temporal-drift.tsx` BAND_PALETTE consumes them only via `var(...)`, no RGBA in component code. The chart caps at `BAND_PALETTE.length = 6` and the comment notes that adding a 7th band requires extending the token family + the CSS bridge. Good.
- Severity: `nit` (note: keep an eye on the catalog count if the SOUL team ever wants more bands — current design accommodates 6).

**F-P1-5 — `?? 1` removal is sound: `volume` is contract-guaranteed.** (Source: deep-lens defensive-coding)
- `chapter-summary.schema.json` and `chapter-detail.schema.json` (and `chapter-body`, `chapter-outline`) all declare `volume: { "type": "integer", "minimum": 1 }` and list it in `required`. The generated TS interfaces (`ChapterSummary`, `ChapterDetail`) reflect this with `volume: number`. Removing the defensive `?? 1` in `chapter-nav.tsx`, `chapter-page.tsx`, `reading-hooks.ts` is correct and matches the plan (T2 / R-V179P0-QC1-002).
- The legitimate optional-prop guards remain in `temporal-drift.tsx` (e.g., `b.newCount > 0 ? '4px' : '0'`, `b.keywords.slice(0, BAND_PALETTE.length)`) — those are guarding computed values, not contract fields.
- Severity: none — this is a positive note (clean removal), recorded here for the maintainability trail.

**F-P1-6 — Dead alias `driftDateHelper` cleanly removed.** (Source: deep-lens module-boundary)
- The alias `export const driftDateHelper = formatDate` in `temporal-drift.tsx` was a test convenience re-export. Verified no other in-tree consumer (`grep -r 'driftDateHelper' apps/web/src` → 0 hits). Removed cleanly with the unused `formatDate` import dropped in the same edit. Good.
- Severity: none.

**F-P1-7 — `pending-reviews-section.tsx` is 214 lines — within discipline but the largest of the new siblings.** (Source: deep-lens module-boundary)
- 214 lines is comfortably under the ≤250-line discipline; the file is dense but cohesive (one section: the table + count badge + refresh/review CTAs + inspector). The original 360-line `memory-page.tsx` was split into three focused siblings (33 + 77 + 214 = 324 lines total, but each <250 and focused). Splitting the inspector into its own `pending-review-inspector.tsx` would push this further but is YAGNI for now — the inspector is bound to the table's `selectedId` state and benefits from the co-location.
- Severity: `nit` (no action recommended; recorded for future maintainability scoping).

## Source Trace

| Finding | Source Type | Source Reference | Confidence |
|---|---|---|---|
| F-P1-1 | manual-reasoning + deep-lens module-boundary | `apps/web/src/pages/memory-page.tsx:55-66`, `apps/web/src/components/memory/soul-section.tsx:18` | High |
| F-P1-2 | manual-reasoning + deep-lens hook-extraction | `apps/web/src/components/reading/chapter-keyboard-nav.ts:60-93` | Medium |
| F-P1-3 | manual-reasoning + test coverage | `apps/web/src/components/reading/chapter-keyboard-nav.ts:78-92`, `apps/web/src/components/reading/chapter-keyboard-nav.test.ts` | High |
| F-P1-4 | manual-reasoning + deep-lens design-token | `apps/web/DESIGN.md:301-321`, `apps/web/DESIGN.dark.md:290-307`, `apps/web/src/index.css:138-148, 278-289`, `apps/web/src/components/soul/temporal-drift.tsx:28-35` | High |
| F-P1-5 | manual-reasoning + deep-lens defensive-coding | `apps/web/src/components/reading/chapter-nav.tsx:40-44`, `apps/web/src/components/reading/chapter-page.tsx:68-69`, `apps/web/src/components/reading/reading-hooks.ts:48-52, 113-117`, `schemas/local-api/works/chapters/chapter-summary.schema.json:8-12`, `chapter-detail.schema.json:8-12` | High |
| F-P1-6 | grep verification | `apps/web/src/components/soul/temporal-drift.tsx:1,142` (removed), `grep driftDateHelper apps/web/src` (0 hits) | High |
| F-P1-7 | manual-reasoning + deep-lens module-boundary | `apps/web/src/components/memory/pending-reviews-section.tsx` (214 lines), `apps/web/src/pages/memory-page.tsx` (71 lines) | High |

## Architecture/Maintainability Lens — Pass Notes

- **Memory-page split (R-V179P1-QC1-001).** Three focused siblings (`pending-reviews-section.tsx` 214, `fragments-section.tsx` 77, `soul-section.tsx` 33) replace the prior 360-line god-module. The page shell is now a clean 71 lines that owns active-creator lookup, lifted `fragmentKeyword` state coordination, Card layout, and section composition. Each section has a single responsibility and a clear data contract (creator_id + optional callbacks). Page shell is under the ≤250-line discipline; the largest section (214) is dense but cohesive. The comment block at the top of `memory-page.tsx` (lines 27-33) explicitly documents the seam and references P0's ownership of `useReviewMemory` — good handoff prose for the next maintainer.
- **Keyboard-nav extraction (R-V179P0-QC1-001).** `useChapterKeyboardNav` + `isEditable` + `hasOpenOverlay` are extracted verbatim from `chapter-page.tsx`. The hook signature is unchanged, the predicates are pure functions, the role-pattern assumption is documented inline and locked by tests in `chapter-keyboard-nav.test.ts`. `chapter-page.tsx` just imports and calls the hook — minimal call-site change.
- **DESIGN.md token structure (R-V179P1-QC1-002).** The `soul-viz-drift-band` family is extended with `fill-2`..`fill-6` slots. The naming convention matches existing token patterns in the file (slot 0 = bare name, slots 1..N = `-N` suffix). Light + dark values are tuned independently (alpha 0.16 light, 0.22 dark). The CSS bridge in `src/index.css` adds the 5 new vars for both `:root` and the dark block. Component code in `temporal-drift.tsx` consumes them only via `var(...)`, no hardcoded RGBA. The chart caps at the palette length (6), and the comment notes the extension path.
- **`?? 1` removal (R-V179P0-QC1-002).** Volume is contract-guaranteed (`type: integer, minimum: 1`, in `required` for `ChapterSummary`/`ChapterDetail`/`ChapterBody`/`ChapterOutline`). The defensive fallback was unnecessary and is correctly removed in `chapter-nav.tsx`, `chapter-page.tsx`, and `reading-hooks.ts`. Legitimate guards on computed values (e.g., `b.newCount > 0 ? '4px' : '0'`) remain.
- **Dead alias removal.** `driftDateHelper` was the only test convenience re-export; verified no in-tree consumer via grep; removed cleanly with the unused `formatDate` import.
- **Tests.** Two new test files (`chapter-keyboard-nav.test.ts`, `chapter-nav.test.tsx`) plus the existing `memory-mutation.test.tsx` extensions cover the new behavior. The test files use the same `renderHook` + `fireEvent.keyDown(document.body, ...)` pattern, which is the right tool for a `window.addEventListener('keydown', ...)` hook. No test smells spotted.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 7 |

**Verdict**: Approve