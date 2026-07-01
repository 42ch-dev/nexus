---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-01-v1.79-manuscript-reading-surface"
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
- plan_id: 2026-07-01-v1.79-manuscript-reading-surface
- Review range / Diff basis: merge-base: 0015694f (origin/main) .. tip: 37d19d51 (HEAD) — `git diff 0015694f...HEAD`
- Working branch (verified): iteration/v1.79
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Verified HEAD: 37d19d51
- Files reviewed: 6 P0 focus files (`apps/web/src/components/reading/*`, `apps/web/src/pages/chapter-page.tsx`) plus supporting query/client/handler/test context
- Commit range: `0015694f...HEAD` (same as assigned diff basis)
- Deep review: triggered (S1: focused P0 diff is >200 lines; S3: new reading-surface component/module area)
- Lenses applied: Performance Lens, Reliability Lens, Standards Lens, Testing Lens
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse --short HEAD && git rev-parse HEAD && git merge-base origin/main HEAD && git diff --stat 0015694f...HEAD && git status --short && git log --oneline -10`
  - `git diff 0015694f...HEAD -- apps/web/src/components/reading apps/web/src/pages/chapter-page.tsx`
  - GitNexus query/context for `useChapters`, `useFindings`, `useWorldKbGraph`
  - `pnpm --filter @42ch/nexus-contracts run build`
  - `pnpm --filter web typecheck`
  - `pnpm --filter web test -- src/pages/chapter-page.test.tsx`

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- [W-QC3-001] Chapter navigation is bounded to the first chapter page and can silently disable prev/next for long manuscripts.
  - Evidence: `useChapterNeighbors` requests one broad chapter page with `NEIGHBOR_PAGE_LIMIT = 200`, flattens only mounted pages, then returns `prev: null, next: null` when the current chapter is not found (`apps/web/src/components/reading/reading-hooks.ts:20-21`, `56-73`). The daemon handler clamps chapter-list `limit` to `1..=100` (`crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:349-353`), so this is effectively a first-100-chapters lookup, not first 200. For chapter 101+ (or any later page), `idx === -1` and the UI renders first/last placeholders plus disables keyboard navigation even though adjacent chapters exist (`apps/web/src/components/reading/chapter-nav.tsx:56-97`, `apps/web/src/pages/chapter-page.tsx:144-149`).
  - Impact: long or multi-volume manuscripts degrade from a reading flow into isolated pages after the first server page. The degradation is silent and user-facing: navigation controls imply no adjacent chapter instead of loading more or showing a bounded-state affordance.
  - Required fix: make neighbor resolution pagination-aware. Minimum durable options: cursor-walk `useChapters`/`fetchNextPage` until the current chapter and one following row are loaded; or add/read a dedicated adjacent-neighbor read endpoint in a later plan. For this P0, do not display `First chapter`/`Last chapter` when `pagination.has_more` means the current chapter may simply be outside the loaded page. Add a regression test with >100 chapters (or mocked `has_more: true`) proving a later chapter does not falsely lose prev/next.
  - Source Type: deep-lens: Performance Lens + Reliability Lens
  - Confidence: High

- [W-QC3-002] Open-findings count is first-page-only, so the maturation badge can undercount high-volume chapters.
  - Evidence: `useOpenFindingsCount` calls `useFindings(..., { status: OPEN_FINDING_STATUSES, chapter, limit: 200 })`, flattens the currently loaded infinite-query pages, and returns `rows.length` (`apps/web/src/components/reading/reading-hooks.ts:87-98`). It never reads `pagination.has_more` or fetches subsequent pages. The plan explicitly allows existing `listFindings` but says to paginate if exact counts exceed one page (`.mstar/plans/2026-07-01-v1.79-manuscript-reading-surface.md:30-33`).
  - Impact: the “N open findings” indicator can present an exact-looking but truncated count when a chapter has more than the mounted page size of non-terminal findings. This is less common than W-QC3-001, but it affects the trustworthiness of the maturation signal.
  - Required fix: either fetch all pages needed for an exact count, or deliberately render a bounded label such as `200+ open findings` when `has_more` is true. Add a regression test for a paginated findings response with `has_more: true`.
  - Source Type: deep-lens: Reliability Lens + Testing Lens
  - Confidence: High

### 🟢 Suggestion
- [S-QC3-001] Session-only progress correctly avoids `localStorage`/DB writes, but explicit in-memory scroll restoration is not implemented.
  - Evidence: focused grep of `apps/web/src/components/reading` found no `localStorage`, `sessionStorage`, IndexedDB, mutation, fetch, or invoke write path beyond comments. `ReadingProgress` only keeps `pct` in component state and cleans up `scroll`/`resize` listeners on unmount (`apps/web/src/components/reading/reading-progress.tsx:22-54`). The plan's user story mentions preserving the position within the session (`.mstar/plans/2026-07-01-v1.79-manuscript-reading-surface.md:43-47`), while the implementation only displays current progress and resets the progress component by chapter key (`apps/web/src/pages/chapter-page.tsx:66-88`).
  - Recommendation: if PM interprets the user story as actual same-session restoration, add a route/chapter-scoped in-memory map (not `localStorage`/DB) and tests. If PM intended only a live progress indicator, update the plan language to avoid over-promising “pick up where I left off”.
  - Source Type: manual-reasoning
  - Confidence: Medium

## Additional Checks
- Query-key invalidation breadth appears acceptable for the reused hooks:
  - `useUpdateFinding` invalidates the work-scoped findings list prefix, which covers the chapter/status count query.
  - World KB mutations invalidate the graph prefix used by `useWorldKbGraph`.
  - Chapter-list cache keys are separate by `limit`, so the reading page does not mutate or corrupt the chapters-page cache.
- Re-render hot path: `ReadingProgress` owns its own state, so scroll updates should not re-render `ReadingProse`; the scroll listener is passive, and cleanup cancels pending RAF work.
- Resource lifecycle: `ReadingProgress` removes `scroll`/`resize`; `useChapterKeyboardNav` removes `keydown`; context-menu listener cleanup is covered by existing tests.
- Sparse/empty states: zero open findings and zero KB counts are rendered visibly; no-World returns `— key blocks` while loading/no binding, which is acceptable as a sparse-data fallback.
- Read-only invariant: no new reading-surface mutation/write route or client write method was introduced in the focused files; the only edit affordance routes back to Canvas.

## Source Trace
- Finding ID: W-QC3-001
  - Source Type: deep-lens: Performance Lens + Reliability Lens
  - Source Reference: `apps/web/src/components/reading/reading-hooks.ts:20-21,56-73`; `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:349-353`; `apps/web/src/components/reading/chapter-nav.tsx:56-97`; `apps/web/src/pages/chapter-page.tsx:144-149`
  - Confidence: High
- Finding ID: W-QC3-002
  - Source Type: deep-lens: Reliability Lens + Testing Lens
  - Source Reference: `apps/web/src/components/reading/reading-hooks.ts:87-98`; `.mstar/plans/2026-07-01-v1.79-manuscript-reading-surface.md:30-33`
  - Confidence: High
- Finding ID: S-QC3-001
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/components/reading/reading-progress.tsx:22-54`; `apps/web/src/pages/chapter-page.tsx:66-88`; `.mstar/plans/2026-07-01-v1.79-manuscript-reading-surface.md:43-47`
  - Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Revalidation

### Revalidation Scope
- Targeted re-review of QC3 findings: W-QC3-001 and W-QC3-002.
- Review range / Diff basis: merge-base: 0015694f (origin/main) .. tip: 0d69b3c0 (HEAD)
- Working branch (verified): iteration/v1.79
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Fix-wave commit reviewed: 3d33688e (`fix(v1.79): P0 QC fix-wave — honest open-findings count (N+ when truncated) + chapter-nav cursor-walk across server pages`)
- Files re-reviewed: `apps/web/src/components/reading/reading-hooks.ts`, `apps/web/src/components/reading/maturation-indicators.tsx`, `apps/web/src/components/reading/chapter-nav.tsx`, `apps/web/src/pages/chapter-page.tsx`, `apps/web/src/pages/chapter-page.test.tsx`; supporting checks against generated `PaginationInfo`, `useChapters` pagination, and daemon chapter/findings handlers.

### Evidence
- `git show 3d33688e` reviewed the P0 fix-wave diff and confirmed the intended changed files.
- `packages/nexus-contracts/src/generated/local-api/kb/PaginationInfo.ts` confirms the envelope is `{ limit, next_cursor?, has_more }` with no `total` field.
- `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:349-384` confirms chapter list requests are clamped to `[1, 100]` and expose `next_cursor` / `has_more`.
- `pnpm --filter @42ch/nexus-contracts run build` passed.
- `pnpm --filter web run test -- src/pages/chapter-page.test.tsx` passed (`44 passed` files / `323 passed` tests; targeted `src/pages/chapter-page.test.tsx` reports `16 tests` passed, including the two new pagination regressions). Existing stderr warnings were unrelated React Router future-flag / act warnings and did not fail the run.

### Finding Disposition
- [W-QC3-002] **Resolved.** `useOpenFindingsCount` now derives `truncated` from the last loaded page's `pagination.has_more` and returns it with the lower-bound `rows.length` (`apps/web/src/components/reading/reading-hooks.ts:129-143`). `MaturationIndicators` forwards `findings.truncated` to `CountBadge`, and `CountBadge` renders `${count}+` when truncated (`apps/web/src/components/reading/maturation-indicators.tsx:47-54,81-94`). This satisfies the requested deviation: since `PaginationInfo` has no `total`, the explicit `N+` label is an honest lower-bound display and removes the silent exact-looking undercount. The regression test `renders an honest "N+" open-findings label when the page is truncated` verifies `has_more: true` renders `2+ open findings`.
- [W-QC3-001] **Resolved.** `useChapterNeighbors` now cursor-walks chapter pages via `fetchNextPage()` while `chapters.hasNextPage` is true and no next-page fetch is already in flight (`apps/web/src/components/reading/reading-hooks.ts:70-109`). Because `useChapters` obtains `hasNextPage` from `pagination.has_more ? next_cursor : undefined` (`apps/web/src/api/queries.ts:352-368`), normal first-page responses with `has_more: false` stop immediately and do not over-fetch. While page 1 or a cursor walk is still resolving, `loading` remains true (`isLoading || hasNextPage || isFetchingNextPage`) and `ChapterNav` renders neutral `Loading chapters…` placeholders rather than misleading `First chapter` / `Last chapter` labels (`apps/web/src/components/reading/chapter-nav.tsx:48-120`; `apps/web/src/pages/chapter-page.tsx:90-97`). The regression test `resolves prev/next across the server page boundary by cursor-walking` verifies a 150-chapter fixture paginated 100/page resolves chapter 101 with prev=100 and next=102, covering the daemon's 100-chapter clamp.

### Updated Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Revalidation Verdict**: Approve
