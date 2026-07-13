# Canvas Architecture Foundation — Iteration Spec

| Attribute | Value |
| --- | --- |
| **Status** | Draft (V1.114) |
| **Document class** | Iteration-scoped spec draft |
| **Scope** | Shared canvas adapter abstraction + dagre auto-layout engine + shared hook consolidation |
| **Coordinates with** | [canvas-strategy-surface.md](../../../specs/canvas-strategy-surface.md) §3.3 (Canvas Shell), [web-ui.md](../../../specs/web-ui.md) |
| **Parent plan** | `2026-07-13-v1.114-canvas-architecture-foundation` |

## Product framing (why authors care)

The canvas is the **steering surface**: authors should open a Work and *think
about structure*, not spend the first minutes arranging boxes. Today every new
surface also costs the product team a full copy of layout/conflict/alt-view/
inspector wiring — so the next surface (compute graph, session replay) arrives
slower and less consistently.

This plan is the **usability + extensibility** half of the canvas foundation:
readable graphs on open, and a recipe so the fifth surface is not a rewrite.

## User stories

1. **As a novelist with a large outline**, when I open Outline canvas I see a
   readable top-down (or surface-default) arrangement of volumes/chapters
   without dragging every node first.
2. **As a worldbuilder with dozens of KeyBlocks**, when I open World KB canvas I
   get an automatic layout that makes entity clusters scannable, then I refine
   positions by hand where story relationships matter.
3. **As an author who already placed nodes carefully**, when I drag a node my
   placement sticks; only an explicit **Re-layout** action re-runs the engine.
4. **As a product team shipping a 5th canvas surface**, I follow a documented
   adapter recipe instead of copying ~300 lines of shell wiring from Strategy.

## Problem

The four shipped canvas surfaces (Strategy, Outline+Timeline, World KB entities,
World KB relationships) each independently wire:

1. A `canvas-layout.tsx` that assembles `CanvasShell` + surface nodes + edges
2. A conflict-modal host that adapts the shared `conflict-modal-base.tsx`
3. An alt-view (table/list) companion
4. An inspector panel + per-node inspector components
5. A `use*CanvasGraph()` hook that projects Daemon API DTOs → React Flow nodes/edges

This duplication is now four-fold — the "three concrete use cases" threshold for
abstraction (STRATEGY.md "Simplicity over premature abstraction") is crossed.
Adding a 5th surface (compute graph, session replay) today would copy ~300 lines
of boilerplate before any surface logic.

Separately, **canvas nodes have no auto-layout**. Authors manually drag every
node into a readable position. This was deferred in V1.72 and named in every
subsequent compass retrospective as the top canvas UX gap. As Works grow (30+
chapters, 50+ world entities), the canvas becomes a chore rather than a steering
surface.

## Goals

1. **Shared canvas adapter abstraction** — extract a `CanvasSurfaceAdapter`
   interface + `useCanvasSurface()` hook that consolidates the duplicated
   wiring: graph projection, conflict handling, alt-view toggle, inspector
   routing. Migrate ≥2 of the 4 shipped surfaces to consume it (proving the
   abstraction without a big-bang rewrite).
2. **Dagre auto-layout engine** — integrate `dagre` (or `@dagrejs/dagre`) as a
   layout backend. Provide a `useAutoLayout()` hook (or shell-level option)
   that positions nodes automatically on graph load + a "Re-layout" action.
   Preserve manual override: once an author drags a node, that position wins
   until the next re-layout.
3. **Shared hook consolidation** — consolidate viewport caching, selection
   model, dirty-state guard into shared hooks consumed by all surfaces.
4. **Architecture documentation** — update `canvas-strategy-surface.md` §3.3 to
   document the adapter pattern as the canonical way to add a canvas surface,
   so future surface additions follow a repeatable recipe.

## Non-goals

- Rewriting all 4 surfaces to the new adapter (≥2 is the proof target; the rest
  migrate opportunistically in future iterations)
- Changing any canvas surface's domain semantics (nodes, write ops, inspectors
  stay surface-owned; only shell wiring + layout capability change)
- Edge-routing layout (dagre positions nodes; edge routing stays React Flow
  default)
- Persisting layout positions to the daemon (V1.114 ships **ephemeral**
  auto-layout only; save-layout is a follow-on)
- ELK or multi-engine layout selection (dagre only this iteration)

## Acceptance criteria (product-facing)

- [ ] Opening a migrated surface with a non-trivial graph yields a readable
  auto-layout without mandatory manual positioning
- [ ] **Re-layout** control is discoverable on the canvas chrome; running it
  repositions nodes via dagre
- [ ] Manual drag after layout is preserved until the next Re-layout
- [ ] ≥2 shipped surfaces consume the shared adapter; existing surface tests
  still pass (no domain-behavior regression)
- [ ] `canvas-strategy-surface.md` §3.3 documents the adapter + layout recipe

## Interface sketch (locked by @architect — Review chain Seat 2)

```ts
/** A surface adapter plugs domain DTOs into the shared canvas shell. */
interface CanvasSurfaceAdapter<TGraph, TNodeData, TEdgeData> {
  surfaceKind: CanvasSurfaceKind;
  /** Project daemon graph DTO → React Flow nodes + edges.
   *  Responsible for setting parentId + extent:"parent" on child nodes
   *  so group/sub-flow nesting (Strategy inner-graphs, outline volumes,
   *  World KB clusters) is preserved for layout. */
  projectGraph(graph: TGraph): { nodes: Node<TNodeData>[]; edges: Edge<TEdgeData>[] };
  /** Node types registry for this surface. */
  nodeTypes: NodeTypes;
  /** Edge types registry (optional). */
  edgeTypes?: EdgeTypes;
  /** Preferred layout direction consumed by useAutoLayout (default "TB"). */
  layoutOptions?: { direction?: "TB" | "LR"; rankSep?: number; nodeSep?: number };
  /** Conflict DTO → conflict-modal props. */
  adaptConflict?(error: unknown): ConflictModalProps | null;
  /** Inspector routing: which inspector renders for a selected node. */
  renderInspector?(node: Node<TNodeData>): ReactNode;
  /** Alt-view companion (table/list). */
  renderAltView?(): ReactNode;
  /** Graph-level a11y summary (required — a11y is not optional). */
  summarizeGraph(graph: TGraph): string;
}
```

### Hook composition

The shared `useCanvasSurface(adapter, queryResult)` hook is a **composition of
sub-hooks**, not a re-implementation:

- **Graph projection** — owns: calls `adapter.projectGraph`, memoizes result.
- **Conflict state** — owns: calls `adapter.adaptConflict`, manages retry/merge state.
- **Alt-view toggle** — owns: graph ↔ list/table switch (a11y companion).
- **Inspector selection** — owns: selected node → `adapter.renderInspector`.
- **Viewport caching** — **delegates** to the existing `useCanvasViewport(surfaceKey)`
  (already shipped in `canvas-shell.tsx`); the hook does not re-implement it.
- **Auto-layout application** — **delegates** to the new `useAutoLayout(nodes, edges,
  adapter.layoutOptions)` hook (T4); `useCanvasSurface` wires the result back into
  node positions but does not compute layout itself.

This separation ensures T1 (adapter + hook) can ship with a no-op layout
integration point, and T4 (dagre) fills it in without touching the adapter
contract.

## Layout integration

- `dagre` computes `position { x, y }` for each node based on edges (top-down
  or left-right per surface default).
- Layout runs on graph load (initial projection) + on explicit "Re-layout"
  action.
- Manual drags update node positions in local state and suppress auto-layout
  until the next re-layout action (per-node `isManualPosition` flag or a
  surface-level `layoutDirty` boolean).
- Group/parent nodes (volumes, inner-graph groups) participate in the layout as
  compound nodes; children are laid out within the parent bounds.
- Layout is **session-ephemeral** — not written back to the daemon in V1.114.

## Verification

- Existing canvas tests (Strategy, Outline, World KB, Relationships) all pass —
  no domain-behavior regression.
- New tests: adapter projection correctness, auto-layout produces valid
  positions, manual-override suppresses re-layout, "Re-layout" action works.
- **Dagre compound-graph profiling gate** (architect flag): profile dagre with a
  50-node compound graph (Strategy inner-graphs, outline volumes with 30+
  chapter children) before P0 Done. If layout latency exceeds 200ms, cap visible
  nodes or lazy-expand subgraphs; record the threshold as a residual if breached.
- Human smoke: a 20-chapter novel outline + a 30-entity world KB auto-layout
  into readable graphs without mandatory manual dragging.
