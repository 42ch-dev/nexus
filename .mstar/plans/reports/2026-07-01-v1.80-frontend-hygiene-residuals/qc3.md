---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-01-v1.80-frontend-hygiene-residuals"
verdict: "Approve"
generated_at: "2026-07-01"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-07-01

## Scope
- plan_id: `2026-07-01-v1.80-memory-review-reliability` (P0) AND `2026-07-01-v1.80-frontend-hygiene-residuals` (P1)
- Review range / Diff basis: `merge-base: ed5c6074fdcd66fe71dad922c0c30edc11a6e417 (main) + tip: 0851e2ccbe9982e3661fd2f262698a85e73adcd0 (iteration/v1.80 HEAD)`; equivalent to `git diff ed5c6074...0851e2cc`
- Working branch (verified): `iteration/v1.80`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Verified HEAD: `0851e2ccbe9982e3661fd2f262698a85e73adcd0`
- Files reviewed: P1 frontend hygiene surfaces plus P0 overlap sites: `apps/web/src/components/reading/*`, `apps/web/src/pages/chapter-page.tsx`, `apps/web/src/components/memory/*`, `apps/web/src/pages/memory-page.tsx`, `apps/web/src/components/soul/temporal-drift.tsx`, `apps/web/src/index.css`, `apps/web/DESIGN.md`, `apps/web/DESIGN.dark.md`, and `apps/web/src/api/queries.ts` for cross-plan drain semantics.
- Commit range: `git diff ed5c6074fdcd66fe71dad922c0c30edc11a6e417...0851e2ccbe9982e3661fd2f262698a85e73adcd0`
- Deep review: triggered (S1: 31 files / 2087 insertions / 502 deletions; S6: frontend + Rust/contracts + harness docs; P1 includes UI/design-token changes)
- Lenses applied: Performance Lens, Reliability Lens, Testing Lens, DESIGN-token audit lens
- Tools run:
  - `git rev-parse --show-toplevel`; `git branch --show-current`; `git rev-parse HEAD`; `git merge-base main HEAD`
  - `git diff --stat ed5c6074...0851e2cc`; `git diff --name-only ed5c6074...0851e2cc`; `git diff ed5c6074...0851e2cc`
  - Read both V1.80 plan files; original V1.78 `qc3.md`; P1 reading/SOUL/CSS/DESIGN files; P0 memory client/server files for cross-plan risk
  - `pnpm --filter @42ch/nexus-contracts run build` (pass)
  - `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test memory_review_fragments_api` (pass: 24/24)
  - `pnpm --filter web run test src/api/memory-mutation.test.tsx` (pass: 7/7; React Router future-flag warnings only)
  - Additional P1 confidence check: `pnpm --filter web run test src/components/reading/chapter-keyboard-nav.test.ts src/components/reading/chapter-nav.test.tsx src/pages/chapter-page.test.tsx` (pass: 44/44; React Router future-flag warnings only)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None.

### 🟢 Suggestion
- None.

## P1 Reliability / Performance Review Notes
- Keyboard-nav coverage is strong for the residual scope: `chapter-keyboard-nav.test.ts` covers ArrowLeft/ArrowRight, first/last/no-neighbor behavior, modifier/defaultPrevented guards, input/textarea/contenteditable focus guards, visible menu/dialog overlays, hidden overlays, overlay removal, and URL-encoded work IDs.
- ChapterNav component tests cover href volume propagation, boundary placeholders, loading placeholders, and multi-volume chip visibility/default. This gives good regression confidence for volume switch and first/last chapter boundaries.
- The reading-neighbor hook walks cursor pages only when `hasNextPage` is true and exposes `loading` while walking, avoiding misleading first/last placeholders. That is a bounded and reasonable reliability tradeoff for long works; no hot-path performance issue found for normal-sized works.
- The memory-page extraction reduces the route shell to composition and state coordination. The new section components preserve the original query/subscription patterns and keep P0-owned `useReviewMemory` semantics in `apps/web/src/api/queries.ts`.
- The SOUL drift-band token bridge resolves in both themes: `DESIGN.md` and `DESIGN.dark.md` define `soul-viz-drift-band.fill` through `fill-6`, and `index.css` projects matching `--color-soul-viz-drift-band-fill*` variables in both `:root` and `.dark`. `temporal-drift.tsx` consumes only CSS variables, with no hardcoded RGBA palette values.
- Runtime cost of the CSS-var bridge is negligible: CSS custom properties are static theme tokens and are read by inline SVG/div styles only as paint values; no JS measurement loop or layout-thrashing path was introduced.

## Cross-plan note
- P0 has a separate QC3 warning in `.mstar/plans/reports/2026-07-01-v1.80-memory-review-reliability/qc3.md` about server `processed`/`has_more` semantics on row-level timeout/failure. That warning blocks P0, not this P1 frontend-hygiene plan.

## Source Trace
- P1 test coverage: testing lens — `apps/web/src/components/reading/chapter-keyboard-nav.test.ts`, `chapter-nav.test.tsx`, `chapter-page.test.tsx`; command passed 44/44 — Confidence: High.
- P1 token bridge: DESIGN-token audit / performance lens — `apps/web/DESIGN.md`, `apps/web/DESIGN.dark.md`, `apps/web/src/index.css`, `apps/web/src/components/soul/temporal-drift.tsx` — Confidence: High.
- P1 module split: maintainability/reliability lens — `apps/web/src/pages/memory-page.tsx`, `apps/web/src/components/memory/*` — Confidence: High.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve
