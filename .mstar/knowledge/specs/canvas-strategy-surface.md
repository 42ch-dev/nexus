# Canvas Strategy Surface — Specification

| Attribute | Value |
| --- | --- |
| **Status** | **Shipped β (V1.74)** — Strategy read + visualization + live overlay + Idea-steer (V1.70), write-boundary operation DTOs + node-granular Strategy edits + conflict policy (V1.71), Outline+Timeline canvas β (Work → Volume → Chapter → Scene/Beat graph projection + timeline lane + foreshadow edges + 3 structured patch routes `outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event` + outlineRevision + structured conflict error + UI retry/merge + non-spatial alternate views) (V1.72), World KB canvas β (World KB graph + candidates projections, 2 structured patch routes `kb.patch_entity` / `kb.promote_candidate`, per-row OCC on `kb_key_blocks.revision` / `kb_extract_jobs.version`, 409/422 structured errors, and 4 Local API routes) (V1.73), and typed World KB relationship editing (schema-backed relationship DTOs, `world_kb.patch_relationship`, `kb_relationships.revision` OCC, directed/symmetric projections, non-spatial relationship table) (V1.74) are shipped. |
| **Document class** | Draft overlay |
| **Scope** | Product vision + Draft architecture for the human-facing **Canvas** control surfaces: Strategy (Preset) orchestration graph, Work outline + timeline graph, World KB graph; React Flow rendering; the "AI owns prose, human steers via Canvas" thesis; node-granular write boundaries; canvas token contract for DESIGN.md placeholders |
| **Coordinates with** | [orchestration-engine.md](orchestration-engine.md) (strategy = graph-of-graphs), [web-ui.md](web-ui.md) (§15 V1.67 stage + V1.68 canvas roadmap), [local-api-surface-conventions.md](local-api-surface-conventions.md), [chapter-content-local-api.md](chapter-content-local-api.md), [daemon-runtime.md](daemon-runtime.md) |
| **Supersedes** | [body-editor.md](body-editor.md) (archived: [../../archived/knowledge/body-editor.md](../../archived/knowledge/body-editor.md)) |
| **Authored** | V1.67 Phase 2b re-discussion — **@architect** (architecture + React Flow feasibility + DAG↔canvas mapping + write boundary) + **@product-manager** (product thesis + canvas UX + Strategy terminology); PM-scaffolded stub pending authoring |

> **Promoted to Draft (2026-06-27 V1.69 P0).** The V1.67 Exploration was promoted to Draft by `@architect` for interface contracts, structured write boundary, and canvas-token contract. Product/UX thesis from the original `@product-manager` contribution remains in §4. This Draft intentionally stops short of schema/codegen or React Flow implementation authority.

> **Promoted to Shipped α (V1.70).** The V1.70 compass ([`v1.70-canvas-strategy-implement-and-ci-optimization-compass-v1.md`](../../iterations/v1.70-canvas-strategy-implement-and-ci-optimization-compass-v1.md)) shipped the first Strategy Canvas slice: read-only Strategy graph projection, canvas visualization, live execution overlay, and Idea-steer affordance. Implementation provenance: parent `079f687f`; feature commits `81cb4256`, `f82bcdd3`, `10edf22f`, `dad35736` on `feature/v1.70-canvas-strategy-read`, merged into `iteration/v1.70`. This promotion is scoped to the α read/overlay/steer slice only; structured write-boundary DTOs, node-granular editing, outline+timeline canvas, and World KB canvas remain Draft for V1.71+.

> **Promoted to Shipped β (V1.71).** The V1.71 compass ([`v1.71-canvas-strategy-write-boundary-and-hygiene-compass-v1.md`](../../iterations/v1.71-canvas-strategy-write-boundary-and-hygiene-compass-v1.md)) ships the Strategy write-boundary slice: schema-backed patch DTOs, 3 node-granular Strategy patch routes, YAML `revision:` graphRevision conflict detection, daemon validation, atomic persistence, and UI retry/merge conflict handling. This promotion is scoped to the Strategy surface only; outline+timeline and World KB canvas surfaces remain Draft for V1.72+.

> **Promoted to Shipped β (V1.72).** The V1.72 compass ([`v1.72-canvas-outline-timeline-beta-and-hygiene-compass-v1.md`](../../iterations/v1.72-canvas-outline-timeline-beta-and-hygiene-compass-v1.md)) ships the Outline+Timeline β slice: schema-backed patch DTOs (`OutlinePatchStructureRequest` / `OutlinePatchChapterRequest` / `TimelinePatchEventRequest` + `OutlinePatchResponse` + `OutlineConflictError` + `OutlineValidationError`), 3 Local API patch routes (structure / chapter / timeline-event), `outline_revision:` markdown frontmatter graphRevision conflict detection, daemon validation (ID existence, structural integrity, status lifecycle, timeline reference resolution, revision precondition), atomic outline markdown persistence (temp + rename + fsync + dir fsync), and UI retry/merge conflict handling with outline-flavored copy + non-spatial alternate views (chapter list + timeline event list). `@42ch/nexus-contracts` 0.7.0 → 0.8.0 (additive outline DTOs). DESIGN.md gains 8 outline/timeline canvas-write tokens (`canvas-outline-volume-fill` + 4 chapter-card statuses + `canvas-outline-timeline-event-pin` + `canvas-outline-foreshadow-edge` + `canvas-outline-timeline-marker` + `canvas-outline-conflict-marker`). This promotion is scoped to the Outline+Timeline surface only; World KB canvas surface remains Draft for V1.73+. Canvas-pivot (retiring V1.65 outline whole-document editor) remains V1.73+ backlog.

> **Promoted to Shipped β (V1.73).** The V1.73 compass ([`v1.73-canvas-world-kb-beta-and-outline-hardening-compass-v1.md`](../../iterations/v1.73-canvas-world-kb-beta-and-outline-hardening-compass-v1.md)) ships the World KB canvas β slice: schema/codegen-backed World KB DTOs, 2 structured patch routes (`POST /v1/local/worlds/{world_id}/kb/patch-entity`, `POST /v1/local/worlds/{world_id}/kb/promote-candidate`), per-row OCC via `expected_version` against `kb_key_blocks.revision` / `kb_extract_jobs.version`, structured 409 `WorldKbConflictError` + 422 `WorldKbValidationError`, and 4 Local API routes including the read projections (`GET /v1/local/worlds/{world_id}/kb/graph`, `GET /v1/local/worlds/{world_id}/kb/candidates`). This promotion is scoped to the World KB entities + candidates surface; typed World KB relationship editing remains V1.74+.

> **Promoted to Shipped β (V1.74).** The V1.74 compass ([`v1.74-world-kb-relationships-and-hygiene-compass-v1.md`](../../iterations/v1.74-world-kb-relationships-and-hygiene-compass-v1.md)) ships the typed World KB relationships β slice: schema-backed relationship DTOs, a single structured patch route (`POST /v1/local/worlds/{world_id}/kb/patch-relationship` with action `add | update | remove`), per-row OCC via `expected_version` against `kb_relationships.revision`, structured 409 `WorldKbConflictError` + 422 `WorldKbValidationError`, and `GET /v1/local/worlds/{world_id}/kb/graph` populated with typed `relationships[]`. This promotion is scoped to first-class relationship editing for the World KB surface; relationship confidence weighting/filtering and automatic relationship extraction remain future work.

## 1. Product thesis (LOCKED from user re-discussion, 2026-06-26)

Nexus is an **AI-autonomous creative executor** (in the spirit of Codex / a design tool): the human **inputs an Idea** and **steers** the work; the **AI owns the prose writing and execution**. Nexus is **not** a manual editor where the human writes chapter bodies by hand.

The human steers through three **Canvas (infinite-canvas) surfaces**, not document editors:

1. **Strategy (Preset) orchestration canvas** — visualize and edit the preset/strategy that drives the creative workflow. Conceptual rename: **"Preset" → "策略 (Strategy)"** — it is the workflow that drives the creative work (this is already the orchestration engine's mental model: a strategy is a hierarchical state-machine of inner DAGs — graph-of-graphs; `orchestration-engine.md` §3).
2. **Work outline + timeline canvas** — compile and steer the Work's outline and timeline as a graph, not a linear rich-text document.
3. **World KB canvas** — browse and steer the World Knowledge Base (entities, events, rules, relationships) as a graph.

**Renderer**: [React Flow](https://reactflow.dev/learn) (`@xyflow/react`) — chosen because a Strategy **is** already a graph/DAG (Directed Acyclic Graph) at runtime (states + edges + converge merge points), so React Flow's node/edge model is a natural projection, not a forced fit.

### 1.1 V1.71 β shipped slice

V1.70 promoted the **Strategy Canvas α** read/overlay/steer subset; V1.71 promotes the **Strategy Canvas β** write-boundary subset from design input to shipped product behavior:

- **Read + visualization**: Strategy/preset graph data is projected into a canvas surface for author comprehension.
- **Live overlay**: runtime/session status is visualized over the Strategy graph so the author can see current, completed, waiting, and error states in context.
- **Idea-steer**: the author can use an Idea-oriented steering affordance to direct Nexus without turning the canvas into a manual prose editor.
- **Node-granular Strategy writes**: the Strategy surface can patch state labels/descriptions, transition/edge conditions and targets, and prompt-template node content through the 3 shipped operations (`strategy.patch_state`, `strategy.patch_transition`, `strategy.patch_prompt_template`).
- **Conflict policy**: each patch carries `base_revision`; the daemon compares it with the YAML `revision:` graphRevision and returns structured conflict errors instead of silent last-write-wins. The UI keeps the draft patch, refetches canonical state, and offers **Use current**, **Reapply my edit**, and **Review side-by-side** (side-by-side enabled only when the draft and canonical changes touch non-overlapping fields).

The shipped β slice still does **not** promote node-granular outline/timeline editing or World KB graph edits. Those surfaces remain Draft until their own domain DTOs, validation rules, persistence ownership, and patch-route contracts are explicitly promoted.

### 1.2 V1.72 β shipped slice

V1.71 promoted the **Strategy Canvas β** write-boundary subset; V1.72 promotes the **Outline+Timeline Canvas β** write-boundary subset:

- **Read + visualization**: Work → Volume → Chapter → Scene/Beat graph projection. Volume lanes render as sub-flows with `parentId`+`extent:parent` children. Chapter cards display `wc`/`slug`/`status` from the outline markdown frontmatter. TipTap fragment preview is **read-only on the canvas** (V1.65 outline editor remains the canonical whole-document editor; canvas is a parallel entry surface per `web-ui.md` §15).
- **Timeline lane**: events across chapters positioned by chapter realization point. `foreshadow` edges link events resolving later. `realizes_event` edges link chapter nodes to events.
- **Node-granular Outline+Timeline writes**: the Outline+Timeline surface can patch chapter structure fields (title, slug, wc, volume binding, status `not_started → outlined → drafted → completed`), timeline events (add_event, remove_event, attach_event_to_chapter, link_foreshadow), and outline structure (move_chapter, link_event, attach_to_volume) through the 3 shipped operations (`outline.patch_structure`, `outline.patch_chapter`, `timeline.patch_event`).
- **Conflict policy**: each patch carries `base_revision`; the daemon compares it with the outline markdown `outline_revision:` frontmatter key and returns structured conflict errors. The UI keeps the draft patch, refetches canonical state, and offers **Use current**, **Reapply my edit**, and **Review side-by-side** (side-by-side enabled only when draft and canonical changes touch non-overlapping fields, disabled for same-field/path or timeline-event content). **Body ownership invariant** (compass §6.4): outline markdown body remains V1.65 editor-owned and is never overwritten by canvas writes — the canvas re-reads body under `RuntimeLockGuard` and preserves it across patch commits.
- **Non-spatial alternate views**: sortable chapter list (title/status/wc/volume/updated) + sortable timeline event list (event/realizes_chapter/foreshadows/updated), toggle from canvas toolbar, default for keyboard-only / screen-reader users.
- **Atomic persistence**: outline markdown write uses temp + rename + fsync + dir fsync; failed validation/conflict does not increment `outline_revision`.

The shipped β slice still does **not** promote node-granular World KB graph edits or canvas-pivot (retiring V1.65 outline whole-document editor). World KB canvas surface remains Draft for V1.73+; canvas-pivot remains V1.73+ backlog.

### 1.3 V1.73 β shipped slice

V1.72 promoted the **Outline+Timeline Canvas β** write-boundary subset; V1.73 promotes the **World KB Canvas β** entities + candidates subset:

- **Read + visualization**: World KB graph projection (`WorldKbGraphResponse`) exposes entities and source-anchor provenance edges; typed `relationships` is reserved and empty in V1.73 pending the V1.74 relationship surface.
- **Candidate workflow**: candidate projection (`WorldKbCandidatesResponse`) supports pending extraction candidates with cursor pagination and the `adopt` / `reject` / `merge` promotion state machine.
- **Node-granular World KB writes**: the World KB surface can patch entity title/body/aliases/block_type and promote candidates through the 2 shipped operations (`kb.patch_entity`, `kb.promote_candidate`).
- **Conflict policy**: each mutating request carries `expected_version`; the daemon compares it with the per-row version (`kb_key_blocks.revision` for entities, `kb_extract_jobs.version` for candidates) and returns 409 `WorldKbConflictError` before mutation on stale writes. Domain-rule failures return 422 `WorldKbValidationError` with `validation_summary`.

The shipped β slice still does **not** promote typed World KB relationship CRUD. Relationship edges render from source-anchor provenance in V1.73; the durable V1.74 target is a first-class World KB relationships surface.

### 1.4 V1.74 β shipped slice

V1.73 promoted the **World KB Canvas β** entities + candidates subset; V1.74 completes the World KB surface with typed relationships:

- **Typed relationship edges**: `WorldKbGraphResponse.relationships[]` now contains `WorldKbRelationshipProjection` items instead of the V1.73 empty reserved array. Each item projects one stored `kb_relationships` row, with `projection_direction` distinguishing the stored direction from a derived symmetric reverse projection.
- **Hybrid taxonomy**: relationship type is a fixed `WorldKbRelationshipKind` core enum (`allied_with`, `opposes`, `parent_of`, `child_of`, `member_of`, `located_in`, `rules_over`, `references`, `serves`, `rival_of`, `mentor_of`, `custom`) plus `custom_label` when `custom` is selected.
- **Directed + symmetric semantics**: storage is a single directed row `(source_entity_id, target_entity_id, relation_type, symmetric)`. When `symmetric=true`, read projection emits a reverse edge that shares `relationship_id`; edits/deletes from either projection target the same stored row.
- **Relationship writes**: the surface can create, update, and remove relationships through `world_kb.patch_relationship`. Stale writes return 409 before mutation; invalid taxonomy, self-loops, invalid anchors, out-of-range confidence, or cross-World entity references return 422.
- **Accessible alternate view**: the non-spatial World KB relationship table is a complete write-equivalent surface for create/edit/delete, not a read-only summary.

`@42ch/nexus-contracts` advances from 0.9.0 to 0.10.0 for the relationship DTOs and graph-response `relationships[]` item-schema promotion.

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
- **Tauri v2 macOS desktop shell** — the shell loads the same `apps/web/dist` in a system webview (`web-ui.md` §14). On macOS that means WKWebView (the macOS system webview, also used by Safari). React Flow's interaction model is standard DOM/SVG/HTML pointer + keyboard work, so it should run in the WKWebView as the same SPA. V1.68 must still smoke-test drag, wheel/pinch zoom, focus rings, and clipboard/keyboard shortcuts inside the Tauri shell because desktop webviews can differ from Chromium in gesture details. (V1.68 implement decision)
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
| **World KB** | World, KeyBlock/entity, event, rule, location, organization, computable block, pending extraction candidate | Typed relationship edges (`WorldKbRelationshipProjection`), source-anchor provenance, timeline membership, rule-applies-to, promotion candidate→confirmed KeyBlock | Entity card, relationship edge, pending-candidate node, source-anchor node, computable-state badge | World detail; KB query/list/detail; pending/confirmed/rejected promotion state; adopt/reject/merge/update. Grounding: `entity-scope-model.md` §1–§2 defines World-owned narrative KB assets; §5.5 defines the World KB promotion state machine; §5.6 defines World KB relationship semantics. |

### 3.4 Interface contracts (B2) — Strategy, Outline+Timeline, and World KB β DTOs shipped

The V1.70 α implementation treats React Flow as a presentation and interaction model over domain-owned graph projections for the shipped Strategy read/overlay/Idea-steer slice. V1.71 β promotes the Strategy write operations (`strategy.patch_state`, `strategy.patch_transition`, `strategy.patch_prompt_template`) to schema/codegen-backed DTOs and Local API routes. V1.72 β promotes Outline+Timeline patch DTOs and routes. V1.73 β promotes World KB entity/candidate DTOs and routes. V1.74 β promotes typed World KB relationship DTOs and the `world_kb.patch_relationship` route. The graph-document shape below remains the shared design language for projections; for World KB relationships, `WorldKbEdgeData` is now backed by `WorldKbRelationshipProjection` rather than design-only prose.

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
| World KB | `WorldKbNodeData = { worldId, keyBlockId?, candidateId?, entityKind, name, lifecycle: "pending" | "confirmed" | "rejected" | "merged", sourceAnchors[] }` | `WorldKbEdgeData = { relationshipId, relationType: WorldKbRelationshipKind, customLabel?, confidence?, sourceAnchorIds[], symmetric, projectionDirection: "stored" | "symmetric_reverse" }` | Promotion state follows the World KB lifecycle in `entity-scope-model.md` §5.5. Relationship edges are schema-backed in V1.74 via `WorldKbRelationshipProjection`; source-anchor-only provenance edges remain a separate projection class. |

V1.73 codegen-derived DTO names use the `world-kb-*.schema.json` filename convention for generated TypeScript/Rust symbols, consistent with the V1.71/V1.72 generated-contract pattern even where schema `title` strings use a verb-prefix form. The shipped names are `WorldKbGraphResponse`, `WorldKbCandidatesResponse`, `WorldKbPatchEntityRequest` / `WorldKbPatchEntityResponse`, `WorldKbPromoteCandidateRequest` / `WorldKbPromoteCandidateResponse`, `WorldKbConflictError`, and `WorldKbValidationError`.

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

### 3.5 Structured write boundary (B3) — **Shipped β (V1.71)**

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

Examples:

| User action | Structured operation shape | Daemon persistence target |
| --- | --- | --- |
| Rename Strategy state | `strategy.patch_state({ strategy_id, state_id, set: { label, description } })` | Updates `preset.yaml` through the preset bundle writer; validates ids/reachability before commit. |
| Rewire Strategy edge | `strategy.patch_transition({ source_state_id, old_target, new_target, condition })` | Rewrites the structured `next`/`branches` field; runs preset semantic validation before commit. |
| Edit prompt text inside a node | `strategy.patch_prompt_template({ node_id, template_patch })` | Applies a template-scoped write; TipTap/Markdown round-trip is limited to that prompt/node content. |
| Move chapter under volume / attach event | `work.patch_outline_graph({ op: "move_chapter" | "link_event", ... })` | Updates outline/index/DB metadata via a structured writer; no whole-document outline PUT from the canvas. |
| Adopt World KB candidate / patch entity | `kb.promote_candidate(...)`, `kb.patch_entity(...)` | Updates `kb_extract_jobs` / `kb_key_blocks` under the World KB state machine (`entity-scope-model.md` §5.5) with per-row OCC. |

This supersedes the V1.65 whole-file outline PUT model for canvas surfaces: V1.65 could save a whole outline document because the UI was a document editor (`web-ui.md` §13.1, §13.5). The canvas model must instead address and validate individual nodes/edges. V1.71 promotes the 3 Strategy operations to schema-backed Local API contracts and daemon-owned persistence; V1.72 and V1.73 extend the same structured patch boundary to Outline+Timeline and World KB entities/candidates. Future canvas operations, including typed World KB relationship editing, must reuse this boundary rather than reintroducing raw file writes.

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

V1.74 extends the shipped `canvas-worldkb-*` family with schema-backed relationship tokens consumed by relationship edge rendering, confidence/grounding badges, and the relationship inspector:

| Token | Intent |
| --- | --- |
| `canvas-worldkb-relationship-edge-default` | Default typed relationship edge stroke for stored relationship projections. |
| `canvas-worldkb-relationship-edge-symmetric` | Visual treatment for symmetric relationship projections, including derived reverse projections. |
| `canvas-worldkb-relationship-edge-custom` | Visual treatment for `WorldKbRelationshipKind = custom` edges with `custom_label`. |
| `canvas-worldkb-relationship-confidence-low` / `canvas-worldkb-relationship-confidence-mid` / `canvas-worldkb-relationship-confidence-high` | Confidence badge fills; confidence remains display-only. |
| `canvas-worldkb-relationship-grounded-badge` | Badge treatment for relationships with one or more `source_anchor_ids`. |
| `canvas-worldkb-relationship-asserted-badge` | Badge treatment for author-asserted relationships with empty `source_anchor_ids`. |
| `canvas-worldkb-relationship-inspector-fill` | Relationship inspector panel fill/chrome. |

The V1.73 `canvas-worldkb-relationship-edge` token remains a compatibility alias to `canvas-worldkb-relationship-edge-default`; new consumers should use the V1.74 token names above.

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
- The UI must make the authorship boundary explicit: the user is giving direction; Nexus will execute and write prose through orchestration. Labels should prefer verbs like **Steer**, **Run**, **Resume**, **Ask Nexus to revise**, and **Apply idea to this node** over **Edit body** or **Write chapter manually**. The write-boundary (§3.5) lets the author adjust Strategy node labels, conditions, or prompt snippets and then steer execution with the same verbs; it does not turn the canvas into a manual prose editor.
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
3. **Screen-reader summaries** — expose graph-level summaries via ARIA (Accessible Rich Internet Applications) live regions: node count, selected node label/type/status, edge count, validation errors, current execution node, and Converge wait state (e.g., "Join waiting for 2 of 3 branches"). Use `ariaLabelConfig` for localized/control labels.
4. **Focus management** — opening a node inspector moves focus to the inspector heading; closing returns focus to the originating node. Validation errors focus the first failing node and mirror the error in the side panel so color/position are not the only cues.
5. **Pointer alternatives** — edge creation/rewiring must have a keyboard/dialog path (choose source node → choose target node → choose edge kind/condition) in addition to drag handles.
6. **Motion and zoom discipline** — honor reduced-motion preferences for animated edges/auto-layout transitions; maintain visible focus rings at all zoom levels; do not encode state only by edge color.
7. **Conflict modal accessibility** — when a 409 conflict occurs, announce the conflict and the current-vs-draft difference via an ARIA live region; move focus into the modal and trap it until the author selects an action; return focus to the originating node or inspector control when the modal closes; provide keyboard shortcuts for **Use current**, **Reapply my edit**, **Review side-by-side**, and **Cancel**; respect `prefers-reduced-motion` for any modal or graph animation triggered by the conflict.

### 4.5 User stories (steering loop)

The author **directs an autonomous executor**; they do not write alongside an assistant. (Pure manual body writing is intentionally absent — the AI owns prose.)

- **Steer by Idea** — *As an author*, I express an Idea (Work-level or on a specific node) and choose **Steer / Run / Resume / Ask Nexus to revise**, then Nexus executes — drafting prose, advancing the chapter, updating the KB — so I direct the work without typing the body myself. After I edit a Strategy node, I can use the same verbs to ask Nexus to act on the revised graph.
- **Read the Strategy as a graph** — *As an author*, I see my Strategy (preset) rendered as a state-machine graph with visible join/wait nodes, so I understand how Nexus will execute my Work before it runs — and I can rewire a branch or adjust a gate on the canvas.
- **Shape the outline/timeline spatially** — *As an author*, I shape volumes/chapters/events as graph nodes with timeline/foreshadow edges, so the structure that a linear outline hides becomes visible and editable — and I steer Nexus to (re)draft the node I point it at.
- **Steer World KB continuity** — *As an author*, I browse entities/events/rules as a relationship graph with promotion-state badges, and adopt/reject/merge from the canvas, so continuity constraints stay coherent as the Work grows.
- **Review AI execution on the canvas** — *As an author*, after Nexus executes I see what changed on the canvas (node status, generated-output links, pending instructions) and review the result; if Nexus updated a node I was editing, the conflict modal lets me choose **Use current**, **Reapply my edit**, or **Review side-by-side**, so I stay in command of an autonomous process.

## 5. Non-goals (V1.70 α)

- No promotion of canvas **writes** in V1.70 α. The shipped slice is read + visualization + live overlay + Idea-steer only.
- No schema/codegen/DTO lock for write operations in V1.70 α. Operation names and TypeScript-like interfaces above remain illustrative paper contracts until V1.71.
- No promotion of the outline+timeline canvas or World KB canvas in V1.70 α; both remain V1.71+ Draft surfaces.
- No removal/regression of the shipped V1.65 outline editor; canvas-pivot and node-granular outline edits are V1.71+ Draft scope.
- No CLI/spec rename of `preset` → `strategy` (breaking; deferred). V1.70 α adopts the terminology in UI/spec wording only.

## 6. Roadmap (durable tracking)

- **V1.70 α — shipped**: Strategy Canvas read projection + visualization + live overlay + Idea-steer. This is the shipped successor slice to the retired body-editor roadmap, not the full three-surface canvas program.
- **V1.71 β — shipped**: structured write boundary and node-granular Strategy edits (`strategy.patch_state`, `strategy.patch_transition`, `strategy.patch_prompt_template`, validation/conflict DTOs, YAML `revision:` graphRevision) promoted through schemas/codegen and daemon-owned persistence contracts.
- **V1.72 β — shipped (V1.72 P0)**: Outline+Timeline Canvas β slice — Work → Volume → Chapter → Scene/Beat graph projection + timeline lane + foreshadow edges + 3 structured patch routes (`outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event`) + `outline_revision:` markdown frontmatter graphRevision + structured conflict error + UI retry/merge (outline-flavored copy) + non-spatial alternate views + atomic outline markdown persistence. `@42ch/nexus-contracts` 0.7.0 → 0.8.0 (additive outline DTOs). 8 outline/timeline DESIGN.md canvas-write tokens added. See V1.72 compass [`v1.72-canvas-outline-timeline-beta-and-hygiene-compass-v1.md`](../../iterations/v1.72-canvas-outline-timeline-beta-and-hygiene-compass-v1.md).
- **V1.73 β — shipped (V1.73 Track A)**: Canvas World KB surface (Draft §3.3 surface 3) promoted through additive World KB DTOs and 4 Local API routes: graph projection, candidates projection, `kb.patch_entity`, and `kb.promote_candidate`. Builds on entity-scope-model §5.5 promotion state machine, Canvas Shell from V1.70, Strategy β write patterns from V1.71, and Outline+Timeline β patterns from V1.72. Uses per-row OCC (`expected_version` → `version`) and structured 409/422 error DTOs.
- **V1.74 β — shipped**: World KB relationships surface — first-class typed relationship edges/CRUD beyond V1.73 source-anchor provenance projection, with `world_kb.patch_relationship`, per-row OCC on `kb_relationships.revision`, directed + `symmetric` read projections, `WorldKbRelationshipKind` + `custom_label`, and complete non-spatial relationship table parity.
- **V1.75+ candidate**: canvas-pivot (retire V1.65 outline whole-document editor in favor of node-granular canvas operations). Depends on V1.72 Outline+Timeline β maturity and post-V1.74 canvas adoption evidence.
