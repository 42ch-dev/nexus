---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-01-v1.79-manuscript-reading-surface"
verdict: "Approve"
generated_at: "2026-07-01"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (no write routes on reading surface, ownership/auth boundary preservation, input validation on user-controlled values, DOM/keyboard safety)
- Report Timestamp: 2026-07-01

## Scope
- plan_id: 2026-07-01-v1.79-manuscript-reading-surface
- Review range / Diff basis: merge-base: 0015694f (origin/main) .. tip: 37d19d51 (HEAD)
- Working branch (verified): iteration/v1.79
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6 (apps/web/src/components/reading/chapter-nav.tsx, maturation-indicators.tsx, reading-hooks.ts, reading-progress.tsx, reading-prose.tsx, apps/web/src/pages/chapter-page.tsx)
- Commit range: 0015694f...37d19d51 (git diff per assignment)
- Tools run:
  - git branch --show-current && git rev-parse HEAD && git merge-base 0015694f HEAD (alignment verification)
  - pnpm --filter @42ch/nexus-contracts run build (contracts built before review)
  - git diff 0015694f...HEAD -- apps/web/src/components/reading apps/web/src/pages/chapter-page.tsx
  - grep for useMutation / .mutate / client.(post|put|patch|delete|create|update) scoped to the diff (zero matches on reading surface)
  - grep for dangerouslySetInnerHTML / innerHTML / document.write / eval (zero matches)
  - grep for DESIGN reading-* tokens (present and dual-themed)
  - Manual security/correctness review per assignment checklist (no-write invariant, auth boundary, query param validation, keyboard guard, ReactMarkdown usage)

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- S-001: `useChapterNeighbors` and `useOpenFindingsCount` use a broad `limit: 200` page to resolve neighbors and counts in one round-trip. This is intentional for small-to-medium manuscripts; for very large Works a dedicated narrow neighbor endpoint or server-side "current chapter index" hint could reduce payload. Not a correctness or security issue for V1.79 scope.
- S-002: Volume query param parsing (`Number(raw)`) + `> 0` guard is present, but the value is passed through to the query layer without an upper bound. The existing `useChapter`/`useChapterBody` contracts already scope by work+chapter+volume; adding an explicit `volume <= 9999` or similar would be defensive hygiene only (low priority).
- S-003: `stripFrontmatter` is duplicated (once in the retired `BodyReadOnly` inline, once in `ReadingProse`). The new location is the canonical one; the old function was removed from the page. A small shared util could be extracted later if more surfaces need the same strip logic.

## Source Trace
- Finding ID: QC2-2026-07-01-R-V179READ-01
- Source Type: git-diff + targeted grep + manual authorization/ownership walk
- Source Reference: apps/web/src/components/reading/reading-hooks.ts:1-114 (read-only composition), reading-prose.tsx:1-150 (ReactMarkdown + PathContextMenu), chapter-page.tsx:1-168 (keyboard nav guard + no mutation), DESIGN.md:69-75 + 268-289 and DESIGN.dark.md (reading-* tokens)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Detailed Security & Correctness Review (per assignment)

### 1. No write route / no mutation (V1.75-pivot body-ownership invariant — P0)
- Explicit grep for `useMutation`, `.mutate`, `client.(post|put|patch|delete|create|update)` **scoped to the reading-surface diff only** returned **zero matches**.
- All data access in the new surface is via existing read-only query hooks:
  - `useChapters`, `useFindings`, `useWork` (from `@/api/queries`)
  - `useWorldKbGraph` (from `@/lib/canvas/use-world-kb-data`)
- `reading-hooks.ts` header explicitly documents: "Read-only composition of EXISTING query hooks... nothing here mutates."
- The "Edit outline → Canvas" CTA correctly routes the user back to the sole authoring surface (canvas) per the V1.75 pivot. No in-place body or outline mutation path exists on the reading surface.
- Mutations found by grep were in **other** pages (chapters-page.tsx, memory-page.tsx, work-detail-page.tsx, etc.) — outside the review scope and expected.

### 2. Read-only consumption does not bypass auth/ownership boundaries
- Chapters, findings, and world KB are fetched via the **same** query functions used elsewhere in the app.
- Ownership scoping (creator/work/world) is enforced server-side by the existing daemon endpoints (`list_chapters`, `list_findings`, `get_world_kb_graph`, etc.). The reading surface performs no client-side trust of untrusted input for authorization decisions.
- `useWorldKbDensity` resolves `world_id` from `useWork(workId)` — the Work record already carries the correct world binding; no client-side world_id spoofing surface is introduced.
- `useOpenFindingsCount` passes `status: "open,triaged,in_review"` and `chapter` — both are passed through the existing `useFindings` contract; the server enforces work+chapter scoping.

### 3. Keyboard navigation guard (XSS / hijack surface)
- `useChapterKeyboardNav` (chapter-page.tsx:133-153) explicitly guards:
  - `e.defaultPrevented` early return
  - `e.metaKey || e.ctrlKey || e.altKey` early return
  - `isEditable(el)` check for INPUT/TEXTAREA/SELECT/contentEditable
  - `hasOpenOverlay()` check for `[role="menu"]:not([hidden])` or `[role="dialog"]:not([hidden])`
- Navigation only fires on clean ArrowLeft/ArrowRight when the reader is not typing or using a menu.
- This prevents the keyboard handler from stealing focus or injecting navigation while the user is interacting with the PathContextMenu or any form field.
- No `addEventListener` without cleanup; effect properly removes the listener on unmount.

### 4. Input validation on user-controllable values (query params / route params)
- Volume: `searchParams.get('volume')` → `Number(raw)` → `n > 0 ? { volume: n } : undefined`. Negative/zero/NaN values are rejected.
- Chapter: `Number(chapterParam)` from route `:chapter`. The value flows into `useChapter`/`useChapterBody`/`useChapterNeighbors` which pass it to the query layer; the server returns 404/empty for non-existent chapters. No raw interpolation into URLs or queries beyond the standard encoded path.
- `encodeURIComponent(workId)` is used on all Link targets — correct.
- No `dangerouslySetInnerHTML`, no `innerHTML`, no `document.write`, no `eval` in the diff.
- ReactMarkdown is used with only `remark-gfm` (no `rehype-raw` or dangerous plugins). Prose renderers only override `<p>` for typography tokens.

### 5. PathContextMenu usage (existing component, read-only affordance)
- `ReadingProse` re-uses the established `PathContextMenu` + `useContextMenu` for the "Copy Path" action on the body.
- The menu is rendered only when `menu.open`; it is a fixed-position role="menu" with proper `aria-label`.
- Copy path uses the standard `navigator.clipboard.writeText` with success/error toasts — no injection surface.
- The component already had desktop-gated native actions (`openWith`, `revealInFinder`) that are conditionally rendered and defended by the desktop capability gate.

### 6. DESIGN token alignment (reading surface)
- Typography tokens (`reading-prose-measure`, `reading-prose-line-height`, `reading-prose-paragraph-spacing`) are defined in both `DESIGN.md` and `DESIGN.dark.md` with identical metric values (theme-independent) and consumed via CSS vars in `index.css`.
- Component tokens (`reading-chapter-nav`, `reading-progress-indicator`, `reading-maturation-badge`) exist under the same names in both theme files.
- Implementation in `reading-prose.tsx` reads the CSS vars directly for line-height and paragraph spacing.
- `maturation-indicators.tsx` and `chapter-nav.tsx` use DESIGN-aligned primitives (badge colors via `color-mix`, `button.secondary`, `rounded-pill`, etc.).
- No token invention; all new visual values trace back to DESIGN.md entries added for V1.79 P0.

### 7. Error / loading / empty states
- Proper use of shared `LoadingState` and `ErrorState` components.
- `ReadingProse` surfaces a retry that calls `body.refetch()` — correct.
- Chapter page surfaces a retry on the chapter query.
- No silent failures; loading and error paths are explicit.

### 8. No new contracts or backend surface
- Zero new API routes, DTOs, or query keys introduced.
- All data shapes come from `@42ch/nexus-contracts` (ChapterSummary, ChapterBody, ChapterStatus, etc.).
- The surface is a pure client-side composition and presentation layer.

### 9. Concurrency / reactivity notes (correctness)
- `useChapterNeighbors` derives `prev/next/volumes` in a `useMemo` over the flattened page list.
- `ReadingProgress` is keyed by `${workId}:${chapter}:${volume}` so the bar resets on chapter navigation (as required by the clarify decision).
- Scroll listener uses `passive: true` and a RAF guard (16 ms) — efficient and non-blocking.
- No mutable module state; all state is React hook or local effect state.

### 10. Residuals / CI
- No new residuals introduced by this change.
- Contracts build was clean before review.
- No CI failures observed in the scope of this review (full web typecheck/build was not re-run locally beyond contracts per assignment guidance; the diff is narrow and uses only pre-existing query hooks and UI primitives).

## Completion Notes
- The change is a read-only reading surface that strictly preserves the V1.75 "canvas is the sole authoring surface" invariant.
- Security surface is minimal and well-guarded (keyboard focus, clipboard, query param validation, ReactMarkdown hygiene).
- Correctness is high: pure derivation from existing queries, proper guards, DESIGN token fidelity.
- Only low-priority suggestions; no blocking issues.

**Verdict**: Approve
