# Spec: Logo gallery + lockup scale

**plan_id:** `2026-07-22-v1.131-p1-logo-gallery-lockup`  
**Tracker:** `DF-V1131-LOGO-GALLERY-LOCKUP`  
**Status:** specify+clarify+plan locked (architect Seat 2)

## Problem

Studio Brand logo cards render `primary` / `whiteBg` at ~32px on light gray panels — postage-stamp plates (user fig1). Dark hero lockup wordmark is undersized vs mark (fig2). Plate variants are shown on non-matching panel backgrounds.

## Goals

1. Plate logo cards (`primary`, `whiteBg`): the lockup **fills the card preview width** (no 32px postage-stamp plate surrounded by dead space). Sizing intent is **"1024-class presentation"**: treat 1024 as the design **source** size (the SVG/plate is authored at 1024-class resolution), **not** a literal 1024 CSS px render. Concrete dogfood target: the lockup spans the **full card content width** with `object-contain` so the mark is readable at card scale; the preview panel has **no large dead-space margin** around the plate.
2. Panel backgrounds **match** plate intent (no mid-gray default):
   - `primary` card → `bg-brand-deep-blue` (`#0D2B3E`) or ink-near panel
   - `whiteBg` card → pure white / light paper (`#FFFFFF` or `--color-background` paper), **not** mid-gray
3. Dark hero lockup: increase the **nexus wordmark** size relative to the mark. Concrete dogfood target: wordmark **cap-height ≥ 60% of mark height** (V1.130/PR-#167 state was ~22px wordmark vs 28px mark ≈ 50% including descenders — wordmark read as a caption; user wants it to read as a co-equal partner). No regression on mark geometry.
4. Chronos mini fixtures: logo lives **in** the deep titlebar row, not only in the body row.
5. Close doc nits `R-VI-007…011` when the touched files are edited (JSDoc, comments, dead ternary, design-studio "five variants" checklist).

## Non-goals

- Regenerating PNG provenance assets (unless required for wordmark path fidelity)
- Changing `logoVariants` keys
- New logo geometry / mark redesign

## Acceptance

All AC are dogfood-testable (human pass/fail by looking at the Studio Brand fixture):

- **AC-1 (gallery fill).** `primary` and `whiteBg` cards: lockup spans the full card content width (no postage-stamp plate, no large dead-space margin). Mark readable at card scale.
- **AC-2 (BG match).** `primary` card panel is deep blue / ink-near; `whiteBg` card panel is white / paper. **No mid-gray panel** for either plate.
- **AC-3 (wordmark scale).** Dark hero lockup: wordmark cap-height **≥ 60% of mark height**, visibly larger than the V1.130/PR-#167 state (fig2). Wordmark reads as a co-equal partner, not a caption. No mark geometry regression.
- **AC-4 (mini titlebar).** Chronos mini fixture: logo renders **in** the deep **Chronos titlebar** row, not only in the body row. Dead identical ternary (`R-VI-010`) removed.
- **AC-5 (doc nits).** `R-VI-007…011` are resolved by the touched edits or handed to P3 with explicit evidence; P3 owns residual archival and P1 owns the code/comment edits.

## Surfaces

- `apps/design-studio/src/pages/brand.tsx`
- `packages/nexus-ui` tokens / JSDoc (`nexus-logo.tsx`, `tokens.ts`, `AGENTS.md` if still stale)
- `.mstar/specs/design-studio.md` checklist count
- Root `DESIGN.md` logo usage notes if needed

## Architecture decision (locked)

- This plan changes no runtime logo geometry, `logoVariants` keys, design tokens, or package promotion list. It is a Studio presentation correction plus documentation cleanup.
- `primary` and `whiteBg` are square plate SVGs. Their gallery previews use asset-native `<img>` width-fill rendering (`display: block; width: 100%; height: auto; object-fit: contain`) inside matching deep-blue/white panels. Do not force width-fill through the current `<NexusLogo size>` API: that component intentionally writes a fixed inline height and auto width for runtime marks.
- Fixed-height runtime specimens continue to use `<NexusLogo>`. The dark hero composes the existing transparent `white` timeline mark and `logo-text.svg`; increase the wordmark rendered height until measured glyph bounds are at least 60% of the mark height. No SVG path editing is in scope.
- `ChronosShellMini` owns one deep titlebar row containing both label and logo. Remove the separate body-logo row and the identical dark/light background ternary; light/dark variation applies to text color and surrounding body only.
- Tests may assert structure, classes, and intrinsic asset selection. The 60% cap-height criterion requires a Studio screenshot/human visual measurement because jsdom does not compute SVG painted bounds.

## Validation

- Studio Brand tests: `primary`/`whiteBg` panel classes and width-fill image classes; logo nested inside the titlebar; no dead ternary.
- Visual review at the standard Brand card width in both themes: no dead-space plate, correct background match, and wordmark cap-height ≥60% of mark height.
- Package/docs checks cover only touched JSDoc/comments/checklist counts; no new public export or dependency is introduced.
