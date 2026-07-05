---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-27-v1.70-canvas-strategy-surface-implement"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-27-v1.70-canvas-strategy-surface-implement
- Review range / Diff basis: merge-base: 69310a3191d05c80460da227360cba6c9d6539b8 + tip: 6dabf0b58c39ddde641c0e0234828e6c7b89d8b3 (equivalent to: git diff 69310a31...HEAD -- apps/web/ .mstar/knowledge/specs/canvas-strategy-surface.md)
- Working branch (verified): iteration/v1.70
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (git rev-parse --show-toplevel)
- Files reviewed: 23 (10 new canvas files + 5 edited: package.json, App.tsx, sidebar.tsx, root-layout.tsx, daemon-status-bar.tsx + 2 design SSOT files + DESIGN dark mirror + tailwind.config.ts + index.css + 1 query-keys + 1 types + 1 nexus browser-client + 1 strategy-page + 1 test file)
- Commit range (identical to Review range line): 69310a31...6dabf0b5 (12 commits between base and tip on the iteration/v1.70 integration branch)
- Tools run: `git diff` / `git show` / `git log`, full reads of all new + edited files under `apps/web/src/{lib,components,pages}/canvas`, `apps/web/{DESIGN.md,DESIGN.dark.md,tailwind.config.ts,src/index.css}`, `grep` for `fetch` / `invoke` in canvas code, `pnpm --filter web typecheck`, `pnpm --filter web test`, `pnpm --filter web build`, and a post-build chunk inspection to verify route-split

## Findings

### 🔴 Critical

(none)

### 🟡 Warning

(none)

### 🟢 Suggestion

- **S-1 (architecture/maintainability): `__current__` sentinel as a `status` field value** → refactor before structured-edit lands in V1.71.
  - In `apps/web/src/components/canvas/strategy-canvas.tsx` line 71 the live-overlay effect writes the literal string `'__current__'` into `data.status` when a node is the current execution target but the session has no `status` string (e.g. `current_task_id` set, `status` empty). In `apps/web/src/components/canvas/strategy-nodes.tsx` line 117 the renderer recognizes the sentinel as `current`. The pairing works but couples the canvas setter and renderer via a private protocol, and it overloads the `status` field with two distinct meanings (live session status string vs. "this node is currently executing"). Cleaner contract: add `isCurrentOverlay: boolean` to `StrategyNodeData` and let the renderer derive `effectiveStatus` from `(isCurrentOverlay, status)`. Flagged as Suggestion rather than Warning because (a) α scope is read-only and no value path collides with `'__current__'` (the `statusFromSession` mapping has no entry containing that substring), and (b) the upgrade is small and V1.71 structured-edit work provides the natural landing.

- **S-2 (architecture/future-proofing): `NodeShell` `accent` boolean is Strategy-specific** → generalize before the Work outline / World KB surfaces land.
  - In `apps/web/src/components/canvas/strategy-nodes.tsx` line 77 the accent left border is hard-coded as `border-l-canvas-strategy-accent`. The same shell will host the next two canvas surfaces (Draft §3.3). Suggest promoting `accent` to a `tone: 'strategy' | 'work' | 'world-kb'` enum on `StrategyNodeData` (or its surface-specific replacement) and mapping tone → token. Acceptable for α because only one surface ships today.

- **S-3 (maintainability/test-coverage): no component-level tests for `canvas-shell`, `idea-input`, `strategy-canvas`, `strategy-alt-view`, `strategy-nodes`**.
  - The graph adapter is covered (`apps/web/src/lib/canvas/strategy-graph.test.ts`, 10 tests). The component layer — Idea-verb selection, effect-driven overlay patching, alt-view toggle, node-data mutation, error-toast path of `useErrorToast` — has no coverage. Acceptable for α (read + overlay + steer, no structured writes), but residual-worthy for V1.71 so the structured-edit additions land with a regression net rather than discovering regressions in QC.

- **S-4 (architecture/clarity): `useActiveSession` fallback `items[0]` can highlight a completed session's stale `current_task_id`**.
  - In `apps/web/src/lib/canvas/use-strategy-data.ts` lines 83–92, when all sessions are completed the hook returns `items[0]` (most recent). `current_task_id` may still point to the last executed node, so the canvas highlights that node as "current" even though execution is done. The comment on lines 88–89 documents the intent but not the user-visible implication ("you'll see a node highlighted as running for a completed session"). Suggest extending the helper text or returning `undefined` once all sessions are completed, with a follow-up to use the `status` enum from the daemon when a typed status field ships. Simplify is acceptable for α; V1.71 should revisit.

- **S-5 (architecture/clarity): `simplify:` `creator_id` derivation uses any schedule, not only the same preset's**.
  - In `apps/web/src/lib/canvas/use-strategy-data.ts` lines 95–103, `useDerivedCreatorId` falls back to `schedules.data?.[0]?.creator_id` regardless of preset. For a brand-new daemon (zero schedules) the comment correctly notes the Run button is disabled. For a daemon with multiple historical schedules owned by different creators, the first-by-list-order may not belong to the currently selected preset. Document the limitation in the helper's docstring and in `idea-input.tsx` so first-run attribution surprises don't surface as a bug. Acceptable for α; promote `creator_id` to an active-creator read endpoint (V1.67 G2 pattern) when the canvas needs deterministic author attribution.

- **S-6 (architecture/documentation): overlay polling cadence is shared but not co-located with the overlay policy**.
  - In `apps/web/src/lib/canvas/use-strategy-data.ts` line 35 `OVERLAY_POLL_MS = 5_000` is shared between `usePresetSessions` and `usePresetSchedules`. The comment on lines 56–58 describes the A3 bounded overlay ("current-node/status per A5 — completed-path history + child-session progress are V1.71") but the constant lives apart from the spec reference (canvas-strategy-surface.md §3.3, §3.7). Suggest moving the constant + its rationale into a named comment block or a small config file alongside the canvas data hooks, so the next reviewer can find the cadence alongside the policy that justifies it.

## Source Trace

- **Finding ID**: F-001 (S-1)
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/components/canvas/strategy-canvas.tsx:71` (sentinel write) ↔ `apps/web/src/components/canvas/strategy-nodes.tsx:117` (sentinel read)
  - Confidence: High

- **Finding ID**: F-002 (S-2)
  - Source Type: manual-reasoning + spec reference
  - Source Reference: `apps/web/src/components/canvas/strategy-nodes.tsx:77` (hard-coded accent) ↔ `.mstar/knowledge/specs/canvas-strategy-surface.md` §3.3 (three surfaces, shared shell)
  - Confidence: High

- **Finding ID**: F-003 (S-3)
  - Source Type: linter (test discovery) + manual-reasoning
  - Source Reference: `find apps/web/src -name "*.test.*" | grep -E "canvas|xyflow"` returns only `strategy-graph.test.ts`
  - Confidence: High

- **Finding ID**: F-004 (S-4)
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/lib/canvas/use-strategy-data.ts:83-92` (fallback to `items[0]`)
  - Confidence: High

- **Finding ID**: F-005 (S-5)
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/lib/canvas/use-strategy-data.ts:95-103` (any-schedule fallback)
  - Confidence: High

- **Finding ID**: F-006 (S-6)
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/lib/canvas/use-strategy-data.ts:35` (constant) vs. inline policy comment on lines 56–58
  - Confidence: Medium

## Architecture & Invariant Checks (no findings)

The following Assignment focus points were verified end-to-end. Each is recorded here so the next reviewer (or V1.71 implementer) can replay the audit without re-reading the codebase.

1. **Canvas architecture matches V1.69 Draft §3.2 mapping** — Verified.
   - Outer state → `strategy-state` node (`strategy-graph.ts:129-156`)
   - `inner_graph` enter → group node (`strategy-graph.ts:159-184`, parentId + `extent: 'parent'`)
   - Converge merge-point → join node (`strategy-graph.ts:131-133`)
   - terminal → terminal node (`strategy-graph.ts:129-130, 173-186`)
   - linear `next` → edge with `transitionKind: 'next'` (`strategy-graph.ts:190-191`)
   - conditional `next.rules[]` → branch edges with `condition` label (`strategy-graph.ts:193-202`)
   - `next.default` → default edge (`strategy-graph.ts:203-212`)
   - inner `inner_graphs.<n>.depends_on` → `depends_on` edges scoped to the group (`strategy-graph.ts:217-239`)
   - All mappings covered by 10 tests in `strategy-graph.test.ts` (lines 99-162), including initial/terminal marking, group with children, converge join, linear + conditional + default edges, depends_on edges, and BFS layering.

2. **Canvas Shell → Strategy adapter → overlay → Idea-input separation is clean** — Verified.
   - `canvas-shell.tsx`: only owns React Flow chrome (provider, controls, minimap, background, screen-reader summary, controlled-state helper `makeNodeChangeHandler`). Imports `Node`/`Edge`/`NodeTypes`/`OnNodesChange`/`OnEdgesChange` from `@xyflow/react`. No preset knowledge.
   - `strategy-graph.ts` + `preset-yaml.ts`: pure adapter, no React imports beyond type-only `Node`/`Edge`. Reads `preset.yaml` → emits nodes/edges/danglingTargets.
   - `strategy-nodes.tsx`: per-kind React components (`memo`ed), consumes `StrategyNodeData`. No fetch/query.
   - `strategy-canvas.tsx`: composer. Owns controlled React Flow state (`useNodesState`, `useEdgesState`), syncs the built graph into state on preset change, patches overlay status, exposes validation/inspector panels.
   - `use-strategy-data.ts`: TanStack Query bindings + NexusClient methods only. No React Flow imports.
   - `idea-input.tsx`: pure steering affordance; receives `presetId` / `creatorId` / `scheduleId` as props.
   - `strategy-alt-view.tsx`: non-spatial alternate; consumes only parsed manifest, no graph state.
   - No cross-layer leakage detected (e.g. the adapter does not call `useNexusClient`; the shell does not parse YAML; the data hooks do not import `@xyflow/react`).

3. **`@xyflow/react` is route-split (not in bootstrap)** — Verified.
   - `apps/web/src/App.tsx:20-22`: `const StrategyPage = lazy(() => import('@/pages/strategy-page').then((m) => ({ default: m.StrategyPage })));`
   - `apps/web/src/pages/strategy-page.tsx` line 10 docstring: "Route-split: this page (and therefore `@xyflow/react`) is lazy-loaded by `App.tsx`".
   - `apps/web/src/components/canvas/canvas-shell.tsx:32`: `import '@xyflow/react/dist/style.css';` — stylesheet lives in the canvas route chunk only.
   - Build artifact inspection: `dist/assets/index-G0IwWtDZ.js` (954.20 kB, bootstrap) contains 0 occurrences of `MiniMap` / `ReactFlow` / `xyflow`; `dist/assets/strategy-page-MIZ_O7bv.js` (305.40 kB, lazy) contains React Flow; `dist/assets/strategy-page-C5ap-Sga.css` (15.87 kB) holds the React Flow stylesheet. The bundle budget Draft §3.1 calls for (React Flow not in bootstrap) holds.

4. **NexusClient transport invariant holds (no raw `fetch` / `invoke`)** — Verified.
   - `grep -rn "fetch\|invoke\|@tauri-apps" apps/web/src/lib/canvas/ apps/web/src/components/canvas/ apps/web/src/pages/strategy-page.tsx` returns only `refetchInterval` / `refetch` calls (TanStack Query API, not transport). Zero direct transport access in canvas code.
   - Canvas-side NexusClient methods used: `getPreset` (read), `listSessions` (overlay), `listSchedules` (overlay + steer), `addSchedule` (Idea→Run), `editCoreContext` (Idea→Steer append), `signalSchedule` (Idea→Steer resume + Resume). All routed through `useNexusClient()` → `BrowserClient`. The transport core lives only in `apps/web/src/lib/nexus/browser-client.ts` (the V1.64 implementation); the `TauriClient` is a one-impl swap (commented as such on `types.ts:80-81`).

5. **`simplify:` items acceptable for α** — Verified (with follow-up suggestions in S-4/S-5/S-6).
   - BFS layout (`strategy-graph.ts:79-98`): single-layer `Map<string, number>` walk from `preset.initial` with orphan fallback. Comment lines 18-22 documents the upgrade path to dagre/elkjs and notes the output shape is preserved across the swap.
   - `creator_id` derivation (`use-strategy-data.ts:95-103`): falls back to `sessions.data?.[0]?.creator_id ?? schedules.data?.[0]?.creator_id`. Comment lines 16-21 documents the upgrade path (active-creator endpoint). Run button disabled with helper copy when no creator is derivable (`idea-input.tsx:55, 144-146`).

6. **`wire_contracts_changed: FALSE` verified** — Verified.
   - `git diff 69310a31...HEAD --stat -- schemas/ packages/nexus-contracts/ crates/nexus-contracts/ tooling/` returns empty.
   - `addSchedule` / `signalSchedule` / `editCoreContext` request/response types are imported from `@42ch/nexus-contracts` (already shipped in V1.67 G2); promoted onto the `NexusClient` interface (`types.ts:112-136`) without introducing new DTOs.
   - `getPreset` was promoted in V1.67 G2 and is reused as-is (`types.ts:155-156`).
   - No `pnpm run codegen` rerun required.

7. **DESIGN.md token consumption pattern followed** — Verified.
   - All canvas tokens added in `apps/web/DESIGN.md` frontmatter `components.canvas` (lines 187-198) and mirrored in `apps/web/DESIGN.dark.md` (lines 187-198). Both light and dark values populated.
   - CSS variables declared in `apps/web/src/index.css` (lines 88-99 light, 169-180 dark). Token names match the spec verbatim (Draft §3.6).
   - Tailwind utilities registered in `apps/web/tailwind.config.ts` lines 112-127 (`canvas.surface`, `canvas.grid`, `canvas.node-fill`, …). Production completeness level (3) carried forward.
   - All canvas node styles use the `bg-canvas-node-fill`, `border-canvas-node-border`, `border-canvas-node-border-selected`, `bg-canvas-surface`, `border-l-canvas-strategy-accent`, `text-purple-700/1000`, `bg-blue-700/...` utilities — no hard-coded hex in canvas components.
   - `bg-[color-mix(...)]` usages (e.g. `strategy-nodes.tsx:164`, `strategy-canvas.tsx:128`) reuse existing semantic tokens (purple, blue) rather than inventing values.

## Test & Build Verification

- `pnpm --filter web typecheck` — pass, no `tsc --noEmit` errors.
- `pnpm --filter web test` — pass, 131 tests across 16 files (3.13 s). New tests in `strategy-graph.test.ts` (10) cover parsing edge cases and every adapter mapping per Draft §3.2.
- `pnpm --filter web build` — pass, 2.79 s. Vite output confirms the route-split chunking: bootstrap (`index-*.js`, 954 kB) and lazy strategy page (`strategy-page-*.js`, 305 kB + 15.87 kB CSS). React Flow / MiniMap identifiers appear only in the strategy chunk (3+ string matches vs. 0 in the bootstrap).
- No new files under `apps/web/dist/` referenced schemas or contracts packages.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 6 |

**Verdict**: Approve

The P0 α-scope Canvas Strategy Surface implement matches the V1.69 Draft §3.2 mapping faithfully, separates the shell/adapter/overlay/idea-input layers cleanly, route-splits `@xyflow/react` out of the bootstrap (verified post-build), keeps all canvas access on the `NexusClient` interface (zero raw `fetch`/`invoke`), consumes the DESIGN.md canvas-token contract without inventing values, and honors the `wire_contracts_changed: FALSE` invariant (zero diff under `schemas/`, `packages/nexus-contracts/`, `crates/nexus-contracts/`, `tooling/`). Both `simplify:` items (BFS layout, `creator_id` derivation) carry documented upgrade paths and are acceptable for α. Typecheck, all 131 tests, and the production build pass.

The six Suggestions (S-1 through S-6) are forward-looking improvements — most pertinently S-1 (replace the `'__current__'` sentinel with an explicit `isCurrentOverlay` flag) and S-3 (component-level tests for the new files). They are **not blocking for P0 sign-off** and belong in the V1.71 backlog alongside structured-edit work; PM should consider opening residual entries for them so they are tracked in `status.json` rather than only in this report.
