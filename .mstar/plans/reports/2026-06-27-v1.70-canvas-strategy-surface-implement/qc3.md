---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-27-v1.70-canvas-strategy-surface-implement"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-27-v1.70-canvas-strategy-surface-implement
- Review range / Diff basis: merge-base: 69310a3191d05c80460da227360cba6c9d6539b8 + tip: 6dabf0b58c39ddde641c0e0234828e6c7b89d8b3 (equivalent to: git diff 69310a31...HEAD -- apps/web/ .mstar/knowledge/specs/canvas-strategy-surface.md)
- Working branch (verified): iteration/v1.70
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 14 targeted source/spec/package files; diff scope is 22 files under `apps/web/`
- Commit range: dad35736, 10edf22f, 81cb4256, f82bcdd3, 079f687f
- Tools run: `git rev-parse --show-toplevel`; `git branch --show-current`; `git diff 69310a31...HEAD --stat -- apps/web/ .mstar/knowledge/specs/canvas-strategy-surface.md`; targeted source reads; Context7 React Flow + TanStack Query docs checks; `pnpm install --frozen-lockfile`; `pnpm --filter web build`; `pnpm --filter web test`; dist asset inspection for React Flow symbols.

## Findings
### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S1 (Duplicate polling observers in StrategyCanvas)**: `StrategyCanvas` mounts `useActiveSession(presetId)` and `useDerivedCreatorId(presetId)`, and each internally calls `usePresetSessions(presetId)`; it also mounts `usePresetSchedules(presetId)` directly and again through `useDerivedCreatorId`. TanStack Query shares the cached query and deduplicates in-flight requests, but each observer owns its own `refetchInterval` timer. At α scale this is a small, bounded cost (two observers for sessions and two for schedules, all at 5 s), and there is no evidence of a leak on unmount. If the Strategy route becomes long-lived or adds more overlay consumers, compose the session/schedule queries once in `StrategyCanvas` and pass derived values down so there is one observer per polled resource.
- **S2 (Polling first-page list endpoints has a many-sessions ceiling)**: `usePresetSessions` and `usePresetSchedules` call `client.listSessions()` / `client.listSchedules()` with no query and then filter the returned first page client-side. This is acceptable for the A5 α decision (no new read route, bounded overlay from existing list summaries), and it is not an unbounded memory operation because only the response page is cached. The reliability ceiling is that a preset's active session/schedule can be absent from the first page when the daemon has many sessions or schedules, causing the live overlay or `creator_id` derivation to appear empty. Future work should add a preset-scoped/active-creator read projection or use server-supported filters once available.
- **S3 (Graph layout is intentionally small-graph only)**: `buildStrategyGraph` uses a naive BFS layering with `queue.shift()` and inner `depends_on` membership checks via `graph.nodes.some(...)`. For the stated α target (≤~10 outer states) this is comfortably performant, and tests cover the representative projection. The code correctly labels the `simplify:` ceiling. If Strategy graphs grow toward hundreds of nodes, move to a layout engine (`dagre`/`elkjs`) and enable React Flow viewport culling (`onlyRenderVisibleElements`) per current React Flow guidance.
- **S4 (Chunk-size posture is acceptable for α, but bootstrap remains large)**: The build creates a separate `strategy-page-MIZ_O7bv.js` at 305.40 kB minified / 97.28 kB gzip and `strategy-page-C5ap-Sga.css` at 15.87 kB / 2.67 kB gzip. Asset inspection found React Flow symbols (`ReactFlowProvider`, `react-flow`, `xyflow`, `MiniMap`) only in the strategy page chunk, not in `index-G0IwWtDZ.js`, so React Flow is route-split as required. The existing bootstrap chunk is still 954.20 kB minified / 304.30 kB gzip and triggers Vite's generic >500 kB warning; this is not caused by React Flow entering bootstrap, but should remain on the broader web bundle hygiene radar.

## Source Trace
- Finding ID: S1
- Source Type: manual-reasoning + library-docs
- Source Reference: `apps/web/src/components/canvas/strategy-canvas.tsx:37-40`; `apps/web/src/lib/canvas/use-strategy-data.ts:55-79,82-103`; TanStack Query docs/code: each `QueryObserver` manages its own `refetchIntervalId`; Query cache shares one `Query` per key and deduplicates in-flight fetches.
- Confidence: High

- Finding ID: S2
- Source Type: manual-reasoning + contract/source review
- Source Reference: `apps/web/src/lib/canvas/use-strategy-data.ts:55-79,94-103`; `packages/nexus-contracts/dist/index.d.ts:873-878` (`ListSessionsQuery` has no `preset_id`); `packages/nexus-contracts/dist/index.d.ts:1254-1260` (`ListSchedulesQuery` has no `preset_id`).
- Confidence: High

- Finding ID: S3
- Source Type: manual-reasoning + test review + library-docs
- Source Reference: `apps/web/src/lib/canvas/strategy-graph.ts:18-21,79-112,216-239`; `apps/web/src/lib/canvas/strategy-graph.test.ts`; React Flow performance guidance for `onlyRenderVisibleElements` on large graphs.
- Confidence: High

- Finding ID: S4
- Source Type: build-output + dist inspection
- Source Reference: `pnpm --filter web build`; `python3` asset scan of `apps/web/dist/assets/*.js` showing React Flow symbols only in `strategy-page-MIZ_O7bv.js`.
- Confidence: High

## Performance / Reliability Checklist
- React Flow route-split: Pass. `App.tsx` lazy-loads `StrategyPage`; build output has a separate 305.40 kB strategy chunk; React Flow symbols are absent from the bootstrap JS chunk.
- 5 s polling interval: Pass for α. The cadence is calm for local loopback and uses TanStack Query rather than hand-rolled intervals. No `refetchIntervalInBackground` is set, so polling follows focus defaults.
- Polling cleanup / memory leak risk: Pass. No manual `setInterval`, event listener, subscription, `ResizeObserver`, or animation loop was introduced in the canvas files. TanStack Query owns observer interval cleanup on unmount; React Flow provider is route-local.
- Unbounded operations: Pass with ceiling noted. Graph construction is bounded by preset YAML size; session/schedule polling is page-bounded but first-page-only and can miss data at high cardinality (S2).
- BFS layout performance: Pass for α. The `simplify:` comment accurately records the ≤~10-state target and upgrade path.
- First-run / empty creator state: Pass. With zero sessions/schedules, `creatorId` is `undefined`; `IdeaInput` disables Run and shows a helper rather than throwing. The broader “active creator” endpoint gap is deferred by design.
- Bundle size: Pass for α. The strategy route chunk is 305.40 kB minified / 97.28 kB gzip and isolated from bootstrap.

## Validation Evidence
- Initial `pnpm --filter web build` / `pnpm --filter web test` failed before workspace install because `apps/web/node_modules` lacked the newly locked `@xyflow/react` and `yaml` packages. After `pnpm install --frozen-lockfile` (lockfile already up to date; no tracked source changes), both commands were rerun.
- `pnpm --filter web build`: pass. Vite output: `strategy-page-MIZ_O7bv.js` 305.40 kB / gzip 97.28 kB; `index-G0IwWtDZ.js` 954.20 kB / gzip 304.30 kB; generic chunk-size warning on bootstrap remains.
- `pnpm --filter web test`: pass. 16 test files, 131 tests passed. React Router future-flag warnings are pre-existing test stderr noise, not failures.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve
