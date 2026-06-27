# Canvas Strategy Surface — Specification

| Attribute | Value |
| --- | --- |
| **Status** | **Shipped α (V1.70)** — Strategy read + visualization + live overlay + Idea-steer shipped; write-boundary operation DTOs + node-granular edits remain **Draft (V1.71)** |
| **Document class** | Draft overlay |
| **Scope** | Product vision + Draft architecture for the human-facing **Canvas** control surfaces: Strategy (Preset) orchestration graph, Work outline + timeline graph, World KB graph; React Flow rendering; the "AI owns prose, human steers via Canvas" thesis; node-granular write boundaries; canvas token contract for DESIGN.md placeholders |
| **Coordinates with** | [orchestration-engine.md](orchestration-engine.md) (strategy = graph-of-graphs), [web-ui.md](web-ui.md) (§15 V1.67 stage + V1.68 canvas roadmap), [local-api-surface-conventions.md](local-api-surface-conventions.md), [chapter-content-local-api.md](chapter-content-local-api.md), [daemon-runtime.md](daemon-runtime.md) |
| **Supersedes** | [body-editor.md](body-editor.md) (archived: [../../archived/knowledge/body-editor.md](../../archived/knowledge/body-editor.md)) |
| **Authored** | V1.67 Phase 2b re-discussion — **@architect** (architecture + React Flow feasibility + DAG↔canvas mapping + write boundary) + **@product-manager** (product thesis + canvas UX + Strategy terminology); PM-scaffolded stub pending authoring |

> **Promoted to Draft (2026-06-27 V1.69 P0).** The V1.67 Exploration was promoted to Draft by `@architect` for interface contracts, structured write boundary, and canvas-token contract. Product/UX thesis from the original `@product-manager` contribution remains in §4. This Draft intentionally stops short of schema/codegen or React Flow implementation authority.

> **Promoted to Shipped α (V1.70).** The V1.70 compass ([`v1.70-canvas-strategy-implement-and-ci-optimization-compass-v1.md`](../../iterations/v1.70-canvas-strategy-implement-and-ci-optimization-compass-v1.md)) shipped the first Strategy Canvas slice: read-only Strategy graph projection, canvas visualization, live execution overlay, and Idea-steer affordance. Implementation provenance: parent `079f687f`; feature commits `81cb4256`, `f82bcdd3`, `10edf22f`, `dad35736` on `feature/v1.70-canvas-strategy-read`, merged into `iteration/v1.70`. This promotion is scoped to the α read/overlay/steer slice only; structured write-boundary DTOs, node-granular editing, outline+timeline canvas, and World KB canvas remain Draft for V1.71+.

## 1. Product thesis (LOCKED from user re-discussion, 2026-06-26)

Nexus is an **AI-autonomous creative executor** (in the spirit of Codex / a design tool): the human **inputs an Idea** and **steers** the work; the **AI owns the prose writing and execution**. Nexus is **not** a manual editor where the human writes chapter bodies by hand.

The human steers through three **Canvas (infinite-canvas) surfaces**, not document editors:

1. **Strategy (Preset) orchestration canvas** — visualize and edit the preset/strategy that drives the creative workflow. Conceptual rename: **"Preset" → "策略 (Strategy)"** — it is the workflow that drives the creative work (this is already the orchestration engine's mental model: a strategy is a hierarchical state-machine of inner DAGs — graph-of-graphs; `orchestration-engine.md` §3).
2. **Work outline + timeline canvas** — compile and steer the Work's outline and timeline as a graph, not a linear rich-text document.
3. **World KB canvas** — browse and steer the World Knowledge Base (entities, events, rules, relationships) as a graph.

**Renderer**: [React Flow](https://reactflow.dev/learn) (`@xyflow/react`) — chosen because a Strategy **is** already a graph/DAG at runtime (states + edges + converge merge points), so React Flow's node/edge model is a natural projection, not a forced fit.

### 1.1 V1.70 α shipped slice

V1.70 promoted only the **Strategy Canvas α** subset from design input to shipped product behavior:

- **Read + visualization**: Strategy/preset graph data is projected into a canvas surface for author comprehension.
- **Live overlay**: runtime/session status is visualized over the Strategy graph so the author can see current, completed, waiting, and error states in context.
- **Idea-steer**: the author can use an Idea-oriented steering affordance to direct Nexus without turning the canvas into a manual prose editor.

The shipped α slice does **not** promote canvas writes. Rewiring Strategy edges, patching Strategy state fields, node-granular outline/timeline editing, and World KB graph edits remain V1.71+ Draft scope until schema/codegen-backed DTOs and daemon-side persistence contracts are explicitly promoted.

## 2. Core architectural principle (LOCKED)

> **Visualization products must not edit raw files directly.** All edits are structured / node-granular operations through the canvas, to avoid accidentally corrupting file structure. Rich-text (TipTap) survives as an **in-node** editing capability (editing the content of a single canvas node), **not** as a whole-document editor.

Implications:
- The shipped V1.65 whole-document outline rich-text editor (TipTap over `outline_path`) is itself a **canvas-pivot candidate** (V1.68+ target, recorded here; **not** changed in V1.67 — no regression to shipped surface).
- The Local API write boundary for canvas surfaces is **structured/node-granular**, not whole-file PUT. (V1.68 design; this Exploration records the principle.)

## 3. Architecture + feasibility

### 3.1 React Flow feasibility

Use **React Flow v12+ via `@xyflow/react`** as the canvas renderer. Context7 lookup against the current React Flow docs (`reactflow.dev` / `xyflow` package docs, v12 line) confirms the APIs this design needs:

| Need | React Flow fit | Notes |
| --- | --- | --- |
| Custom graph elements | `ReactFlow` takes `nodes`, `edges`, `nodeTypes`, and `edgeTypes`; custom nodes receive `NodeProps`; connection points are rendered with `<Handle type="source|target" position={Position.*} id="..." />`. | Fits Strategy states, Converge joins, outline/timeline nodes, and World KB nodes without inventing a custom canvas engine. |
| Edge rewiring | `onConnect`, `onNodesChange`, `onEdgesChange`, and `addEdge` are first-class controlled-state hooks. | UI edits must still be validated by the daemon before persistence; client state is a draft projection, not the source of truth. |
| Graph-of-graphs / sub-flows | React Flow supports parent-child/group nodes using `type: "group"`, `parentId`, `extent: "parent"`, and nested child nodes; docs show nested sub-flow examples with grouped nodes. | Good fit for the orchestration model's outer state machine + inner DAG graph (§3.2). Limitation: React Flow provides visual grouping/nesting, not semantic graph validation; the daemon/preset validator remains authoritative. |
| Accessibility baseline | Current docs expose `nodesFocusable`, per-node `focusable`, `disableKeyboardA11y`, `ariaLabelConfig`, keyboard selection/movement, and focusable nodes/edges. | Adequate baseline, but Nexus must add product-specific keyboard flows and screen-reader summaries (§4.4). |

`apps/web/package.json` confirms **React Flow is not yet installed** and **TipTap is already present** (`@tiptap/react`, `@tiptap/starter-kit`, `tiptap-markdown`). Therefore V1.68 would add `@xyflow/react` as a new dependency and keep TipTap only for rich content **inside a node**, not for whole-document editing.

Feasibility across the two shipped UI containers:

- **Browser tab (Vite SPA)** — React Flow is a DOM/React library, compatible with the current React 18 + Vite stack in `apps/web/package.json`. There is no SSR path in this repo; nevertheless React Flow should be imported only in browser-rendered routes/components because it depends on DOM sizing/interaction.
- **Tauri v2 macOS desktop shell** — the shell loads the same `apps/web/dist` in a system webview (`web-ui.md` §14). On macOS that means WKWebView. React Flow's interaction model is standard DOM/SVG/HTML pointer + keyboard work, so it should run in the WKWebView as the same SPA. V1.68 must still smoke-test drag, wheel/pinch zoom, focus rings, and clipboard/keyboard shortcuts inside the Tauri shell because desktop webviews can differ from Chromium in gesture details. (V1.68 implement decision)
- **Bundle/performance** — React Flow is a significant interactive UI dependency. It should be route-split behind the canvas routes, not pulled into the Control Room bootstrap. Large Work/World graphs need lazy detail panes, filtered projections, and possibly virtualized side panels; React Flow renders graph DOM/SVG elements, so the first implementation should cap visible nodes and progressively expand subgraphs rather than attempting to render an entire World at once. (V1.68 implement decision)

### 3.2 Strategy-DAG ↔ React Flow mapping

The mapping is a projection of the runtime model, not a separate design language. `orchestration-engine.md` defines the Strategy shape as a **graph-of-graphs**: an outer state machine and inner DAG graphs (§1.2, §3.4), and a strategy tick loads a preset bundle, opens/resumes a session, runs one step, possibly launches a child session for an inner graph, and persists after each step (§3.3).

| Runtime concept | Source | Canvas projection | Notes |
| --- | --- | --- | --- |
| Preset / Strategy bundle | `preset.yaml` (`orchestration-engine.md` §7.2) | Canvas document root / graph metadata | UI label is **Strategy**; persisted object remains `preset` until a breaking CLI/schema rename is authorized. |
| Outer state-machine state | `states[].id`, `enter`, `exit_when`, `next` | React Flow node | Node type varies by state kind: prompt/capability/manual-wait/judge/rule/timer/inner-graph/terminal. |
| Outer transition | linear `next`, labeled `next`, expression `branches`, default target | React Flow edge | Edge labels show condition/label/default. Edges remain draft UI until daemon validation accepts them. |
| Converge merge-point state | `converge.strategy` in `orchestration-engine.md` §7.5 and `preset-conditional-routing.md` §3.3.3 | Join node | `wait_for_all`, `first_completed`, and `any` become visible join semantics. The user can see why a branch is waiting. Note: the engine's canonical values are `first_completed` / `any`; the UI should display those and may explain them as "wait for first/any". |
| Inner DAG per state | `inner_graphs.<name>.nodes[].depends_on`, `output_binding` | Nested React Flow sub-flow / group node | A state that launches an inner graph expands into a group/sub-flow; its child nodes represent prompt/tool/capability steps. Parent/child node nesting (`parentId`, group nodes) matches this graph-of-graphs projection. |
| Live execution state | `orchestration_sessions`, child sessions, current task/status/context | Runtime overlay on graph | Highlights current node, completed paths, paused/waiting/error states, and child-session progress. |

**Data source.** The static canvas is fed by the Strategy definition (preset YAML bundle: `states`, `inner_graphs`, prompt/template references). The live overlay is fed by session state from the daemon (`orchestration-engine.md` §3.3, §4.2; `web-ui.md` §5 `NexusClient` boundary). V1.67 promotes preset get/update/delete methods on the TS client, but this Exploration does **not** assert that the Local API already exposes the exact graph document shape or session detail needed by the canvas. V1.68 should add or promote read endpoints such as "get Strategy graph projection" and "get session graph overlay" if the existing preset/session detail endpoints are too YAML/raw or too summary-only. (V1.68 implement decision)

### 3.3 Three canvas surfaces

All three surfaces should share a **Canvas Shell** and specialize by data adapter + node/edge registry:

- Shared shell: React Flow provider, pan/zoom controls, minimap/overview, selection model, command palette, side inspector, validation/errors panel, dirty-state guard, keyboard shortcuts, screen-reader graph summary, and `NexusClient` transport injection (`web-ui.md` §5).
- Per-surface adapters: convert Local API domain DTOs into `nodes`/`edges`, and convert user edits into structured operations (§3.4). No surface may read/write files directly from the browser/Tauri webview.

| Surface | Graph nodes | Graph edges | Custom node types | Primary Local API needs |
| --- | --- | --- | --- | --- |
| **Strategy (Preset) editor** | Outer states; nested inner-graph steps; Converge join nodes; terminal nodes | Linear, labeled, expression/default, converge incoming/outgoing, inner `depends_on` | State node, join node, inner-graph group, prompt/capability node, manual-wait node, terminal node | Preset list/detail/update/delete/validate; session list/detail for live overlay; capability list for node configuration. |
| **Work outline + timeline** | Work, volume, chapter, scene/beat, timeline event, foreshadowing/index item | Contains/ordered-after, references, foreshadows, belongs-to-volume, event→chapter realization | Volume lane, chapter card, event node, dependency/foreshadow node, in-node TipTap outline editor | Work/detail, chapter list/detail, outline read/structured patch, structure patch, timeline/index read/patch. The shipped V1.65 outline is a linear rich-text document (`web-ui.md` §13); the canvas projection turns headings/chapters/events into addressable graph nodes instead of replacing the underlying Work model. |
| **World KB** | World, KeyBlock/entity, event, rule, location, organization, computable block, pending extraction candidate | Relationship/reference, source-anchor, timeline membership, rule-applies-to, promotion candidate→confirmed KeyBlock | Entity card, relationship edge, pending-candidate node, source-anchor node, computable-state badge | World detail; KB query/list/detail; pending/confirmed/rejected promotion state; adopt/reject/merge/update. Grounding: `entity-scope-model.md` §1–§2 defines World-owned narrative KB assets; §5.5 defines the World KB promotion state machine. |

### 3.4 Draft interface contracts (B2) — α read projection shipped; operation DTOs remain illustrative

The V1.70 α implementation treats React Flow as a presentation and interaction model over domain-owned graph projections for the shipped Strategy read/overlay/Idea-steer slice. The graph-document shape below remains useful as the design language for projections, but these contracts are still **paper contracts** for write promotion: operation names such as `strategy.patch_state`, `strategy.patch_transition`, `strategy.patch_prompt_template`, `work.patch_outline_graph`, and `world_kb.patch_relationship` are illustrative until a V1.71 wire-contract plan promotes schema/codegen-backed DTOs and Local API routes.

#### Shared React Flow document shape

All three surfaces use one shell-level graph envelope before conversion to `@xyflow/react` `nodes` and `edges`:

```ts
type CanvasSurfaceKind = "strategy" | "work-outline-timeline" | "world-kb";

interface CanvasGraphDocument<NodeData, EdgeData> {
  surface: CanvasSurfaceKind;
  graphId: string;
  version: string;
  nodes: Array<CanvasNode<NodeData>>;
  edges: Array<CanvasEdge<EdgeData>>;
  viewport?: { x: number; y: number; zoom: number };
  validation: CanvasValidationSummary;
  liveOverlay?: CanvasLiveOverlay;
}

interface CanvasNode<TData> {
  id: string;
  type: string;
  position: { x: number; y: number };
  data: TData;
  parentId?: string;
  extent?: "parent";
  draggable?: boolean;
  selectable?: boolean;
  focusable?: boolean;
}

interface CanvasEdge<TData> {
  id: string;
  type: string;
  source: string;
  sourceHandle?: string;
  target: string;
  targetHandle?: string;
  label?: string;
  data: TData;
  selectable?: boolean;
  focusable?: boolean;
}
```

The shell owns React Flow provider state, viewport, selection, dirty state, accessibility summaries, minimap/controls, command palette, validation panel, side inspector, and transport injection via the existing `NexusClient` boundary. Per-surface adapters own domain DTO projection into these node/edge arrays.

#### Surface-specific node/edge schema

| Surface | Node data contract | Edge data contract | Notes |
| --- | --- | --- | --- |
| Strategy (Preset) | `StrategyNodeData = { stateId, label, stateKind, presetId, innerGraphId?, status?, promptRef?, capabilityRef?, validation[] }` | `StrategyEdgeData = { transitionKind: "next" | "branch" | "default" | "converge" | "depends_on", condition?, convergeStrategy? }` | UI label is Strategy; persisted identifiers remain preset/runtime names until a breaking rename plan. |
| Work outline + timeline | `WorkNodeData = { workId, nodeKind: "work" | "volume" | "chapter" | "scene" | "beat" | "timeline_event" | "foreshadow", title, status?, path?, tiptapFragment? }` | `WorkEdgeData = { relation: "contains" | "ordered_after" | "references" | "foreshadows" | "belongs_to_volume" | "realizes_event" }` | TipTap is allowed only inside a selected node/fragment, not as whole-document editing. |
| World KB | `WorldKbNodeData = { worldId, keyBlockId?, candidateId?, entityKind, name, lifecycle: "pending" | "confirmed" | "rejected" | "merged", sourceAnchors[] }` | `WorldKbEdgeData = { relationType, confidence?, sourceAnchorIds[], promotionState? }` | Promotion state follows the World KB lifecycle in `entity-scope-model.md` §5.5. |

#### State model

The shared shell state is intentionally UI-local until a structured operation is accepted by the daemon:

- `selectedNodeIds` / `selectedEdgeIds`: inspector and command-palette scope.
- `hoveredNodeId` / `hoveredEdgeId`: transient highlight only.
- `collapsedGroupNodeIds`: sub-flow visibility; collapse does not remove canonical children.
- `draftOperations`: ordered client-side operations pending validation/save.
- `validationByElementId`: daemon and client validation mirrored in graph and side panel.
- `liveOverlay`: execution progress, current node, paused/waiting/error states, and child-session status.

#### Sub-flow nesting model

Strategy is a graph-of-graphs per `orchestration-engine.md` §3: outer Strategy states can launch inner DAGs. React Flow group nodes model this without changing engine semantics:

- Outer states are top-level nodes.
- An `inner_graph` state expands into a group node (`type: "strategy-inner-graph-group"`).
- Inner DAG steps are child nodes with `parentId` set to the group node and `extent: "parent"`.
- Inner `depends_on` edges remain inside the group; outer transitions connect to the group/state boundary.
- Collapse hides the child nodes visually but keeps validation and execution status summarized on the group.

The same mechanism can group volumes/chapters in Work and entity clusters in World KB, but Strategy is the canonical nested-flow case.

#### Browser tab and Tauri WKWebView parity

The canvas must run in both the daemon-served browser SPA and the Tauri macOS shell that embeds the same `apps/web/dist`. V1.70 smoke tests must cover drag, pan/zoom, wheel/pinch gestures, keyboard focus movement, clipboard shortcuts, and inspector focus return in Chromium-like browsers and WKWebView. Any desktop-only filesystem action still routes through Tauri/native capabilities and structured daemon operations; the canvas webview never reads or writes raw local files directly.

### 3.5 Structured write boundary (B3) — **V1.71+ Draft — NOT shipped at α**

The locked rule in §2 becomes this implementation principle: **canvas edits produce structured domain operations; the daemon applies them atomically; the UI never mutates raw files.**

Concrete shape:

```text
React Flow draft edit
  → typed canvas operation
    → NexusClient method
      → daemon validates against domain/preset semantics
        → daemon applies atomic persistence (DB tx and/or temp+rename+fsync file write)
          → UI refetches canonical graph projection
```

Examples (operation names are illustrative until a V1.71+ wire-contract plan promotes them):

| User action | Structured operation shape | Daemon persistence target |
| --- | --- | --- |
| Rename Strategy state | `strategy.patch_state({ strategy_id, state_id, set: { label, description } })` | Updates `preset.yaml` through the preset bundle writer; validates ids/reachability before commit. |
| Rewire Strategy edge | `strategy.patch_transition({ source_state_id, old_target, new_target, condition })` | Rewrites the structured `next`/`branches` field; runs preset semantic validation before commit. |
| Edit prompt text inside a node | `strategy.patch_prompt_template({ node_id, template_patch })` | Applies a template-scoped write; TipTap/Markdown round-trip is limited to that prompt/node content. |
| Move chapter under volume / attach event | `work.patch_outline_graph({ op: "move_chapter" | "link_event", ... })` | Updates outline/index/DB metadata via a structured writer; no whole-document outline PUT from the canvas. |
| Adopt World KB candidate / edit relationship | `world_kb.adopt_candidate(...)`, `world_kb.patch_relationship(...)` | Updates `kb_extract_jobs` / `kb_key_blocks` under the World KB state machine (`entity-scope-model.md` §5.5). |

This supersedes the V1.65 whole-file outline PUT model for the future canvas surface: V1.65 could save a whole outline document because the UI was a document editor (`web-ui.md` §13.1, §13.5). The canvas model must instead address and validate individual nodes/edges. The daemon should continue to use the existing durability pattern established elsewhere in the repo (atomic temp write + rename + directory fsync, DB transactions, guarded paths); the open design is which exact operation DTOs become schema-backed Local API contracts. (V1.70 implement decision)

#### Conflict policy vs host tool body writes

Orchestration may write prose or artifacts through host-tool paths such as `host_tool_handlers.rs` `body_path`. The canvas must not concurrently mutate those same raw files. Draft policy:

1. Canvas saves carry a base revision (`graphRevision`, `nodeRevision`, or equivalent domain version) from the last canonical projection.
2. Daemon rejects stale node/edge operations with a structured conflict error that identifies the changed node/file/object and recovery action.
3. UI keeps the user's draft operation list, refetches the canonical graph, and offers reapply/merge at node granularity where safe.
4. If orchestration is actively writing a node/body artifact, canvas editing for that node is read-only with a clear status label (`Nexus is writing this node…`).
5. Raw `body_path` conflicts are never resolved in the browser by loading and overwriting files; they are resolved by daemon-owned structured merges or explicit retry after refetch.

TipTap remains useful as an in-node editor for prompt snippets, outline fragments, notes, or constraints. It is not a whole-document manuscript editor and must not bypass the operation boundary.

### 3.6 Canvas → DESIGN.md token contract (B4)

V1.69 freezes the minimal credible token names that V1.70 canvas implementation will need. `apps/web/DESIGN.md` and `apps/web/DESIGN.dark.md` stub these as commented LEVEL placeholders; V1.70 assigns concrete values when implementing the canvas.

| Token | Intent |
| --- | --- |
| `canvas-surface` | Infinite-canvas background behind graph nodes; distinct from cards/page background so grid and selection remain visible. |
| `canvas-grid` | Subtle grid/dot/guide color on `canvas-surface`; must pass reduced-contrast needs without visual noise. |
| `canvas-node-fill` | Default node card fill for Strategy, Work, and World KB nodes. |
| `canvas-node-fill-hover` | Hover/focus-adjacent node fill for pointer and keyboard discovery. |
| `canvas-node-border` | Default node outline, including collapsed sub-flow group boundaries. |
| `canvas-node-border-selected` | Selected/focused node outline; must pair with the global focus-ring language and not rely on color alone. |
| `canvas-edge` | Default relationship/transition edge stroke. |
| `canvas-edge-hover` | Hovered/selected edge stroke for rewiring and relationship inspection. |
| `canvas-port` | Handle/port fill and border for connectable source/target points. |
| `canvas-minimap` | Minimap viewport/region color and quiet overview affordances. |
| `canvas-strategy-accent` | Strategy/preset-specific accent for state-machine nodes, inner-graph groups, and Strategy nav affordances; expected to derive from the purple family unless V1.70 changes the palette deliberately. |

These tokens intentionally cover shared canvas primitives only. Surface-specific status still uses existing semantic colors (`green-*`, `amber-*`, `red-*`, `teal-*`, `purple-*`) so the canvas remains consistent with non-canvas dashboard states.

### 3.7 "AI owns prose" execution trigger

The canvas is the **steering surface**, not the prose surface. A human can:

1. Input an **Idea** at the Work/Strategy entry point or on a specific node.
2. Change graph structure or node instructions (e.g., add a research branch, adjust a chapter card, attach a World KB constraint).
3. Ask Nexus to **run / resume / re-run from here**.

Execution then moves to orchestration: the Strategy/preset drives ACP prompts and capabilities, writes prose or structured artifacts through authorized host tools, and persists session state. The UI overlays progress and outputs back on the canvas. Human-authored rich text is limited to steering artifacts (node labels, prompt snippets, outline-node content, notes, constraints); chapter/body prose remains AI-produced unless a future compass explicitly authorizes a manual prose-editing product line.

Open V1.70 design points include the exact trigger verbs (for example, "Run Strategy", "Resume from Node", "Regenerate Branch", "Apply Idea to Node"), whether triggers enqueue schedule runs or call a direct orchestration advance endpoint, and how rollback/preview is shown before generated prose is committed. (V1.70 implement decision)

### 3.8 Relationship to V1.67 Local API convergence

The canvas is a heavy Local API consumer: every graph node binds to list/detail data, every inspector needs typed update operations, and every execution overlay depends on consistent session/status responses. Therefore V1.67 P0 is not incidental hygiene; it is the foundation for V1.68 canvas work:

- **F-P3 `items` convergence** gives graph adapters one list shape across Works, sessions, schedules, capabilities, and future graph-supporting endpoints.
- **FE1-ORCH error envelope convergence** gives canvas validation, save, and execution toasts one parseable error surface instead of per-handler exceptions.
- **F-F1 sort convergence** makes node pickers and side panels deterministic (chapters, sessions, capabilities, presets) without bespoke client sorting.

The Canvas Shell must keep the `web-ui.md` §5 transport invariant: React components depend on `NexusClient`, not `fetch`, Tauri `invoke`, or raw filesystem access.

## 4. Product / UX

*Pure product-voice user stories remain owned by `@product-manager`; this section records the technical/UX architecture that constrains that copy.*

### 4.1 Idea-input affordance architecture

- The **Idea input** is a persistent canvas affordance, not a document body field. It can appear as a global entry control (start or steer the Work) and as a contextual node action (apply an idea to this Strategy state / chapter / KB item).
- Submitting an Idea creates a structured steering event: `idea_text`, target scope (`strategy`, `work`, `node`, `world_kb_item`), optional selected nodes/edges, and desired action (`explore`, `revise_plan`, `run`, `resume`). The daemon/orchestration layer decides how that event becomes prompt input or session signal. (V1.68 implement decision)
- The UI must make the authorship boundary explicit: the user is giving direction; Nexus will execute and write prose through orchestration. Labels should prefer verbs like **Steer**, **Run**, **Ask Nexus to revise**, and **Apply idea to this node** over **Edit body** or **Write chapter manually**.
- Idea submissions should land in the graph as visible, reviewable steering artifacts (e.g., a note badge, pending instruction, or session input node) so the user can understand why the AI did something later. The exact persistence model is open. (V1.68 implement decision)

### 4.2 Strategy terminology adoption scope

- In UI and specs, use **Strategy / 策略** for the human-facing concept: the workflow that drives creation.
- Keep runtime/file/CLI identifiers as **preset** in V1.67 and until an explicit breaking-change plan authorizes a rename. This includes preset YAML, existing Local API routes, generated DTO names, and CLI command names.
- UI copy can bridge the terms during transition: **Strategy (preset)** on first mention, then **Strategy** in navigation and screen titles. Developer-facing inspectors may show `preset_id` as metadata to avoid hiding the underlying contract.
- A future CLI/schema rename is a separate breaking design and migration task. (V1.68+ implement decision)

### 4.3 Per-surface UX architecture

| Surface | Primary author task | Canvas UX shape | Inspector / details |
| --- | --- | --- | --- |
| Strategy | Understand and steer how Nexus executes creative work | Top-level state-machine graph with expandable inner DAG groups; join nodes make waiting/merge semantics visible. | State settings, prompt/template snippets, capability requirements, validation diagnostics, live session overlay. |
| Outline + timeline | Shape the Work without manually writing final prose | Volumes/chapters/events as graph nodes; timeline/foreshadow/reference edges show structure that a linear outline hides. | In-node TipTap for outline fragments only; structure fields; status; generated-output links/read-only preview. |
| World KB | Inspect and steer continuity constraints | Entity/event/rule graph with relationship edges and promotion-state badges. | KeyBlock detail, source anchors, pending/confirmed/rejected state, adopt/reject/merge actions. |

### 4.4 Accessibility of a graph surface

React Flow provides a baseline (`nodesFocusable`, keyboard selection/movement, focusable nodes/edges, `ariaLabelConfig`), but Nexus must design an accessible graph experience rather than relying on pointer-only spatial navigation.

Concrete requirements for the Draft:

1. **Keyboard-first traversal** — `Tab` reaches the canvas, selected nodes, edge list/relationship list, inspector, minimap/controls, and validation panel in a predictable order. Arrow-key movement must not conflict with page scroll; provide explicit "move selected node" mode or documented shortcuts. (V1.68 implement decision)
2. **Non-spatial alternate views** — every canvas must have a list/tree/table companion: Strategy states in execution order + branch table, outline chapters/events as sortable lists, World KB items/relationships as searchable tables. This is both accessibility and productivity.
3. **Screen-reader summaries** — expose graph-level summaries via ARIA/live regions: node count, selected node label/type/status, edge count, validation errors, current execution node, and Converge wait state (e.g., "Join waiting for 2 of 3 branches"). Use `ariaLabelConfig` for localized/control labels.
4. **Focus management** — opening a node inspector moves focus to the inspector heading; closing returns focus to the originating node. Validation errors focus the first failing node and mirror the error in the side panel so color/position are not the only cues.
5. **Pointer alternatives** — edge creation/rewiring must have a keyboard/dialog path (choose source node → choose target node → choose edge kind/condition) in addition to drag handles.
6. **Motion and zoom discipline** — honor reduced-motion preferences for animated edges/auto-layout transitions; maintain visible focus rings at all zoom levels; do not encode state only by edge color.

### 4.5 User stories (steering loop)

The author **directs an autonomous executor**; they do not write alongside an assistant. (Pure manual body writing is intentionally absent — the AI owns prose.)

- **Steer by Idea** — *As an author*, I express an Idea (Work-level or on a specific node) and choose **Steer / Run / Ask Nexus to revise**, then Nexus executes — drafting prose, advancing the chapter, updating the KB — so I direct the work without typing the body myself.
- **Read the Strategy as a graph** — *As an author*, I see my Strategy (preset) rendered as a state-machine graph with visible join/wait nodes, so I understand how Nexus will execute my Work before it runs — and I can rewire a branch or adjust a gate on the canvas.
- **Shape the outline/timeline spatially** — *As an author*, I shape volumes/chapters/events as graph nodes with timeline/foreshadow edges, so the structure that a linear outline hides becomes visible and editable — and I steer Nexus to (re)draft the node I point it at.
- **Steer World KB continuity** — *As an author*, I browse entities/events/rules as a relationship graph with promotion-state badges, and adopt/reject/merge from the canvas, so continuity constraints stay coherent as the Work grows.
- **Review AI execution on the canvas** — *As an author*, after Nexus executes, I see what changed on the canvas (node status, generated-output links, pending instructions) and review the result read-only, so I stay in command of an autonomous process.

## 5. Non-goals (V1.70 α)

- No promotion of canvas **writes** in V1.70 α. The shipped slice is read + visualization + live overlay + Idea-steer only.
- No schema/codegen/DTO lock for write operations in V1.70 α. Operation names and TypeScript-like interfaces above remain illustrative paper contracts until V1.71.
- No promotion of the outline+timeline canvas or World KB canvas in V1.70 α; both remain V1.71+ Draft surfaces.
- No removal/regression of the shipped V1.65 outline editor; canvas-pivot and node-granular outline edits are V1.71+ Draft scope.
- No CLI/spec rename of `preset` → `strategy` (breaking; deferred). V1.70 α adopts the terminology in UI/spec wording only.

## 6. Roadmap (durable tracking)

- **V1.70 α — shipped**: Strategy Canvas read projection + visualization + live overlay + Idea-steer. This is the shipped successor slice to the retired body-editor roadmap, not the full three-surface canvas program.
- **V1.71 — Draft target**: promote the structured write boundary and node-granular Strategy edits (`strategy.patch_state`, `strategy.patch_transition`, prompt-template patching, validation/conflict DTOs) through real schemas/codegen and daemon-owned persistence contracts.
- **V1.71+ — Draft target**: outline+timeline canvas and World KB canvas remain future surfaces. They should reuse the Canvas Shell and the structured operation boundary only after their domain DTOs, validation rules, and persistence ownership are explicitly promoted.
