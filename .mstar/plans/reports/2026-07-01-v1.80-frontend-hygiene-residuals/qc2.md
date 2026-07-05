---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-01-v1.80-frontend-hygiene-residuals"
verdict: "Approve"
generated_at: "2026-07-01"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk (with emphasis on contract fidelity, behavior preservation, prop drilling, and defensive-code removal)
- Report Timestamp: 2026-07-01

## Scope
- **plan_id**: `2026-07-01-v1.80-frontend-hygiene-residuals` (P1)
- **Review range / Diff basis**: `merge-base: ed5c6074fdcd66fe71dad922c0c30edc11a6e417 (main) + tip: 0851e2ccbe9982e3661fd2f262698a85e73adcd0 (iteration/v1.80 HEAD)`; equivalent to `git diff ed5c6074...0851e2cc`
- **Working branch (verified)**: `iteration/v1.80`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: `apps/web/src/pages/chapter-page.tsx`, `apps/web/src/components/reading/chapter-keyboard-nav.ts` + `.test.ts`, `apps/web/src/components/reading/chapter-nav.tsx`, `apps/web/src/pages/memory-page.tsx`, `apps/web/src/components/memory/{pending-reviews-section.tsx,fragments-section.tsx,soul-section.tsx}`, schemas for ChapterSummary/ChapterDetail, generated contracts (spot-check), DESIGN.md token additions for BAND_PALETTE.
- **Tools run**: `git diff`, targeted schema reads, component diff vs prior inline behavior, prop audit on extracted sections.
- **Lenses applied**: Correctness of `?? 1` removal (contract vs runtime), behavior-preservation of extracted keyboard nav, completeness of memory-page extraction (no dropped props/callbacks), token hygiene.

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- The `currentVolume ?? 1` guard that remains in `chapter-nav.tsx:87-89` (display label) and the passing of `ch.volume ?? currentVolume` in chapter-page.tsx:97 are now the only legitimate uses. Because `ChapterSummary.volume` and `ChapterDetail.volume` are both `required` + `minimum:1` in the schemas (and generated types), any future path that could legitimately receive `undefined` for volume would be a contract violation, not a UI bug. Good that the defensive `?? 1` was removed from the data-flow side.
- Keyboard-nav extraction comment correctly documents the role-pattern assumption (`[role=menu]:not([hidden])` etc.). If new overlay primitives adopt different roles, the `hasOpenOverlay` helper will need an update — the comment makes this explicit (addresses R-V179P0-QC1-001).
- Memory-page split is clean: the page shell is now ~70 lines and only owns active-creator lookup, the lifted `fragmentKeyword` state (for SOUL viz → fragments cross-filter), Card chrome, and composition. All three sections receive exactly the props they declare. No dropped callbacks or missing data observed. P0's `useReviewMemory` drain logic correctly stayed in `queries.ts`.

## Source Trace
- Volume required-ness: `schemas/local-api/works/chapters/chapter-summary.schema.json:8,12` and `chapter-detail.schema.json:8,12` (both list `volume` in `required` and declare `"minimum": 1`).
- Keyboard nav extraction: `chapter-keyboard-nav.ts` vs prior inline in chapter-page.tsx (keys, guards, `useEffect` deps, and `hasOpenOverlay` query are identical; only moved + documented).
- Memory split: `memory-page.tsx` (shell) imports and renders `PendingReviewsSection`, `FragmentsSection`, `SoulSection`; each section file declares its exact props (creatorId + keyword/onChange where needed). No prop loss.
- BAND_PALETTE token promotion: DESIGN.md + DESIGN.dark.md updated; `temporal-drift.tsx` now consumes `var()` (verified in diff).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 (all hygiene / future-proofing notes) |

**Verdict**: Approve

---

## Completion Report v2

**Agent**: qc-specialist-2  
**Task**: Security + correctness review (contract fidelity for volume removal, behavior preservation of keyboard-nav extraction, completeness of memory-page component split, token hygiene) for P1 `2026-07-01-v1.80-frontend-hygiene-residuals`  
**Status**: Done  
**Scope Delivered**: Reviewed all four residuals (R-V179P0-QC1-001/002, R-V179P1-QC1-001/002). Verified schemas, diffed extraction, audited props, confirmed `?? 1` removal is safe because volume is contract-required.  
**Artifacts**: This `qc2.md` (both plan report dirs), commit below.  
**Validation**: 
- Schema inspection: volume is in `required` + `minimum:1` for both ChapterSummary and ChapterDetail.
- Behavior diff: `useChapterKeyboardNav` is a faithful move (same guards, same keys, documented assumption).
- Extraction audit: memory-page is now a thin shell; sections receive precisely the props they need; no broken callbacks.
- Tests: existing chapter keyboard nav tests + memory mutation tests remain green (no new breakage introduced).
**Issues/Risks**: None blocking. The remaining `?? 1` in the nav label is the correct defensive display case (the data prop `ch.volume` is now trusted).  
**Plan Update**: None required from QC.  
**Handoff**: Future overlay role changes should update `hasOpenOverlay` per the comment already present.  
**Git**: (see final commit command output)
