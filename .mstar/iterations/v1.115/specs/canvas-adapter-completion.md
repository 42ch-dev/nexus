# Canvas Adapter Completion & Contract Convergence (V1.115 P0)

> Iteration-scoped product/tech brief for V1.115 P0. Not a normative `{SPECS_DIR}`
> Master — consumes `.mstar/specs/canvas-strategy-surface.md` §3.3 / §3.3.1 and
> `.mstar/iterations/v1.114/specs/canvas-architecture-foundation.md`.

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-13-v1.115-canvas-adapter-completion` |
| **Tier** | Must |
| **Audience** | Authors (no domain-behavior regression) + maintainers (honest reusable adapter) |

## Problem framing

V1.114 proved the `CanvasSurfaceAdapter` + `useCanvasSurface()` recipe on two
orchestrators (Strategy + World KB entities). The abstraction is **not yet
honest or complete**:

1. **Incomplete coverage** — Outline+Timeline still re-wires shell boilerplate
   outside the adapter, so a 5th surface or canvas↔compute intersection still
   has two recipes (adapter path vs Outline-era path).
2. **Contract leaks** — Strategy `projectGraph` is a passthrough (real
   projection lives upstream); World KB `renderInspector` / `renderAltView`
   ignore the declared `node` parameter; `useAutoLayout` first-run clobbers any
   supplied positions and uses non-null assertions on dagre labels.

Until both are fixed, “use the adapter” is marketing copy, not a maintainable
contract.

## User value

| Who | Why they care |
| --- | --- |
| **Authors** | Opening Outline+Timeline after migration must feel identical — same graph, inspectors, conflicts, alt-view. No new capability this plan; zero domain regression is the product promise. |
| **Maintainers / next-iteration implementers** | A 5th surface (compute graph, session replay) or Strategy `onConnect` deep-dive can copy one recipe. Contract methods mean what they say (`projectGraph` projects; `renderInspector(node)` uses `node`). |
| **Future layout persistence** | Surfaces that later supply positions will not have them wiped on first open (W003). |

## Surface inventory (product vs kind — locked)

Normative product surfaces remain **three** (`canvas-strategy-surface.md` §3.3):
Strategy, Outline+Timeline, World KB.

`CanvasSurfaceKind` currently has **four** keys for viewport/cache identity:

| Kind | Orchestrator today | V1.115 P0 action |
| --- | --- | --- |
| `strategy` | `strategy-canvas.tsx` (on adapter) | Fix W001 passthrough |
| `world-kb-entities` | `world-kb-canvas.tsx` (on adapter) | Fix W002 node-param leak |
| `outline` | `outline-canvas.tsx` (**not** on adapter) | **Migrate** to adapter |
| `world-kb-relationships` | No separate orchestrator (Relationships live as World KB edges + alt-view table) | **Do not invent a second World KB surface** this iteration. Kind stays reserved as a viewport/cache identity key, not a product surface. Document in T5 knowledge note that a dedicated orchestrator can adopt it later if a relationships route is ever built. |

**Done for “every shipped surface is on the adapter”** means: every **shipped
canvas orchestrator** (Strategy, World KB, Outline+Timeline) consumes
`useCanvasSurface()` + a real adapter. Do not inflate scope to implement a
phantom Relationships canvas.

## Goals

1. Migrate Outline+Timeline to `OutlineCanvasAdapter` + `useCanvasSurface()`.
2. Make Strategy adapter `projectGraph` the projection owner (W001).
3. Make World KB `renderInspector` / `renderAltView` honor the contract node
   parameter (W002); audit both inspector and alt-view paths on that adapter.
4. Preserve supplied positions on first open when present (W003); defensive
   dagre fallback (M001).
5. Update the canvas implementation-pattern knowledge note (layer 14) only —
   no new normative Master promotion.

## Non-goals

- New canvas product surface (compute graph, session replay, 5th orchestrator)
- Separate World KB Relationships orchestrator / route
- Strategy inner-graph `onConnect` deep-dive
- Persisted layout positions to the daemon
- ELK / alternate layout engines
- Domain write-op or node-semantics changes

## Target state

- Three product canvas orchestrators share one composition path.
- Adapter methods are non-decorative: projection and inspector routing live
  where the interface claims they live.
- Existing canvas tests remain green; new tests cover Outline adapter
  projection equivalence + W001–W003 / M001 behaviors.

## Acceptance criteria (author/maintainer-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P0-1** | Outline+Timeline opens and behaves with no domain-behavior regression after consuming `useCanvasSurface()` + `OutlineCanvasAdapter` | Existing Outline+Timeline tests pass; before/after projection of the same input graph is identical (nodes/edges/`parentId` nesting) |
| **AC-P0-2** | Strategy `projectGraph` performs projection (not a passthrough); `useStrategyCanvas` does not own a second projection path | Code inspection + diff-test: same preset → same nodes/edges; Strategy projection tests target the adapter |
| **AC-P0-3** | World KB `renderInspector(node)` / `renderAltView` use the contract inputs (no inspector side-channel that ignores `node`) | Inspector renders the entity for the passed node; World KB inspector tests pass + regression for selected-node routing |
| **AC-P0-4** | Surfaces that supply meaningful positions on first open keep them; surfaces without positions still auto-layout on open | Unit tests for both branches of `useAutoLayout` first-run |
| **AC-P0-5** | `useAutoLayout` / dagre path has no non-null assertions on labels; missing label falls back safely | Unit test: missing dagre label does not throw |
| **AC-P0-6** | All shipped canvas **orchestrators** (Strategy, World KB, Outline+Timeline) consume `CanvasSurfaceAdapter` / `useCanvasSurface()` | Grep/code review: no Outline shell re-wiring outside the adapter path |

## Product decisions (locked this seat)

| Decision | Choice | Rationale |
| --- | --- | --- |
| Migration target | Outline+Timeline only as the remaining product orchestrator | Matches §3.3; Relationships is not a separate orchestrator today |
| Task granularity | One product surface (`OutlineCanvasAdapter`); implementer may split internal tasks | Product DoD is “Outline on adapter,” not a multi-surface program |
| W002 scope | World KB adapter inspector + alt-view paths | Same contract leak class; do not invent a Relationships adapter to “fix” W002 |
| Capacity cut order | Prefer thinner Outline migration tasks over dropping W001–W003 | Contract honesty is the iteration story; incomplete Outline migration without leak fixes still leaves a dishonest foundation |

## Architect decisions (Seat 2 — resolved)

| # | Question | Decision | Rationale |
| --- | --- | --- | --- |
| 1 | `world-kb-relationships` kind: keep reserved, document, or remove? | **Keep reserved.** Document in T5 knowledge note that it is a viewport/cache identity key, not a product surface. | Removing it is a breaking `CanvasSurfaceKind` union change with zero benefit. The kind carries semantic meaning (World KB has two projection modes); it costs nothing to keep. A dedicated orchestrator can adopt it later if a relationships route is ever built. |
| 2 | Outline task split: single task vs projection-extract + shell-rewire? | **Split into T1a (adapter extract) + T1b (shell rewire).** | Outline has the most complex projection (parentId/extent nesting for Volume→Chapter→Scene→Beat, scene/beat fixtures, position-merge sync). Two focused SDD review checkpoints. Product DoD unchanged. |
| 3 | W001 boundary: exact move of Strategy projection without timing regression? | **Move `buildStrategyGraph(parsed)` into `adapter.projectGraph`; drop the pre-projected `graph` field from `StrategySurfaceGraph`. Surface `danglingTargets` via `ctxRef.current`.** | Both old and new paths use `useMemo` over the same `parsed` dep → identical timing. `useCanvasSurface` memoizes `projectGraph` exactly as the old hook memoized `buildStrategyGraph`. No double-projection (function runs once per memoized input). |
| 4 | W002 ctxRef: which fields stay legitimate vs must come from `node`? | **`node.data` / `node.type` is the authority for entity-vs-candidate routing. Callbacks + cross-node context (`confirmedEntities`, `reseedSignal`) STAY on ctxRef. `selection` is redundant for node-based routing. Relationship (edge) inspector stays on orchestrator path — known interface gap, do NOT expand this iteration.** | The contract says `renderInspector(node)` uses `node`. Handlers and cross-node context are legitimately orchestrator-owned. Edge selection has no contract method today — adding one is YAGNI until a second edge-inspector surface exists. |

### Selection coordination ownership (architect invariant)

`useCanvasSurface()` owns node selection (`selectedNode` / `selectedNodeId`).
It already provides the position-merge sync effect (preserving dragged
positions across projection rebuilds — `use-canvas-surface.ts` lines 73–83).
Migrated surfaces MUST NOT re-implement either. Surface-specific selection
resolution (e.g., Outline chapter/scene/beat from `selectedNodeId`) stays in
the orchestrator as a thin resolver effect.

## Spec refs

- `.mstar/specs/canvas-strategy-surface.md` §3.3, §3.3.1
- `.mstar/specs/web-ui.md`
- `.mstar/iterations/v1.114/specs/canvas-architecture-foundation.md`
- Residuals: `R-V1114P0QC1-W001`, `R-V1114P0QC1-W002`, `R-V1114P0QC1-W003`,
  `R-V1114P0QC2-M001`
