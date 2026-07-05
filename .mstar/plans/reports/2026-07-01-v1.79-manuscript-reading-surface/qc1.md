---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-01-v1.79-manuscript-reading-surface"
verdict: "Approve"
generated_at: "2026-07-01"
---

# Code Review Report — P0 Reading Surface (qc-specialist #1: architecture/maintainability)

## Reviewer Metadata

- Reviewer: @qc-specialist (seat #1 — architecture coherence & maintainability)
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Module decomposition, hook composition, contract discipline, body-ownership invariant preservation, DESIGN.md token coherence (light + dark), test coverage.
- Report Timestamp: 2026-07-01 (ISO-8601)
- Scope restricted to P0 (Track A) per Assignment: `feature/v1.79-reading-surface` files only. P1 (SOUL viz) is a parallel track under the same diff but is out of scope for this seat's findings.

## Scope

- plan_id: `2026-07-01-v1.79-manuscript-reading-surface`
- Review range / Diff basis: `merge-base: 0015694f (origin/main) .. tip: 37d19d51 (HEAD)` — equivalent to `git diff 0015694f...HEAD`
- Working branch (verified): `iteration/v1.79`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Commit range (matches Review range): `0015694f..37d19d51`
- HEAD: `37d19d51 Merge feature/v1.79-soul-viz: P1 SOUL personality visualization + memory-fragment-info DTO 0.13->0.14`
- Files reviewed (P0 scope only):
  - `apps/web/src/components/reading/reading-hooks.ts` (114 lines)
  - `apps/web/src/components/reading/reading-prose.tsx` (150 lines)
  - `apps/web/src/components/reading/chapter-nav.tsx` (101 lines)
  - `apps/web/src/components/reading/reading-progress.tsx` (74 lines)
  - `apps/web/src/components/reading/maturation-indicators.tsx` (103 lines)
  - `apps/web/src/pages/chapter-page.tsx` (168 lines)
  - `apps/web/src/pages/chapter-page.test.tsx` (347 lines)
  - `apps/web/src/index.css` (reading-prose CSS vars: 68ch / 1.75 / 1.25em)
  - `apps/web/DESIGN.md` + `apps/web/DESIGN.dark.md` (reading-* + reading-maturation-badge tokens)
- Total P0 diff size: ~864 lines (incl. CSS vars + tokens), all six source modules under the 250-line discipline (`apps/web/AGENTS.md` invariant).
- Tools run:
  - `git rev-parse --show-toplevel` + `git branch --show-current` + `git log -1 --oneline` (Review Context Gate — `mstar-branch-worktree`).
  - `git diff --stat 0015694f...HEAD -- apps/web/` — scope verification.
  - `pnpm --filter web run typecheck` → clean (no output, exit 0).
  - `pnpm --filter web run test -- src/pages/chapter-page.test.tsx src/components/reading` → **44 test files passed, 321 tests passed**, including all 14 chapter-page tests and all P1 tests under the same diff (no P0-side regressions).
  - Manual cross-references against `crates/nexus-daemon-runtime/src/api/handlers/findings.rs` (`list_findings_handler` accepts comma-separated `status` since V1.50; `ListFindingsQuery.chapter` is `Option<i64>`), `apps/web/src/api/queries.ts` (cursor-paginated `useInfiniteQuery` for `useChapters` and `useFindings`), `apps/web/src/lib/canvas/use-world-kb-data.ts` (`useWorldKbGraph` returns `{entities, source_anchors, relationships}` projection), `apps/web/src/components/path-context-menu.tsx` (`useContextMenu` uses conditional render, not `hidden` toggle), and `packages/nexus-contracts/src/generated/local-api/works/chapters/ChapterSummary.ts` (`volume: number` is required, not optional).

## Findings

### 🟡 Warning

- **W-001 — `useOpenFindingsCount` silently truncates at page-1 limit (200 items), deviates from plan text** → **Fix / residual**
  - **Where**: `apps/web/src/components/reading/reading-hooks.ts:87-98`
  - **Evidence**:
    ```ts
    export function useOpenFindingsCount(workId, chapter) {
      const findings = useFindings(workId || undefined, {
        status: OPEN_FINDING_STATUSES,
        chapter,
        limit: NEIGHBOR_PAGE_LIMIT, // = 200
      });
      const rows = useMemo(() => flattenPages(findings.data), [findings.data]);
      return { count: rows.length, isLoading: findings.isLoading };
    }
    ```
    `useFindings` is an `useInfiniteQuery` (`apps/web/src/api/queries.ts:134-151`) — the caller never invokes `fetchNextPage()`, so only page 1 (200 items) is fetched. `flattenPages` flattens only fetched pages, so the count is capped at 200.
  - **Plan text (`.mstar/plans/2026-07-01-v1.79-manuscript-reading-surface.md` §Scope B, "Open-findings count")**: "paginate if exact counts exceed one page". Implementation does NOT paginate.
  - **User-visible effect**: For Works with >200 non-terminal findings on the chapter, the open-findings badge displays "200" (or less) instead of the true count, with no "200+" indicator.
  - **Maintainability impact**: The discrepancy between plan text ("paginate if needed") and code ("fetch first page only") makes the contract ambiguous. The dev flagged this as a known limit in the assignment brief, but the residual disposition is not yet tracked in `status.json.residual_findings[<plan-id>]`.
  - **Recommended fix**: Either (a) gate the count on `pagination.has_more` and surface "200+ findings" instead of an exact integer when truncated, or (b) call `fetchNextPage()` until `has_more=false` and report the true total (still bounded by 200/page * N pages; for V1.79 MVP an exact count above 200 is unlikely in practice). Log this as a residual (R#) tracked for V1.80.

- **W-002 — `useChapterNeighbors` returns `prev/next: null` for any chapter past page 1 (>200 chapters in the Work)** → **Fix / residual**
  - **Where**: `apps/web/src/components/reading/reading-hooks.ts:51-74`
  - **Evidence**:
    ```ts
    const chapters = useChapters(workId || undefined, { limit: NEIGHBOR_PAGE_LIMIT }); // = 200
    const rows = useMemo(() => flattenPages(chapters.data), [chapters.data]);
    // ...
    const idx = rows.findIndex((r) => matchCurrent(r, chapter, volume));
    if (idx === -1) {
      return { chapters: rows, prev: null, next: null, volumes: deriveVolumes(rows) };
    }
    ```
    If the user navigates to chapter 250 in a 500-chapter Work, `rows` only contains chapters 1-200. `findIndex` returns `-1`, and the hook falls into the not-found branch — silently degrading to "First chapter / Last chapter" placeholders in `<ChapterNav>`. The dev flagged this in the assignment brief ("neighbor-resolution 200-chapter limit (dev flagged)").
  - **Plan text**: §Scope A "Chapter/volume navigation" does not specify pagination, but §Acceptance criterion 5 mandates `apps/web` reading route ≤250-line module discipline (passed) and "Web typecheck + tests green" (passed). The 200-chapter page limit is a hidden capacity bound that isn't surfaced to users.
  - **Maintainability impact**: This is the kind of limit that bites a real author at scale. There is no UI affordance indicating truncation; the user thinks they're at the end of the manuscript when they're in the middle.
  - **Recommended fix**: Either (a) when `idx === -1`, request the current chapter detail and fetch additional pages around its `chapter` number, or (b) surface a small "Chapter N of M" indicator (using total count from the chapter detail endpoint if available). Log as residual (R#) tracked for V1.80.

### 🟢 Suggestion

- **S-001 — Keyboard navigation (←/→) has no direct test coverage; the `hasOpenOverlay` DOM-query guard is brittle as a pattern**
  - **Where**: `apps/web/src/pages/chapter-page.tsx:128-168` (`useChapterKeyboardNav`, `isEditable`, `hasOpenOverlay`)
  - **Evidence**: `hasOpenOverlay()` runs `document.querySelector('[role="menu"]:not([hidden]), [role="dialog"]:not([hidden])')` on every keydown. It works for the current reading surface because the only mounted overlay is `path-context-menu` which uses **conditional render** (not `[hidden]` toggle), but the pattern is fragile to any future overlay that adopts `hidden` toggling or uses a non-ARIA role for an interactive surface. Tests cover Escape-closes-menu and the listener cleanup, but do not exercise ←/→ chapter navigation, the `isEditable` guard, or the `hasOpenOverlay` guard at all (chapter-page.test.tsx only fires `Escape` keys, lines 263/268).
  - **Fix**: Add three unit tests for `useChapterKeyboardNav` (or `ChapterPage`): (1) ArrowLeft on chapter 2 with prev=1 triggers `navigate(...)` to chapter 1; (2) ArrowLeft while focus is in an `<input>` does NOT navigate; (3) ArrowLeft while path-context-menu is open does NOT navigate (the menu's `role="menu"` satisfies the guard).
  - **Architectural note**: The DOM-query guard couples the page to whatever overlays happen to be in the DOM at any moment. A more maintainable alternative would be a context provider (`ReadingOverlayContext`) or a portal-tracked count, but for the current single-overlay surface the simpler form is justified — flag for future overlay proliferation.

- **S-002 — Component-level unit tests for `reading-hooks.ts`, `chapter-nav.tsx`, `reading-progress.tsx`, `maturation-indicators.tsx` are absent; all coverage flows through `chapter-page.test.tsx` integration only**
  - **Where**: `apps/web/src/components/reading/*.{ts,tsx}` — no `*.test.{ts,tsx}` files exist in this directory.
  - **Evidence**: `find apps/web/src/components/reading -type f` shows only 5 source files, no test files. The integration test covers the page-level concerns (canvas redirect, body render, copy path, navigation labels, indicator counts) but does not unit-test the `stripFrontmatter` regex, the rAF/RAF_GUARD_MS scroll debounce in `reading-progress.tsx`, the `deriveVolumes` sort order, or the `CountBadge` loading/zero/attention variants.
  - **Fix**: Add focused tests for: `stripFrontmatter` (YAML `---` mid-content edge cases), `reading-progress.tsx` (initial flush sets `pct` from scroll position; resize updates), `maturation-indicators.tsx` (CountBadge `attention` variant when count>0, `neutral` when zero, `info` for KB density), `chapter-nav.tsx` (volume chip rendered only when `volumes.length > 1`).
  - **Architectural impact**: Without these, future refactors of the small components could regress behavior (e.g., the frontmatter regex) without test failures.

- **S-003 — `useChapterNeighbors.matchCurrent` volume-disambiguation is defensive against a schema constraint that already requires `volume: number`**
  - **Where**: `apps/web/src/components/reading/reading-hooks.ts:34-44`
  - **Evidence**: `ChapterSummary.volume: number` is a required field in `packages/nexus-contracts/src/generated/local-api/works/chapters/ChapterSummary.ts:15`. The hook's `matchCurrent(row, chapter, volume)` and `deriveVolumes(rows)` both use `row.volume ?? 1` defensively, which can never actually fire given the contract.
  - **Fix**: Drop the `?? 1` fallbacks (or, if defensive coding is the project preference, leave them with a brief comment "server contract requires volume; defensive fallback retained"). The chapter-nav `chapterHref` and `useChapter`/`useChapterBody` query construction also use `row.volume ?? 1` — same observation.
  - **Architectural impact**: Minor; consistency vs. wire-contract confidence. Lean toward removing.

- **S-004 — `reading-progress.tsx` RAF_GUARD_MS debounce is correct but slightly over-engineered for a 16ms (= 1 frame) window**
  - **Where**: `apps/web/src/components/reading/reading-progress.tsx:16-54`
  - **Evidence**: The effect maintains a `last` timestamp (`performance.now()`) and skips `flush` calls within 16ms of the last one, coalescing into an `rAF` if not already pending. This is equivalent to a passive scroll listener that just calls `setPct` directly — React's batching makes the difference negligible. The current code is not wrong (and correctly handles divide-by-zero via `scrollable > 0` short-circuit, plus `Math.max(0, Math.min(100, ...))` clamp).
  - **Fix**: None required; flag as a noted design choice. If future performance issues arise on long chapters, this debounce can be tightened (e.g., 32-50ms) or replaced with `IntersectionObserver`-based section markers.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| W-001 | manual-reasoning + plan-text-comparison | `apps/web/src/components/reading/reading-hooks.ts:87-98` vs `.mstar/plans/2026-07-01-v1.79-manuscript-reading-surface.md §Scope B` | High |
| W-002 | manual-reasoning + plan-text-comparison | `apps/web/src/components/reading/reading-hooks.ts:51-74` vs assignment brief dev flag | High |
| S-001 | manual-reasoning + test-coverage-audit | `apps/web/src/pages/chapter-page.tsx:128-168`; `apps/web/src/pages/chapter-page.test.tsx` (no ArrowLeft/ArrowRight test) | Medium |
| S-002 | doc-rule + test-coverage-audit | `apps/web/AGENTS.md` (test discipline) + `find apps/web/src/components/reading -type f` (no test files) | High |
| S-003 | manual-reasoning + contracts-audit | `apps/web/src/components/reading/reading-hooks.ts:34-44, 76-80, 32-35` vs `packages/nexus-contracts/src/generated/local-api/works/chapters/ChapterSummary.ts:15` | High |
| S-004 | manual-reasoning | `apps/web/src/components/reading/reading-progress.tsx:16-54` | Medium |

## Strengths (Architecture & Maintainability)

- **Module-size discipline held**: All six source modules ≤250 lines (chapter-nav 101, hooks 114, maturation 103, progress 74, prose 150, chapter-page 168). The 250-line rule is documented in `apps/web/AGENTS.md`; this plan lands well within budget even after adding V1.79 features.
- **Read-only invariant preserved (V1.75 pivot)**: No `useMutation` is imported by any reading component or the chapter page. The only edit affordance is the "Edit outline → Canvas" `<Link>` to the canvas route — verified in `chapter-page.test.tsx:336-346` (no Save/Edit body buttons). Body data flow is strictly `useChapter` + `useChapterBody` + `useChapters` + `useFindings` + `useWork` + `useWorldKbGraph` (all read-only hooks).
- **Hook composition is clean**: `reading-hooks.ts` is a thin read-only projection layer over existing hooks; no new query-key namespace, no new write path, no `useState`/`useRef` side channels. Reuses `flattenPages` (`apps/web/src/api/queries.ts:154-157`) for cursor-paginated flattening.
- **DESIGN.md token discipline (light + dark parity)**:
  - `reading-prose-{measure,line-height,paragraph-spacing}` are theme-independent metrics (`68ch / 1.75 / 1.25em`), identical in `DESIGN.md` and `DESIGN.dark.md` with the same values, justified by comment. The CSS vars are defined in `:root` only (`apps/web/src/index.css:153-155`) — no `.dark` override needed.
  - `reading-maturation-badge` colors use the established `color-mix(in_srgb, var(--color-teal-700)_10%, transparent)` pattern that already passes light+dark in the V1.77/V1.78 badges — no new color tokens invented. Concrete light + dark values land in DESIGN.md / DESIGN.dark.md.
  - The `chapter-completion-state` badge reuses the existing `ChapterStatusBadge` (`apps/web/src/components/status-badge.tsx:82-88`) which already maps `ChapterStatus` → `BadgeProps['variant']` per the V1.66 data-table mapping.
- **TypeScript strict, types-first**: All wire types come from `@42ch/nexus-contracts` (`ChapterSummary`, `ChapterBody`, `ChapterStatus`); no handwritten parallel DTOs. `typecheck` passes clean.
- **V1.75 residuals preserved verbatim**: `ReadingProse` keeps the `body` markdown render + frontmatter strip + `Copy Path` button + right-click `PathContextMenu`. `ChapterPage` keeps the canvas redirect CTA. The test "renders the canvas redirect CTA" (line 167-175) asserts the V1.75 behavior is intact.
- **Frontmatter stripping is correct**: `stripFrontmatter` uses the line-anchored regex `/\n---[ \t]*(?:\r?\n|$)/` (not naive `indexOf('---', 3)`) — this prevents the `title: foo --- bar` embedded-fence bug documented in the inline comment. Smart.
- **Stable references**: `proseRenderers` is a module-level constant (not re-created per render); `useMemo` for `bodyContent` only recomputes on body change; `useChapterNeighbors` returns a stable `useMemo`-cached object keyed on `[rows, chapter, volume]`.
- **Progress bar reset on navigation**: `progressKey = ${workId}:${ch.chapter}:${ch.volume ?? 1}` uses React `key` remount semantics, so navigating to a new chapter forces a fresh `useEffect` (no manual reset logic needed). The `pct` starts at 0 and the effect's initial `flush()` immediately picks up the current scroll position.
- **Accessibility**: Keyboard nav guards `INPUT`/`TEXTAREA`/`SELECT`/`isContentEditable`; prev/next buttons carry `aria-label="Previous chapter: <title>"`; volume chip carries `aria-label="Volume N"`; maturation badge text + count travel together (never color-only); reading-progress is a `role="progressbar"` with `aria-valuenow/min/max` and a text label ("`{pct}%`").

## Summary

| Severity | Count |
|---|---|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (disclosed known limits, residual-tracked) |
| 🟢 Suggestion | 4 (test coverage / minor design polish) |

**Verdict**: **Approve with residuals**

Rationale (per `mstar-roles/references/qc-specialist-shared.md` verdict rules): No unresolved Critical findings. The two Warnings are disclosed known MVP boundaries (dev flagged both in the assignment brief; plan text acknowledges the pagination contingency without mandating it for V1.79) without architectural disagreement — they are scope-bound MVP limits, not hidden bugs. Per `mstar-review-qc` §Residual Findings 留档门禁, **"Approve with residuals" is allowed when `Critical = 0`**; PM consolidated report must include the residual list with tracking pointers.

PM action items (suggested residual registration):
- R1 (W-001): `useOpenFindingsCount` page-1 truncation → owner `@frontend-dev`, target V1.80 backlog. Tracking: `status.json.residual_findings["2026-07-01-v1.79-manuscript-reading-surface"]` + this report §W-001.
- R2 (W-002): `useChapterNeighbors` >200-chapter degradation → owner `@frontend-dev`, target V1.80 backlog. Tracking: same path + §W-002.
- R3 (S-001): Keyboard nav + overlay-guard test coverage → owner `@qa-engineer` (test additions) + `@frontend-dev` (refactor if guard pattern changes). Tracking: same path + §S-001.

Body-ownership invariant (V1.75 pivot: canvas = sole authoring surface) is preserved. Read-only composition only. Acceptance criteria 1-6 from `§Acceptance` are all met by this implementation (verified via `pnpm --filter web run typecheck` clean + 321 tests pass + module size discipline + DESIGN.md token parity).

## Revalidation

**Trigger**: PM dispatched targeted re-review for fix-wave commit `3d33688e` (merged at `0d69b3c0`) addressing qc1 W-001 + qc1 W-002 (this report) and qc3 W-QC3-001 / W-QC3-002. Re-review scope restricted to those two warnings + the four Suggestions, in scope only where the fix-wave touched them.

**Scope verification (Review Context Gate, `mstar-branch-worktree`)**:
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus` (verified via `git rev-parse --show-toplevel`)
- Working branch: `iteration/v1.79` (verified via `git branch --show-current`)
- HEAD: `0d69b3c0` (merge commit containing fix-wave `3d33688e`)
- Review range / Diff basis: `merge-base: 0015694f (origin/main) .. HEAD: 0d69b3c0` (matches initial-review Assignment; fix-wave diff = merge commit + its parent fix commit)
- Files re-reviewed (fix-wave diff): `apps/web/src/components/reading/reading-hooks.ts`, `apps/web/src/components/reading/maturation-indicators.tsx`, `apps/web/src/components/reading/chapter-nav.tsx`, `apps/web/src/pages/chapter-page.tsx`, `apps/web/src/pages/chapter-page.test.tsx`

**Evidence**:
- `git show 3d33688e --stat`: 5 files changed, 193 insertions, 11 deletions. **Zero mutations / write routes introduced** (body-ownership invariant preserved — only `useEffect` / `useMemo` additions in `reading-hooks.ts`, derived boolean in `maturation-indicators.tsx`, pure render JSX changes in `chapter-nav.tsx` + `chapter-page.tsx`, test additions in `chapter-page.test.tsx`).
- `pnpm --filter @42ch/nexus-contracts run build`: contracts package rebuilt (required by `apps/web/AGENTS.md` build/typecheck contract); exit 0, dts emitted.
- `pnpm --filter web run typecheck`: clean, exit 0.
- `pnpm --filter web run test -- src/pages/chapter-page.test.tsx`: **16 chapter-page tests pass** (was 14; +2 new regression tests). Both new tests verified by name via `--reporter=verbose`:
  - ✓ `ChapterPage (V1.79 P0 QC fix-wave — pagination correctness) > renders an honest "N+" open-findings label when the page is truncated (qc3 W-QC3-002)` — 19 ms
  - ✓ `ChapterPage (V1.79 P0 QC fix-wave — pagination correctness) > resolves prev/next across the server page boundary by cursor-walking (qc3 W-QC3-001)` — 23 ms
- Full suite: **44 test files passed, 323 tests passed** (was 321; +2 regression tests, zero other regressions).

**Deviation review (PM brief vs. dev execution)**: The brief said "use `PaginationInfo.total`", but `PaginationInfo` (verified at `packages/nexus-contracts/src/generated/local-api/kb/PaginationInfo.ts:11-15` and the SSOT schema at `schemas/local-api/kb/pagination-info.schema.json:8-20`) declares exactly `{ limit, next_cursor?, has_more }` with `"additionalProperties": false`. **No `total` field exists, and the schema forbids adding one** — generating one would require both a schema change AND a daemon handler change AND a regenerated `@42ch/nexus-contracts` package, none of which the brief authorized. The dev's deviation to render an honest "N+" lower-bound label via `has_more` (qc3's option b) is therefore contract-faithful: no invented field, no schema drift, no fallback to a value the server never returns. This is the right call — inventing a `total` field would have been a contract violation; rendering "N+" matches the data the server actually sends.

**Per-finding disposition**:

| Finding ID | Status | Evidence |
|---|---|---|
| W-001 (open-findings page-1 truncation) | **Resolved** | `reading-hooks.ts:139-142`: `useOpenFindingsCount` now exposes `truncated: boolean` derived from `lastPage.pagination.has_more`. `maturation-indicators.tsx:52, 64-69, 81, 83`: `CountBadge` accepts `truncated` and renders `${count}+` when true (`const text = count === null ? '—' : truncated ? \`${count}+\` : String(count)`). Regression test asserts the truncated-page case renders the `aria-label="2+ open findings"` badge — verified passing. Honest lower-bound label; no clipped-looking exact integer. |
| W-002 (chapter-neighbor >200-chapter degradation) | **Resolved** | `reading-hooks.ts:81-89`: cursor-walk effect fires `fetchNextPage()` when `hasNextPage && !isFetchingNextPage`; `loading = isLoading \|\| hasNextPage \|\| isFetchingNextPage`. `chapter-nav.tsx:30-37, 48, 63-79, 104-120`: new `loading?: boolean` prop renders a neutral "Loading chapters…" placeholder (`aria-label="Loading chapters"`, using existing `border-gray-alpha-300` / `bg-background-200` / `text-gray-700` primitives — no new tokens) instead of misleading "First chapter"/"Last chapter" labels during the walk. Regression test: 150-chapter Work paginated at 100/page, chapter 101 resolves prev=100/next=102 after the walk — verified passing. Normal Works (first page returns `has_more: false`) never trigger the effect — no over-fetch. |
| S-001 (keyboard nav + overlay-guard test coverage) | Not in scope of fix-wave; no changes to `useChapterKeyboardNav` or keyboard coverage in `chapter-page.test.tsx`. **Still open** (unchanged from initial review). |
| S-002 (component-level unit tests for reading components) | Not in scope of fix-wave; the fix-wave added 2 integration tests in `chapter-page.test.tsx`, not component-level tests. **Still open** (unchanged from initial review). |
| S-003 (`volume ?? 1` defensive fallback) | Not in scope of fix-wave; defensive fallbacks still present in `deriveVolumes` (`reading-hooks.ts:113`), `matchCurrent` (`reading-hooks.ts:50`), `chapterHref` (`chapter-nav.tsx:40`). **Still open** (unchanged from initial review). |
| S-004 (RAF_GUARD_MS debounce over-engineering) | Not in scope of fix-wave. **Still open** (unchanged from initial review). |

**Architectural impact (architecture/maintainability lens)**:
- **Read-only invariant preserved (V1.75 pivot)**: zero `useMutation`/write paths added. Verified by `git show 3d33688e -- '*.ts' '*.tsx' | grep -E 'useMutation|fetch\([^)]*POST|fetch\([^)]*PATCH|fetch\([^)]*DELETE'` → no matches. All new logic is composition over existing read hooks (`useChapters`, `useFindings`).
- **Module-size discipline held** (`apps/web/AGENTS.md` ≤250-line rule, post-fix HEAD):
  - `reading-hooks.ts`: 159 lines (was 114, +45 — within budget)
  - `chapter-nav.tsx`: 124 lines (was 101, +23 — within budget)
  - `maturation-indicators.tsx`: 111 lines (was 103, +8 — within budget)
  - `chapter-page.tsx`: 169 lines (was 168, +1 — within budget)
- **Contract discipline held**: no parallel DTOs; all types from `@42ch/nexus-contracts`. `PaginationInfo` access is read-only through the `useFindings` / `useChapters` cache; no new query-key namespace.
- **DESIGN.md / DESIGN.dark.md parity**: no new tokens invented. `CountBadge` reuses the existing `color-mix(in_srgb, ...)` pattern from V1.77/V1.78 badges (light + dark correct via base CSS vars). The "Loading chapters…" placeholder uses standard `border-gray-alpha-300` / `bg-background-200` / `text-gray-700` primitives. "2+" / "N+" labels are inline count text, not titles/nav/buttons — Title Case rule from `apps/web/AGENTS.md` §Voice & Content does not apply.
- **Accessibility preserved**: `aria-label="Loading chapters"` on placeholder spans (screen-reader-distinct from `aria-label="No previous chapter"` / `"No next chapter"`); `aria-label="2+ open findings"` keeps the count + label together on `CountBadge` (no color-only state); cursor-walk effect dependency array `[chapters.hasNextPage, chapters.isFetchingNextPage, chapters.fetchNextPage]` avoids re-firing on unrelated store updates.

**Verdict update**: Both Warnings (W-001, W-002) resolved by fix-wave `3d33688e`. Zero new findings introduced. Body-ownership invariant intact, contract discipline intact, DESIGN.md token discipline intact, regression tests pass, typecheck clean, full suite green.

Per `mstar-review-qc` verdict rules: `Critical = 0`, `Warning = 0` (unresolved). Verdict upgraded from **Approve with residuals** → **Approve**.

**PM action items (residual lifecycle)**:
- R1 (W-001) and R2 (W-002) should be closed in `status.json.residual_findings["2026-07-01-v1.79-manuscript-reading-surface"]` and moved to `archived/residuals/<plan-id>.json` with `closure_note` referencing fix-wave commit `3d33688e` and `lifecycle: resolved` (per `mstar-plan-artifacts` §Residual 关闭与验证). QC does not own residual lifecycle writes.
- S-001, S-002, S-003, S-004 remain unaddressed by this fix-wave (intentionally out of scope per Assignment). If PM chooses to retain them as Suggestion-severity residuals into V1.80 backlog, register via `mstar-plan-artifacts/references/status-and-residuals.md` enum SSOT. Out of scope for this re-review round.