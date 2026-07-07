# QC Consolidated Decision — V1.94 P-last

**plan_id**: `2026-07-06-v1.94-closure`
**Integrated HEAD under review**: `6d002e15` (final — post button-rule correction + full audit + tailwind-merge root cause fix)
**Diff basis**: `merge-base bf0e60cc` (main HEAD pre-V1.94) + `tip 6d002e15`
**Consolidated verdict**: **Approve (3/3)** — confirmed twice (initial + second revalidation after audit)

## Tri-review outcomes

| Reviewer | Initial | Post fix-wave | Post audit (final) | Report |
|----------|---------|---------------|--------------------|--------|
| qc1 — IA + structure lens | Request Changes (5W + 6S) | Approve (`b5ae306b`) | **Approve** (second revalidation `6d002e15`) | [qc1.md](qc1.md) |
| qc2 — security + correctness lens | Approve (0W + 2S) | Approve (unchanged) | Approve (unchanged) | [qc2.md](qc2.md) |
| qc3 — reliability + regression lens | Approve (0W + 0S) | Approve (unchanged) | Approve (unchanged) | [qc3.md](qc3.md) |

## Fix-wave #1 (qc1 W001-W005 → all fixed, commit `4f4b468b`)

| # | Warning | Fix |
|---|---------|-----|
| F-001 | Wizard agent / launch_command not persisted | Tauri command `set_agent_profile` + wizard `finish()` awaits it before `markCompleted()` |
| F-002 | Dead `presets-page.tsx` | Deleted; no remaining imports |
| F-003 | Zero unit tests for new primary surfaces | +5 test files; 470 → 491 tests |
| F-004 | No button contrast snapshot | New `button.test.tsx` (light + dark snapshots) |
| F-005 | Footer switcher missing roving-tabindex | Rewritten with `role="toolbar"` + Arrow/Home/End |

## Fix-wave #2 — user clarification + audit (commits `d47e953d` + `1dd7f002`)

The user clarified that the button contrast rule is **background-driven, not mode-driven** (dark bg → light text; light bg → dark text — independent of light/dark mode). This surfaced two things:

1. **The V1.94 fix-wave's `dark:text-white` on cyan bg was a regression** (light bg + light text = low contrast). Reverted to `dark:text-brand-deep-blue`. V1.83's original rule was actually correct.
2. **A full audit found 4 DESIGN.md token violations + 6 component implementations violating the corrected rule**. All fixed.
3. **The audit discovered the real root cause of the original "primary button not white" complaint**: `tailwind-merge` was silently stripping `text-white` because it didn't recognize `text-button-*` (and `text-heading-*`, `text-label-*`, `text-copy-*`) as font-size tokens, treating them as text-color classes conflicting with `text-white`. Fix: `extendTailwindMerge` registers all custom font-size tokens. **Side effect (positive)**: corrects a latent typography regression across 181 occurrences in `apps/web/src/` where the same bug was stripping standard typography tokens.

## Deferred to V1.95 (10 residuals — registered in status.json)

- F-101..F-106 (qc1 Suggestions): setup-step-agent over-fire; duplicated health-probe subscription; silent Tauri error swallow; browser-fallback path string; /strategy redirect preset ID; sidebar NAV_ITEMS typed guard.
- qc2 S-001/S-002: scan endpoint force_refresh param; JoinSet refactor.
- qc1R-S001: set_agent_profile hardcodes config path.
- qc1 second-revalidation note: `conflict-modal-base.tsx` "Use current" button light-mode contrast on `#e5484d` ~4.0:1 (AA borderline, pre-existing).

## Decision

**Consolidated Approve (3/3), confirmed twice.** The audit + tailwind-merge root cause fix turned a cosmetic-button-defect iteration into one that also closes a latent typography regression. Proceed to iteration-close (Phase 3 §3.3–§3.5) + PR delivery (Phase 4).

## Final verification snapshot (integrated HEAD `6d002e15`)

- `cargo +nightly-2026-06-26 fmt --all --check`: clean.
- `cargo clippy --all -- -D warnings`: green.
- `cargo test --all`: green.
- `pnpm --filter web run test`: 494 pass (69 files).
- `pnpm --filter web run typecheck`: clean.
- `pnpm --filter web run build`: green (3.09s).
- `pnpm run validate-schemas`: 201 valid, 0 invalid.
- tailwind-merge repro: `twMerge('bg-blue-700 text-white text-button-14')` → both preserved.
