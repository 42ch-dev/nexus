---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-29-v1.75-canvas-pivot"
verdict: "Request Changes"
generated_at: "2026-06-29"
---
# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-29T15:26:40Z

## Scope
- **plan_id**: `2026-06-29-v1.75-canvas-pivot` (lead; covers P0 + P1)
- **Review range / Diff basis**: `6e6b42c6..8360fa10` (origin/main merge-base..iteration/v1.75 HEAD; 12 commits). Equivalent to `git diff 6e6b42c6..8360fa10`.
- **Working branch (verified)**: `iteration/v1.75`
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 50 changed files via `git diff --stat`; deep review focused on `chapter-page.tsx`, outline canvas routing/selection, TipTap round-trip tests, codegen source/generated output, world-KB graph cap note, and relevant route/client removals.
- Commit range: identical to Review range.
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse --short HEAD && git rev-parse HEAD && git status --short`
  - `git diff 6e6b42c6..8360fa10 --stat`
  - `pnpm run codegen && git diff --exit-code -- crates/nexus-contracts/src/generated/ packages/nexus-contracts/src/generated/`
  - `pnpm --filter web build`
  - `pnpm --filter web test -- --run`
  - `pnpm run validate-schemas`
  - `SQLX_OFFLINE=true cargo test -p nexus-contracts --test schema_drift_detection`
  - Targeted file reads / grep for route, PUT DTO, Eq derive, graph cap, and TipTap parity checks.

## Findings
### 🔴 Critical
- None.

### 🟡 Warning
- **F-QC3-001 — Canvas redirect CTA does not honor the `?chapter=` preselect parameter.** `ChapterPage` builds the requested link (`/works/{workId}/outline?chapter={n}`), and the route exists, but `OutlinePage` only passes `workId` into `<OutlineCanvas />`, while `OutlineCanvas` initializes `selectedChapterId` to `null` and never reads `useSearchParams()` / `chapter`. This means the CTA lands on the canvas but does not pre-select the chapter in the inspector, violating the locked morphology and DoD expectation that the CTA opens the canvas with the chapter preselected. -> Fix by wiring the query param into the outline page/canvas initial selection (and add a route-level test that rendering `/works/w-123/outline?chapter=1` selects chapter 1 / shows its inspector content), while preserving normal manual selection.

### 🟢 Suggestion
- **S-QC3-001 — Web build still emits the Vite chunk-size warning.** The build passes, and canvas routes are lazy-split, but `pnpm --filter web build` reports a chunk-size warning (`index-DwG6dCnG.js` ~508.93 kB; outline page ~476.33 kB). This is not a blocker for this pivot, but the new TipTap inspector keeps the outline surface relatively large. Consider a future manualChunks/lazy-boundary pass if the bootstrap chunk continues to grow.

## Source Trace
- Finding ID: F-QC3-001
  - Source Type: manual-reasoning + git-diff
  - Source Reference:
    - `apps/web/src/pages/chapter-page.tsx:57` builds `const canvasHref = "/works/${encodeURIComponent(workId)}/outline?chapter=${ch.chapter}"`.
    - `apps/web/src/pages/outline-page.tsx:12-16` reads only `workId` and renders `<OutlineCanvas workId={workId} />`.
    - `apps/web/src/components/canvas/outline-canvas.tsx:47` initializes `selectedChapterId` as `null`; grep found no `useSearchParams`, `searchParams.get('chapter')`, or equivalent query handling in the outline canvas path.
    - `apps/web/src/pages/chapter-page.test.tsx:69-77` checks only the CTA href, not the destination preselection behavior.
  - Confidence: High
- Finding ID: S-QC3-001
  - Source Type: build-output
  - Source Reference: `pnpm --filter web build` Vite warning: “Some chunks are larger than 500 kB after minification”; output includes `dist/assets/index-DwG6dCnG.js 508.93 kB` and `dist/assets/outline-page-myr9CR0D.js 476.33 kB`.
  - Confidence: Medium

## Shared Checklist
### Code quality
- Naming and ownership are generally clear: `ChapterOutlineContentEditor` is extracted and `chapter-inspector.tsx` remains under the 250-line cap (248 lines).
- The chapter-page morph is substantially simpler and removes the old save-state/PUT editor path.

### Security and correctness
- PUT outline write DTO/consumer removal appears complete for code paths: grep found only comments mentioning `PutChapterOutlineRequest` / `putChapterOutline`, no live schema/API/client method.
- Body read-only render remains isolated from outline write behavior.
- One correctness gap remains: the CTA destination does not consume the chapter preselect query.

### Performance and reliability
- Codegen determinism passed: rerunning `pnpm run codegen` produced zero diff in committed generated TS/Rust contract files.
- B1 Eq correctness spot-check passed: non-f64 `OutlinePatchChapterSet` derives `Eq`; f64-bearing `WorldKbRelationshipProjection` and transitive `WorldKbGraphResponse` correctly skip `Eq`.
- B6 graph cap is at least documented: `GRAPH_ENTITY_CAP` remains 500 and `world_kb.rs` documents V1.76 relationship pagination/truncation follow-up.
- Integrated web build/test and schema/Rust drift gates pass. The web build emits only a non-fatal Vite chunk-size warning.

### Maintainability
- The TipTap round-trip parity test covers headings, bold, italic, bullet list, ordered list, blockquote, mixed document content, and repeated round-trip stability.
- The missing query-param preselection test is the main reliability coverage gap for the chapter-page CTA.

## Validation Results
- `pnpm run codegen && git diff --exit-code -- crates/nexus-contracts/src/generated/ packages/nexus-contracts/src/generated/` — PASS (no generated diff).
- `pnpm --filter web build` — PASS (Vite chunk-size warning noted as suggestion).
- `pnpm --filter web test -- --run` — PASS: 34 files, 235 tests passed (React Router future warnings and pre-existing act warnings observed in output).
- `pnpm run validate-schemas` — PASS: 170 valid, 0 invalid.
- `SQLX_OFFLINE=true cargo test -p nexus-contracts --test schema_drift_detection` — PASS: 4 passed.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes
