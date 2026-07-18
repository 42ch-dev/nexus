# Canvas Strategy Surface — Specification

| Attribute | Value |
| --- | --- |
| **Status** | **Shipped β (V1.74)** — Strategy read + visualization + live overlay + Idea-steer (V1.70), write-boundary operation DTOs + node-granular Strategy edits + conflict policy (V1.71), Outline+Timeline canvas β (Work → Volume → Chapter → Scene/Beat graph projection + timeline lane + foreshadow edges + 3 structured patch routes `outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event` + outlineRevision + structured conflict error + UI retry/merge + non-spatial alternate views) (V1.72), World KB canvas β (World KB graph + candidates projections, 2 structured patch routes `kb.patch_entity` / `kb.promote_candidate`, per-row OCC on `kb_key_blocks.revision` / `kb_extract_jobs.version`, 409/422 structured errors, and 4 Daemon API routes) (V1.73), and typed World KB relationship editing (schema-backed relationship DTOs, `world_kb.patch_relationship`, `kb_relationships.revision` OCC, directed/symmetric projections, non-spatial relationship table) (V1.74) are shipped. |
| **Document class** | Draft overlay |
| **Scope** | Product vision + Draft architecture for the human-facing **Canvas** control surfaces: Strategy (Preset) orchestration graph, Work outline + timeline graph, World KB graph; React Flow rendering; the "AI owns prose, human steers via Canvas" thesis; node-granular write boundaries; canvas token contract for DESIGN.md placeholders |
| **Coordinates with** | [orchestration-engine.md](orchestration-engine.md) (strategy = graph-of-graphs), [web-ui.md](web-ui.md) (§15 V1.67 stage + V1.68 canvas roadmap), [local-api-surface-conventions.md](local-api-surface-conventions.md), [chapter-content-local-api.md](chapter-content-local-api.md), [daemon-runtime.md](daemon-runtime.md) |
| **Supersedes** | [body-editor.md](body-editor.md) (archived: [../../archived/knowledge/body-editor.md](../../archived/knowledge/body-editor.md)) |
| **Authored** | V1.67 Phase 2b re-discussion — **@architect** (architecture + React Flow feasibility + DAG↔canvas mapping + write boundary) + **@product-manager** (product thesis + canvas UX + Strategy terminology); PM-scaffolded stub pending authoring |

> **Promoted to Draft (2026-06-27 V1.69 P0).** The V1.67 Exploration was promoted to Draft by `@architect` for interface contracts, structured write boundary, and canvas-token contract. Product/UX thesis from the original `@product-manager` contribution remains in §4. This Draft intentionally stops short of schema/codegen or React Flow implementation authority.

> **Promoted to Shipped α (V1.70).** The V1.70 compass ([`v1.70/delivery-compass.md`](../../iterations/v1.70/delivery-compass.md)) shipped the first Strategy Canvas slice: read-only Strategy graph projection, canvas visualization, live execution overlay, and Idea-steer affordance. Implementation provenance: parent `079f687f`; feature commits `81cb4256`, `f82bcdd3`, `10edf22f`, `dad35736` on `feature/v1.70-canvas-strategy-read`, merged into `iteration/v1.70`. This promotion is scoped to the α read/overlay/steer slice only; structured write-boundary DTOs, node-granular editing, outline+timeline canvas, and World KB canvas remain Draft for V1.71+.

> **Promoted to Shipped β (V1.71).** The V1.71 compass ([`v1.71/delivery-compass.md`](../../iterations/v1.71/delivery-compass.md)) ships the Strategy write-boundary slice: schema-backed patch DTOs, 3 node-granular Strategy patch routes, YAML `revision:` graphRevision conflict detection, daemon validation, atomic persistence, and UI retry/merge conflict handling. This promotion is scoped to the Strategy surface only; outline+timeline and World KB canvas surfaces remain Draft for V1.72+.

> **Promoted to Shipped β (V1.72).** The V1.72 compass ([`v1.72/delivery-compass.md`](../../iterations/v1.72/delivery-compass.md)) ships the Outline+Timeline β slice: schema-backed patch DTOs (`OutlinePatchStructureRequest` / `OutlinePatchChapterRequest` / `TimelinePatchEventRequest` + `OutlinePatchResponse` + `OutlineConflictError` + `OutlineValidationError`), 3 Daemon API patch routes (structure / chapter / timeline-event), `outline_revision:` markdown frontmatter graphRevision conflict detection, daemon validation (ID existence, structural integrity, status lifecycle, timeline reference resolution, revision precondition), atomic outline markdown persistence (temp + rename + fsync + dir fsync), and UI retry/merge conflict handling with outline-flavored copy + non-spatial alternate views (chapter list + timeline event list). `@42ch/nexus-contracts` 0.7.0 → 0.8.0 (additive outline DTOs). DESIGN.md gains 8 outline/timeline canvas-write tokens (`canvas-outline-volume-fill` + 4 chapter-card statuses + `canvas-outline-timeline-event-pin` + `canvas-outline-foreshadow-edge` + `canvas-outline-timeline-marker` + `canvas-outline-conflict-marker`). This promotion is scoped to the Outline+Timeline surface only; World KB canvas surface remains Draft for V1.73+. Canvas-pivot (retiring V1.65 outline whole-document editor) remains V1.73+ backlog.

> **Promoted to Shipped β (V1.73).** The V1.73 compass ([`v1.73/delivery-compass.md`](../../iterations/v1.73/delivery-compass.md)) ships the World KB canvas β slice: schema/codegen-backed World KB DTOs, 2 structured patch routes (`POST /v1/local/worlds/{world_id}/kb/patch-entity`, `POST /v1/local/worlds/{world_id}/kb/promote-candidate`), per-row OCC via `expected_version` against `kb_key_blocks.revision` / `kb_extract_jobs.version`, structured 409 `WorldKbConflictError` + 422 `WorldKbValidationError`, and 4 Daemon API routes including the read projections (`GET /v1/local/worlds/{world_id}/kb/graph`, `GET /v1/local/worlds/{world_id}/kb/candidates`). This promotion is scoped to the World KB entities + candidates surface; typed World KB relationship editing remains V1.74+.

> **Promoted to Shipped β (V1.74).** The V1.74 compass ([`v1.74/delivery-compass.md`](../../iterations/v1.74/delivery-compass.md)) ships the typed World KB relationships β slice: schema-backed relationship DTOs, a single structured patch route (`POST /v1/local/worlds/{world_id}/kb/patch-relationship` with action `add | update | remove`), per-row OCC via `expected_version` against `kb_relationships.revision`, structured 409 `WorldKbConflictError` + 422 `WorldKbValidationError`, and `GET /v1/local/worlds/{world_id}/kb/graph` populated with typed `relationships[]`. This promotion is scoped to first-class relationship editing for the World KB surface; relationship confidence weighting/filtering and automatic relationship extraction remain future work.

> **Promoted to Draft (V1.122) — Timeline peer-surface amendment.** The V1.122 compass ([`v1.122/delivery-compass.md`](../iterations/v1.122/delivery-compass.md)) elevates **Timeline** from a lane inside the Outline+Timeline surface to a fourth peer `CanvasSurfaceKind = "timeline"` that is the **World-building hero** surface and the **default World-entry** surface. This is an **additive Draft overlay**: it introduces the Timeline peer surface, its World-building projection contract, its write-boundary reuse plan, the Timeline-as-default-World-entry IA rule, and it repositions the existing `work-outline-timeline` surface narratively as **"Outline (Timeline-companion)"** (the `CanvasSurfaceKind` string `"work-outline-timeline"` is unchanged; only the narrative label is clarified). Shipped β normative text for the Strategy, Outline+Timeline, and World KB surfaces is **not rewritten or removed**. The architect-locked data composition, adapter contract, write boundary, and conflict policy live in §3.3.2; the authoritative iteration-scoped contract is [`timeline-canvas-architecture.md`](../iterations/v1.122/specs/timeline-canvas-architecture.md). `wire_contracts_changed: false` (additive `CanvasSurfaceKind` enum value + reuse of 12 shipped DTOs/routes; no new schemas, no codegen, no daemon Rust change).

> **Promoted to Draft (V1.123) — Three-Layer Timeline + Work Timeline peer amendment.** The V1.123 compass ([`v1.123/delivery-compass.md`](../iterations/v1.123/delivery-compass.md)) deepens Timeline into **three zoom layers** — **Brief** (world-global era markers), **Narrative** (event-level, V1.122 reframed), **Moment** (scene/beat precision) — with **domain-differentiated layer composition** (World Timeline: Brief + Narrative; Work Timeline: Narrative + Moment). This is an **additive Draft overlay**: §3.3.3 introduces (a) the Brief↔Narrative layer switcher on the V1.122 `timeline` surface; (b) a new `CanvasSurfaceKind = "work-timeline"` peer surface for Work Timeline with a Narrative↔Moment layer switcher; (c) the architect-locked carrier contract (Brief-on-KeyBlock via new wire `BlockType = "era"`; Moment-on-Outline via V1.108 Scene/Beat UI projection from V1.72 `WorkOutline`); (d) cross-layer navigation rules; (e) per-layer empty-state honesty rules; (f) layer-state persistence via URL query `?layer=...`. Shipped β normative text for the Strategy, Outline+Timeline, World KB, and V1.122 Timeline surfaces is **not rewritten or removed**. The authoritative iteration-scoped contract (Brief/Moment carrier trade-off matrices, adapter TypeScript signatures, conflict policy per layer, 8-point wire-contracts gate) is [`three-layer-architecture.md`](../iterations/v1.123/specs/three-layer-architecture.md). `wire_contracts_changed: true` (single additive enum value `"era"` in `BlockType` — `schemas/common/common.schema.json`; no new daemon route, no new DTO, no new conflict DTO; codegen regen + minor `@42ch/nexus-contracts` version bump).

## 1. Product thesis (LOCKED from user re-discussion, 2026-06-26)

Nexus is an **AI-autonomous creative executor** (in the spirit of Codex / a design tool): the human **inputs an Idea** and **steers** the work; the **AI owns the prose writing and execution**. Nexus is **not** a manual editor where the human writes chapter bodies by hand.

The human steers through three **Canvas (infinite-canvas) surfaces**, not document editors:

1. **Strategy (Preset) orchestration canvas** — visualize and edit the preset/strategy that drives the creative workflow. Conceptual rename: **"Preset" → "策略 (Strategy)"** — it is the workflow that drives the creative work (this is already the orchestration engine's mental model: a strategy is a hierarchical state-machine of inner DAGs — graph-of-graphs; `orchestration-engine.md` §3).
2. **Work outline + timeline canvas** — compile and steer the Work's outline and timeline as a graph, not a linear rich-text document.
3. **World KB canvas** — browse and steer the World Knowledge Base (entities, events, rules, relationships) as a graph.

> **V1.122 Draft overlay:** a fourth peer surface — **Timeline (World-building hero)** — is added alongside these three shipped surfaces (§3.3.2). Timeline is the default surface for **World entry**; the three surfaces above remain peers, and the Work outline + timeline surface is renamed narratively to **"Outline (Timeline-companion)"** (its `CanvasSurfaceKind` string `"work-outline-timeline"` is unchanged). The shipped β description of the original three surfaces in this section is not rewritten.

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

- **Read + visualization**: Work → Volume → Chapter → Scene/Beat graph projection. Volume lanes render as sub-flows with `parentId`+`extent:parent` children. Chapter cards display `wc`/`slug`/`status` from the outline markdown frontmatter. TipTap fragment preview is **read-only on the canvas** (the V1.65 whole-document outline editor was retired in V1.75; outline prose is now authored in the Chapter node inspector via `outline.patch_chapter` `set.content` — see §3.5).
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
- The Daemon API write boundary for canvas surfaces is **structured/node-granular**, not whole-file PUT. (V1.68 design; this Exploration records the principle.)

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

**Data source.** The static canvas is fed by the Strategy definition (preset YAML bundle: `states`, `inner_graphs`, prompt/template references). The live overlay is fed by session state from the daemon (`orchestration-engine.md` §3.3, §4.2; `web-ui.md` §5 `NexusClient` boundary). V1.67 promotes preset get/update/delete methods on the TS client, but this Exploration does **not** assert that the Daemon API already exposes the exact graph document shape or session detail needed by the canvas. V1.68 should add or promote read endpoints such as "get Strategy graph projection" and "get session graph overlay" if the existing preset/session detail endpoints are too YAML/raw or too summary-only. (V1.68 implement decision)

### 3.3 Four canvas surfaces (V1.122 Draft overlay: Timeline peer elevated)

All four surfaces share a **Canvas Shell** and specialize by data adapter + node/edge registry:

- Shared shell: React Flow provider, pan/zoom controls, minimap/overview, selection model, command palette, side inspector, validation/errors panel, dirty-state guard, keyboard shortcuts, screen-reader graph summary, and `NexusClient` transport injection (`web-ui.md` §5).
- Per-surface adapters: convert Daemon API domain DTOs into `nodes`/`edges`, and convert user edits into structured operations (§3.4). No surface may read/write files directly from the browser/Tauri webview.

| Surface | Graph nodes | Graph edges | Custom node types | Primary Daemon API needs |
| --- | --- | --- | --- | --- |
| **Strategy (Preset) editor** | Outer states; nested inner-graph steps; Converge join nodes; terminal nodes | Linear, labeled, expression/default, converge incoming/outgoing, inner `depends_on` | State node, join node, inner-graph group, prompt/capability node, manual-wait node, terminal node | Preset list/detail/update/delete/validate; session list/detail for live overlay; capability list for node configuration. |
| **Outline (Timeline-companion)** *(narrative label for `work-outline-timeline`; Work projection surface, default for **Work entry** per V1.118)* | Work, volume, chapter, scene/beat, timeline event, foreshadowing/index item | Contains/ordered-after, references, foreshadows, belongs-to-volume, event→chapter realization | Volume lane, chapter card, event node, dependency/foreshadow node, in-node TipTap outline editor | Work/detail, chapter list/detail, outline read/structured patch, structure patch, timeline/index read/patch. The shipped V1.65 outline is a linear rich-text document (`web-ui.md` §13); the canvas projection turns headings/chapters/events into addressable graph nodes instead of replacing the underlying Work model. Chapter-relative timeline affordances remain here as Timeline-companion UX. |
| **Timeline (World-building hero)** *(V1.122 Draft overlay; `CanvasSurfaceKind = "timeline"`; default for **World entry**)* | World-scoped KeyBlock entities of `block_type=event` (when-axis events); other entity kinds (character/scene/organization/…) as **Context clusters** off the when-axis. **No Fork marker nodes** — Fork data renders only as optional header-badge chrome from a `WorldState` sidecar (§3.3.2) | Typed `WorldKbRelationshipProjection` edges reused **verbatim** (`WorldKbEdgeData`, V1.74); `source_anchors[]` render as grounding badges. No Timeline-specific edge DTOs (no `ForeshadowEdge`/`RealizesEdge`/`ForkFromEdge`) | `TimelineEventNode` (`layoutHint: 'event'`), `TimelineKeyBlockNode` (`layoutHint: 'context'`, Context cluster) | **Single graph source:** `GET /v1/daemon/worlds/{world_id}/kb/graph` → `WorldKbGraphResponse` (V1.73). **Optional sidecar:** `GET /v1/daemon/narrative/worlds/{world_id}` → `WorldState` for Fork-badge header chrome only. **Write:** `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` (`kb.patch_entity`) **only**. See §3.3.2 for the full architect-locked contract. |
| **World KB** | World, KeyBlock/entity, event, rule, location, organization, computable block, pending extraction candidate | Typed relationship edges (`WorldKbRelationshipProjection`), source-anchor provenance, timeline membership, rule-applies-to, promotion candidate→confirmed KeyBlock | Entity card, relationship edge, pending-candidate node, source-anchor node, computable-state badge | World detail; KB query/list/detail; pending/confirmed/rejected promotion state; adopt/reject/merge/update. Grounding: `entity-scope-model.md` §1–§2 defines World-owned narrative KB assets; §5.5 defines the World KB promotion state machine; §5.6 defines World KB relationship semantics. |

### 3.3.1 CanvasSurfaceAdapter recipe (V1.114 P0)

V1.114 P0 introduces a shared **CanvasSurfaceAdapter** interface and a single `useCanvasSurface()` composition hook so new surfaces do not re-wire shell boilerplate. The shell (`CanvasShell`) owns React Flow provider state, viewport caching, selection, and the **Re-layout** action; the adapter owns domain projection, node/edge types, the inspector, the alt-view companion, and an accessibility summary.

#### Adapter interface

```ts
interface CanvasSurfaceAdapter<TGraph, TNodeData, TEdgeData> {
  surfaceKind: CanvasSurfaceKind;
  /** Project daemon graph DTO → React Flow nodes/edges. Owns parentId/extent nesting. */
  projectGraph(graph: TGraph): { nodes: Node<TNodeData>[]; edges: Edge<TEdgeData>[] };
  /** Node types registry for this surface. */
  nodeTypes: NodeTypes;
  /** Edge types registry (optional). */
  edgeTypes?: EdgeTypes;
  /** Dagre layout options; omit to opt out of auto-layout. */
  layoutOptions?: CanvasSurfaceLayoutOptions;
  /** Conflict DTO → conflict-modal props (optional). */
  adaptConflict?(error: unknown): ConflictModalProps | null;
  /** Inspector routing: which inspector renders for a selected node. */
  renderInspector?(node: Node<TNodeData>): ReactNode;
  /** Non-spatial alt-view companion (table/list). */
  renderAltView?(): ReactNode;
  /** Graph-level a11y summary (required). */
  summarizeGraph(graph: TGraph): string;
}

interface CanvasSurfaceLayoutOptions {
  direction?: 'TB' | 'LR';
  rankSep?: number;
  nodeSep?: number;
}
```

#### `useCanvasSurface()` composition

```ts
const surface = useCanvasSurface(adapter, queryResult);
```

`useCanvasSurface`:

1. Caches the viewport via `useCanvasViewport(adapter.surfaceKind)`.
2. Projects the daemon graph with `adapter.projectGraph` (memoized; adapter must be stable).
3. Merges new projections with the existing local React Flow state, preserving manual positions and selection for nodes that already exist.
4. Applies `useAutoLayout` when `adapter.layoutOptions` is defined.
5. Computes `summaryText`, `altView`, `inspector`, and `conflict` from the adapter.
6. Exposes `relayout()` only when the adapter opts into layout.

Surfaces should pass the returned `nodes`, `edges`, `nodeTypes`, `onNodesChange`, `summaryText`, `relayout`, and the overlay children (`inspector`, `altView`) to `CanvasShell`.

#### `useAutoLayout()` integration + manual-override semantics

`useAutoLayout(nodes, edges, options)` is a dagre (`@dagrejs/dagre`) wrapper with compound-graph support. Semantics:

- **Opt-in only.** Surfaces that omit `layoutOptions` receive a pass-through; positions are never changed.
- **Initial layout.** On the first projection of an opt-in surface, dagre runs automatically and produces a readable default arrangement.
- **Manual override.** If the user drags any node, `useAutoLayout` detects the deviation from the last computed layout and suppresses automatic re-layouts on subsequent projections. This prevents a data refetch from undoing the author's manual positioning.
- **Re-layout.** The **Re-layout** button in `CanvasShell` calls `relayout()`, which clears the manual-override flag and re-runs dagre. `CanvasShell` only renders the button when `relayout` is supplied.
- **Compound graphs.** Dagre is configured with `compound: true`; `parentId` edges are registered so nested sub-flows (Strategy inner-graph groups, Work volume lanes, etc.) are laid out relative to their parent bounds.
- **Performance guard.** Layouts that exceed `200ms` log a warning; sustained breaches should be recorded as a residual (e.g., cap visible nodes or lazy-expand subgraphs).

#### Recipe: add a new canvas surface

1. Define the surface graph payload (`TGraph`) and node/edge data types.
2. Implement `CanvasSurfaceAdapter<TGraph, TNodeData, TEdgeData>`:
   - `projectGraph` converts the daemon DTO into React Flow nodes/edges.
   - `nodeTypes` registers the custom node components.
   - Optionally provide `edgeTypes`, `layoutOptions`, `adaptConflict`, `renderInspector`, `renderAltView`.
   - `summarizeGraph` returns a string for the screen-reader live region.
3. If the adapter needs mutable orchestrator state (e.g., selected node, form state, callbacks), build it with a stable factory that reads from a mutable `React.RefObject` context. The adapter object itself must stay stable so `useCanvasSurface` does not re-project on every render.
4. In the surface orchestrator, call `useCanvasSurface(adapter, queryResult)` where `queryResult` conforms to `CanvasSurfaceQueryResult<TGraph>`.
5. Render `CanvasShell` with the returned values. Pass `relayout` to enable the Re-layout button.
6. Wire the structured write boundary (§3.5) through the adapter or orchestrator; the shell never writes to files directly.

#### Worked examples

- **Strategy canvas (T2).** `apps/web/src/components/canvas/strategy-canvas/strategy-canvas-adapter.tsx` implements a stable `createStrategyCanvasAdapter(ctxRef)` that reads mutable form/save/conflict state from the context ref. It sets `layoutOptions: { direction: 'TB' }` to opt into top-down auto-layout and exposes the Re-layout action. Inspectors (`StateInspector`, `EdgeInspector`, `PromptInspector`) and the `StrategyAltView` are surface-owned and rendered through the adapter.
- **World KB canvas (T3).** `apps/web/src/components/canvas/world-kb/world-kb-canvas-adapter.tsx` projects entities, candidates, source anchors, and typed relationships into nodes/edges. It currently omits `layoutOptions` (pass-through), so it relies on the daemon/projections' own spatial hints and manual positioning. Inspectors and `WorldKbAltView` are similarly adapter-driven.

### 3.3.2 Timeline (World-building hero) surface — V1.122 Draft overlay

> **Architect-locked.** This subsection is the normative Draft contract P1 implements. It is **additive** — it does not rewrite the shipped Strategy / Outline+Timeline / World KB β text. The authoritative iteration-scoped elaboration (TypeScript signatures, conformance rules, temporal-positioning rule, empty-state copy, verification gate) is [`timeline-canvas-architecture.md`](../iterations/v1.122/specs/timeline-canvas-architecture.md). Where this overlay and the iteration-scoped architecture spec agree, both are normative; where they differ in detail, the architecture spec is the finer-grained P1 reference.

The Timeline surface is **World-scoped**. It projects a World's history — events, KeyBlock entities, typed relationships — onto a when-axis, making the World's Timeline the central instrument for World building. It is the **default surface for World entry** (§4.5); the Outline (Timeline-companion) surface remains the default for Work entry (V1.118, unchanged).

#### Data composition (LOCKED — single graph source)

- **Graph source (sole):** `GET /v1/daemon/worlds/{world_id}/kb/graph` → **`WorldKbGraphResponse`** (V1.73 shipped; schema `schemas/daemon-api/canvas/world-kb/world-kb-graph-response.schema.json`). The adapter's `projectGraph` accepts `WorldKbGraphResponse` directly — **no wrapper, no join, no second graph endpoint**.
- **Optional sidecar (header chrome only):** the orchestrator MAY additionally fetch `GET /v1/daemon/narrative/worlds/{world_id}` → `WorldState` (V1.26) to render a read-only Fork-badge in the canvas header when `WorldState.is_fork === true` ("Fork of `<parent_world_id>` at event `<forked_from_event_id>`"). This sidecar is **not a timeline data source**; it MUST NOT be merged into `projectGraph`. If the sidecar is absent or errors, the badge is omitted (graceful degradation) and the Timeline surface remains fully functional.
- **Explicitly NOT composed:** Work-scoped outline timeline events (`timeline.patch_event` surface) are **not** merged onto the World when-axis in V1.122. They are chapter-relative (`realizes_chapter_id`, foreshadow edges between chapter-linked events) with no World-level merge key; composing them would require N+1 fetches per bound Work. They remain on the Outline (Timeline-companion) surface for Work entry.
- **Deferred:** a World-scoped `TimelineEvent` HTTP route (`GET /v1/daemon/worlds/{world_id}/timeline`) is out of V1.122 scope (would require daemon Rust changes + a new external route). The domain `schemas/domain/timeline-event.schema.json` table remains reachable only via `NarrativeGateway::get_timeline()` (internal) and the `nexus.timeline.recent.get` host-tool capability. Tracked under `DF-V1122-DEEPER-WB`.

#### Projection mapping (LOCKED)

| `WorldKbGraphResponse` field | Projection on Timeline canvas | Node/edge kind |
|-------------------------------|-------------------------------|----------------|
| `entities[block_type=event]` | When-axis events (the "when" content of the World) | `TimelineEventNode` (`layoutHint: 'event'`) |
| `entities[block_type!=event]` (character / scene / organization / item / info_point / conflict / ability / species / faction / magic_system / technology / deity / level / economy_tier / dialogue / beat / act) | Context clusters off the when-axis; may be positioned near related events via relationship edges | `TimelineKeyBlockNode` (`layoutHint: 'context'`) |
| `relationships[]` | Typed relationship edges (read-only in V1.122), reusing `WorldKbEdgeData` **verbatim** (V1.74 `WorldKbRelationshipProjection`) | `Edge<TimelineEdgeData>` where `TimelineEdgeData = WorldKbEdgeData` |
| `source_anchors[]` | Grounding badge data on referenced nodes (optional rendering) | Node metadata; **not** a separate node kind |

`block_type=event` entities ARE World-scoped narrative events per [`entity-scope-model.md`](entity-scope-model.md) §5.1.1 — they ARE the "when-axis" content the Timeline hero surface projects. `foreshadow` / `realizes` / `fork-from` are **Work-outline projection labels**, not Timeline edge DTOs; the Timeline surface introduces **no** new edge types.

#### Node types

- **`TimelineEventNode`** — a `block_type=event` KeyBlock entity projected onto the when-axis. Temporal positioning uses only `body.attributes.occurred_at` when present; the adapter MUST NOT fabricate chronology from `updated_at`, `canonical_name`, `version`, `sequence_no`, or any non-temporal field. Entities without a temporal signal cluster in a temporal-unknown group with honest copy.
- **`TimelineKeyBlockNode`** — a non-event KeyBlock entity rendered as a Context cluster off the when-axis.
- **No `TimelineForkMarkerNode`.** Fork data is reserved for the optional canvas-header badge from the `WorldState` sidecar; there are no Fork nodes on the timeline in V1.122. Fork create/merge UI is a Non-Goal (`DF-V1122-FORK-UI`).

#### Adapter contract (LOCKED)

`TimelineCanvasAdapter implements CanvasSurfaceAdapter<WorldKbGraphResponse, TimelineNodeData, WorldKbEdgeData>` per the V1.114 §3.3.1 recipe:

- `TimelineGraph = WorldKbGraphResponse` (alias, no wrapper).
- `TimelineNodeData = WorldKbEntityProjection & { layoutHint: 'event' | 'context'; occurredAtHint?: string }`.
- `TimelineEdgeData = WorldKbEdgeData` (verbatim reuse; no extension).
- Stable factory **`createTimelineCanvasAdapter(ctxRef)`** — the adapter object stays stable across renders (V1.114 §3.3.1 "stable factory that reads from a mutable `React.RefObject` context").
- `layoutOptions: { direction: 'LR' }` — opts into dagre left-to-right auto-layout. `direction: 'LR'` is a **visual** choice, not a chronology promise.
- `summarizeGraph(graph)` MUST include the ordering disclaimer ("Ordering inferred from available event data; not a canonical chronology.") whenever any event lacks an `occurredAtHint` temporal signal.

#### Write boundary (LOCKED — reuse only)

- **Permitted:** `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` (`kb.patch_entity`, V1.73 shipped) — edits World-scoped KeyBlock entity `title` / `body` / `aliases` / `block_type` with per-row OCC via `expected_version` against `kb_key_blocks.revision`.
- **Forbidden from this surface in V1.122:**
  - `POST /v1/daemon/works/{work_id}/timeline/patch` (`timeline.patch_event`) — Work-scoped; operates on outline markdown, not World entities. P1 regression tests MUST assert non-invocation.
  - `POST /v1/daemon/worlds/{world_id}/kb/patch-relationship` (`world_kb.patch_relationship`) — read-only on the Timeline surface in V1.122 (deferred to post-MVP; relationship edits remain on the World KB peer surface).
  - `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate` (`kb.promote_candidate`) — candidate workflow belongs to the World KB surface.
  - Any raw-file write (`PUT` to a file route, Tauri `invoke` writing to disk) — §2 invariant preserved.
- **No new Daemon API routes** in V1.122.

#### Conflict policy (LOCKED — reuse only)

- **Reuses** `WorldKbConflictError` (HTTP 409 — stale `expected_version`) and `WorldKbValidationError` (HTTP 422 — domain-rule failure) from V1.73.
- **No Timeline-specific conflict DTO.** Conflict-modal copy is **world-kb-flavored**, reusing the V1.73 entity-patch / V1.74 relationship-patch copy tokens.
- The `adaptConflict(error)` adapter method parses the canonical `ErrorResponse` envelope and projects `WorldKbConflictError` / `WorldKbValidationError` `details` to the existing conflict-modal props (Use current / Reapply my edit / Review side-by-side).

#### Honest empty-state

A sparse World timeline is a **valid MVP**. The adapter MUST NOT fabricate event ordering. Empty-state copy explains the spine and offers a CTA to the World KB peer surface (e.g. "This World's timeline is empty. Events you add through World KB or chapter extraction will appear here."). Exact copy strings are pinned in the iteration-scoped architecture spec §7.

#### `wire_contracts_changed: false` verification

V1.122 P1 adds **only** a frontend `CanvasSurfaceKind = "timeline"` enum value + a new adapter module under `apps/web/src/components/canvas/timeline-canvas/`. It reuses 12 shipped DTOs/routes (`WorldKbGraphResponse`, `WorldKbEntityProjection`, `WorldKbRelationshipProjection`, `WorldKbPatchEntityRequest`/`Response`, `WorldKbEntityPatch`, `WorldKbConflictError`, `WorldKbValidationError`, `WorldKbSourceAnchorProjection`, `WorldKbRelationshipKind`, `WorldState`, the graph read route, the narrative sidecar route, and the `kb.patch_entity` write route). No new `schemas/`, no codegen drift, no `@42ch/nexus-contracts` version bump, no daemon Rust change. The eight-point P1 verification gate is pinned in the iteration-scoped architecture spec §9.2.

### 3.3.3 Three-Layer Timeline + Work Timeline peer — V1.123 Draft overlay

> **Architect-locked (seat 2).** This subsection is the normative Draft contract P1 + P2 implement. It is **additive** — it does not rewrite the shipped Strategy / Outline+Timeline / World KB β text in §3.3 table or the V1.122 Timeline peer surface text in §3.3.2. The authoritative iteration-scoped elaboration (Brief/Moment carrier verdicts, trade-off matrices, adapter TypeScript signatures, conformance rules, conflict policy, wire-contracts gate) is [`iterations/v1.123/specs/three-layer-architecture.md`](../iterations/v1.123/specs/three-layer-architecture.md). Where this overlay and the iteration-scoped architecture spec agree, both are normative; where they differ in detail, the architecture spec is the finer-grained P1/P2 reference.

V1.123 deepens Timeline into **three zoom layers** — **Brief** (world-global era markers), **Narrative** (event-level, V1.122 reframed — the Timeline layer, distinct from prose-craft narrative writing), **Moment** (scene/beat precision — the Timeline layer, distinct from Moment Context Assembly) — with **domain-differentiated layer composition**: the World Timeline shows Brief + Narrative; the new Work Timeline peer surface shows Narrative + Moment. Cross-layer navigation is **within one Timeline surface** (Brief↔Narrative on World; Narrative↔Moment on Work) — not cross-surface (World Timeline ↔ Work Timeline cross-surface jump is P3 IA scope, not a layer switcher concern).

#### Layer composition (LOCKED — product semantics; carrier implementation per iteration architecture)

```
World Timeline (`CanvasSurfaceKind = "timeline"` — V1.122 preserved):
  ├── Brief layer (hero)     — `block_type=era` KeyBlock projection
  ├── Narrative layer (peer) — `block_type=event` KeyBlock projection (V1.122 unchanged)
  └── Moment layer           — NOT composed (DF-V1123-WORLD-MOMENT)

Work Timeline (`CanvasSurfaceKind = "work-timeline"` — V1.123 NEW):
  ├── Brief layer            — NOT composed (DF-V1123-WORK-BRIEF)
  ├── Narrative layer (peer) — `WorkOutline.timeline_events[]` projection (V1.72 preserved)
  └── Moment layer (hero-on-demand) — V1.108 Scene/Beat projection from `WorkOutline`
```

#### `CanvasSurfaceKind` enum extension (additive — V1.123)

The frontend `CanvasSurfaceKind` union (`apps/web/src/components/canvas/canvas-surface-adapter.ts:6-15`) gains one peer value:

```ts
export type CanvasSurfaceKind =
  | 'strategy'
  | 'outline'
  | 'world-kb-entities'
  | 'world-kb-relationships'
  | 'timeline'             // V1.122 — World Timeline hero surface
  // V1.123 P1 + P2 — additive layer-aware Timeline surfaces.
  //   `timeline`      gains Brief↔Narrative layer switcher (this section).
  //   `work-timeline` is a NEW peer surface for Work Timeline with
  //                   Narrative↔Moment layer switcher (this section).
  // See `iterations/v1.123/specs/three-layer-architecture.md` §2 (Brief carrier),
  // §3 (Moment carrier), §7 (Work Timeline adapter contract).
  | 'work-timeline';
```

No existing `CanvasSurfaceKind` value is renamed or removed. The V1.122 `timeline` surface is **not rewritten** — V1.123 adds a layer switcher to its canvas header and a Brief projection to its adapter, both internally to the surface (additive; the V1.122 §3.3.2 architect-locked contract is preserved).

#### `timeline` surface — Brief↔Narrative layer switcher (V1.123 overlay on V1.122 §3.3.2)

| Property | Contract |
|----------|----------|
| **Layer pair** | Brief + Narrative only (Moment out of scope for World Timeline — see explicit non-composition in iteration architecture §9) |
| **Default layer** | **Brief if `block_type=era` data exists; else Narrative** with honest Brief empty-state in layer chrome |
| **Layer switcher placement** | Timeline canvas header (layer chrome + breadcrumbs) — see `iterations/v1.123/specs/layer-feel-differentiation.md` §3.4 |
| **Switcher modes** | Explicit segmented control (Brief \| Narrative) AND optional semantic zoom past thresholds (P4 owns threshold numbers; see `layer-feel-differentiation.md` §3.2) |
| **One-click rule** | Brief is one click from Narrative and vice versa |
| **Carrier contract** | Brief = `WorldKbGraphResponse.entities[block_type=era]` projection (era is a cross-profile world-shape marker, not a profile-specific category); Narrative = V1.122 `WorldKbGraphResponse.entities[block_type=event]` projection (V1.122 §3.3.2 preserved) — see iteration architecture §2 (Brief-on-KeyBlock LOCK) + §8 (data composition) |
| **Write boundary** | Brief + Narrative both edit World-scoped KeyBlocks via `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` (`kb.patch_entity`, V1.73). Brief-era writes use `block_type: "era"` in the patch. No new write route. |
| **Conflict policy** | Reuses V1.73 `WorldKbConflictError` (409) + `WorldKbValidationError` (422) for both layers. No new conflict DTO. — see iteration architecture §6 |
| **Empty-state honesty** | Brief empty (no `block_type=era` entities) → default to Narrative layer with Brief empty-state chrome explaining "Brief shows the world's shape across ages. Switch to Narrative to browse events, or add era markers when the Brief carrier is ready." (P4 owns final i18n strings.) |

#### `work-timeline` surface — NEW peer surface (V1.123)

The Work Timeline is a **fourth peer surface** on the Work Canvas shell (alongside Outline, Strategy, World KB). It is **not** the Work default — Work entry stays Outline (V1.118 preserved). The surface is reachable from Work Canvas shell nav at `/works/:workId/timeline` (or equivalent peer-route path — implementer's choice).

| Property | Contract |
|----------|----------|
| **Surface kind** | `CanvasSurfaceKind = "work-timeline"` (new additive enum value) |
| **Layer pair** | Narrative + Moment only (Brief out of scope for Work Timeline — see explicit non-composition in iteration architecture §9) |
| **Default layer** | **Narrative** (architect UX-risk override — see iteration architecture §7.3). The product spec §4.3 preference was "Moment when Scene/Beat data exists"; the architect overrides because the V1.72 `WorkOutline` wire has no Scene/Beat data today (Moment-default would surface persistent empty-state). Moment is one click away via the layer switcher. When the WorkOutline wire extends to expose scenes/beats (V1.124+), the default may flip to Moment. |
| **Layer switcher placement** | Work Timeline canvas header (layer chrome + breadcrumbs) |
| **Switcher modes** | Explicit segmented control (Narrative \| Moment) AND optional semantic zoom past thresholds (P4) |
| **One-click rule** | Moment is one click from Narrative and vice versa |
| **Carrier contract** | Narrative = `WorkOutline.timeline_events[]` projection (V1.72 preserved); optional client-side composition with bound World's `WorldKbGraphResponse.entities[block_type=event]` for cross-surface navigation binding. Moment = V1.108 `OutlineSceneNodeData` / `OutlineBeatNodeData` projection from `WorkOutline` (wire extension deferred to V1.124+; honest empty-state until then) — see iteration architecture §3 (Moment-on-Outline LOCK) + §8 (data composition) |
| **Adapter contract** | `WorkTimelineLayerAdapter` TypeScript signature — see iteration architecture §7.1 (full signature) + §7.2 (conformance rules). Mirrors the V1.122 `TimelineCanvasAdapter` stable-factory pattern (`createWorkTimelineCanvasAdapter(ctxRef)`). |
| **Write boundary** | Narrative writes route through `POST /v1/daemon/works/{work_id}/timeline/patch` (`timeline.patch_event`, V1.72). Moment is **read-only in V1.123** — edits route through Outline (`outline.patch_chapter` / `outline.patch_structure`) via "Edit in Outline" affordance. No new write route. |
| **Conflict policy** | Narrative writes reuse V1.72 `OutlineConflictError` (409) + `OutlineValidationError` (422). Moment has no direct writes (read-only); Outline adapter owns the write when the user navigates there. No new conflict DTO. — see iteration architecture §6 |
| **Empty-state honesty** | Moment empty (no Scene/Beat data) → default to Narrative with Moment empty-state chrome explaining "Moment is scene-precise and manuscript-anchored. Add scenes and beats in Outline, or switch to Narrative for events." (P4 owns final i18n strings.) Narrative empty (no `timeline_events[]`) → honest empty-state explaining how events appear on Work Timeline. |

#### Cross-layer navigation rules (LOCKED)

Cross-layer navigation is **within one Timeline surface**, not cross-surface:

| Direction | Surface | Author intent | Behavior |
|-----------|---------|---------------|----------|
| Brief → Narrative | World Timeline | "Drill into this era" | Narrative filters (or focuses) events whose `body.attributes.occurred_at` falls within the era's `start_hint`/`end_hint` when an era is selected; otherwise full Narrative |
| Narrative → Brief | World Timeline | "Zoom out to world shape" | Brief becomes prominent layer |
| Narrative → Moment | Work Timeline | "Drill into this scene" | Moment filters to moments realized by the selected event/chapter when bound; otherwise full Moment stack |
| Moment → Narrative | Work Timeline | "Zoom out to events" | Narrative becomes prominent layer |

**Cross-layer is NOT cross-surface.** Cross-surface navigation (Work Timeline Moment ↔ bound World Timeline Narrative; World Timeline Narrative ↔ Work Timeline Moment realizing it) is **P3 IA scope** — it uses explicit jump affordances ("View on World Timeline" / "View in Work Timeline"), not the layer switcher. The layer switcher UI MUST NOT pretend to be cross-surface navigation.

#### Per-layer empty-state honesty rules (LOCKED)

| Layer | Empty condition | Behavior |
|-------|-----------------|----------|
| **Brief** (World) | No `block_type=era` entities | Default to Narrative layer; show Brief empty-state in layer chrome ("No era markers yet — switch to Narrative to see events." + short why-Brief line + how to add era markers). P4 owns final i18n strings. |
| **Narrative** (World) | No `block_type=event` entities | Reuse V1.122 Timeline empty-state (`timeline-canvas-architecture.md` §7 — "This World's timeline is empty. Events you add through World KB or chapter extraction will appear here." + CTA → World KB peer). |
| **Narrative** (Work) | No `WorkOutline.timeline_events[]` | Honest empty-state: "This Work's timeline is empty. Events you add in Outline will appear here." + CTA → Outline peer. |
| **Moment** (Work) | No Scene/Beat data | Default to Narrative layer; show Moment empty-state in layer chrome ("No scene/beat data yet — switch to Narrative to see events." + CTA toward Outline beats). |

The adapter MUST NOT fabricate Brief eras from `updated_at` / `canonical_name` / non-era KeyBlocks. The adapter MUST NOT fabricate Moment scenes/beats from chapter titles alone. These rules extend the V1.122 §3.3 temporal-honesty discipline ("MUST NOT fabricate chronology from `updated_at`, `canonical_name`, `version`, or `sequence_no`") to layer data fabrication.

#### Layer-state persistence (LOCKED mechanism)

| Requirement | Contract |
|-------------|----------|
| **Survive surface switch** | World Timeline → World KB → back restores Brief/Narrative choice; Work Timeline → Outline → back restores Narrative/Moment choice |
| **Preferred encoding** | URL query `?layer=brief\|narrative\|moment` on the Timeline route (shareable, refresh-safe) |
| **Secondary** | React context / session store for in-shell switches without full navigation |
| **Invalid layer** | If URL asks for Moment on World Timeline → ignore, use Brief/Narrative; if Brief on Work Timeline → ignore, use Narrative/Moment |
| **Default when absent** | World: Brief-if-`era`-data-else-Narrative; Work Timeline: Narrative (architect UX-risk override — see iteration architecture §7.3) |
| **Test** | AC-V1123-23 layer-state-persistence test (P4) |

#### `wire_contracts_changed: true` verification (V1.123)

V1.123 P1 + P2 add the frontend `CanvasSurfaceKind = "work-timeline"` enum value + a Brief-on-KeyBlock carrier via a new wire `BlockType = "era"` enum value in `schemas/common/common.schema.json`. The single schema change is **additive** and triggers `wire_contracts_changed: true` (minimal — one enum value, one schema file, no new daemon route, no new DTO, no new conflict DTO). The 8-point V1.123 verification gate is pinned in the iteration-scoped architecture spec §4.3.



The V1.70 α implementation treats React Flow as a presentation and interaction model over domain-owned graph projections for the shipped Strategy read/overlay/Idea-steer slice. V1.71 β promotes the Strategy write operations (`strategy.patch_state`, `strategy.patch_transition`, `strategy.patch_prompt_template`) to schema/codegen-backed DTOs and Daemon API routes. V1.72 β promotes Outline+Timeline patch DTOs and routes. V1.73 β promotes World KB entity/candidate DTOs and routes. V1.74 β promotes typed World KB relationship DTOs and the `world_kb.patch_relationship` route. The graph-document shape below remains the shared design language for projections; for World KB relationships, `WorldKbEdgeData` is now backed by `WorldKbRelationshipProjection` rather than design-only prose.

#### Shared React Flow document shape

All three surfaces use one shell-level graph envelope before conversion to `@xyflow/react` `nodes` and `edges`:

```ts
type CanvasSurfaceKind = "strategy" | "work-outline-timeline" | "timeline" | "world-kb";
// V1.123 Draft overlay (architect seat 2 — see §3.3.3):
//   - `timeline` gains Brief↔Narrative layer switcher (Brief = `block_type=era` KeyBlock).
//   - The union gains a peer `"work-timeline"` (Work Timeline with Narrative↔Moment layer switcher;
//     Moment = V1.108 Scene/Beat projection from V1.72 `WorkOutline`).
// The authoritative frontend enum lives in `apps/web/src/components/canvas/canvas-surface-adapter.ts`
// (SSOT — V1.122 + V1.123 P1/P2 implementers extend it additively).

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
| **Timeline** *(V1.122 Draft overlay)* | `TimelineNodeData = WorldKbEntityProjection & { layoutHint: "event" \| "context"; occurredAtHint?: string }` — `block_type=event` entities project as `TimelineEventNode` on the when-axis; other entity kinds project as `TimelineKeyBlockNode` Context clusters. **No Fork marker nodes.** | `TimelineEdgeData = WorldKbEdgeData` (**verbatim reuse** of the V1.74 relationship edge DTO; no Timeline-specific edge types — `foreshadow`/`realizes`/`fork-from` are Work-outline projection labels, not Timeline DTOs) | Single graph source `WorldKbGraphResponse` (V1.73); write path `kb.patch_entity` only; conflict reuses `WorldKbConflictError` (409) + `WorldKbValidationError` (422). Full architect-locked contract: §3.3.2 + [`timeline-canvas-architecture.md`](../iterations/v1.122/specs/timeline-canvas-architecture.md). |

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

This supersedes the V1.65 whole-file outline PUT model for canvas surfaces: V1.65 could save a whole outline document because the UI was a document editor (`web-ui.md` §13.1, §13.5). The canvas model must instead address and validate individual nodes/edges. V1.71 promotes the 3 Strategy operations to schema-backed Daemon API contracts and daemon-owned persistence; V1.72 and V1.73 extend the same structured patch boundary to Outline+Timeline and World KB entities/candidates. Future canvas operations, including typed World KB relationship editing, must reuse this boundary rather than reintroducing raw file writes.

> **V1.75 Canvas-Pivot shipped.** V1.75 ships the outline canvas as the sole outline authoring surface. The V1.65 chapter-page whole-document TipTap outline editor is retired after parity-close: chapter outline prose is edited inside the selected Chapter node inspector, not as a whole-document editor. `outline.patch_chapter` now accepts `set.content` for the chapter's outline prose notes; the daemon persists that prose to the chapter's `outline_path` markdown file while preserving the work-level `outline_revision` CAS in `Outlines/outline.md`. This keeps the structured write boundary intact: canvas patches mutate outline metadata/prose only and never touch AI-owned body files under `body_path` / `Stories/**`. The V1.65 `PUT /chapters/{n}/outline` write route and `PutChapterOutlineRequest` DTO are removed in `@42ch/nexus-contracts` 0.11.0; the `GET /chapters/{n}/outline` read route remains as the inspector content preview.

#### Conflict policy vs host tool body writes

Orchestration may write prose or artifacts through host-tool paths such as `host_tool_handlers.rs` `body_path`. The canvas must not concurrently mutate those same raw files. Draft policy:

1. Canvas saves carry a base revision (`graphRevision`, `nodeRevision`, or equivalent domain version) from the last canonical projection.
2. Daemon rejects stale node/edge operations with a structured conflict error that identifies the changed node/file/object and recovery action.
3. UI keeps the user's draft operation list, refetches the canonical graph, and offers reapply/merge at node granularity where safe.
4. If orchestration is actively writing a node/body artifact, canvas editing for that node is read-only with a clear status label (`Nexus is writing this node…`).
5. Raw `body_path` conflicts are never resolved in the browser by loading and overwriting files; they are resolved by daemon-owned structured merges or explicit retry after refetch.

TipTap remains useful as an in-node editor for prompt snippets, outline fragments, notes, or constraints. It is not a whole-document manuscript editor and must not bypass the operation boundary.

### 3.6 Canvas → DESIGN.md token contract (B4)

V1.69 freezes the minimal credible token names that V1.70 canvas implementation will need. Repo-root [`DESIGN.md`](../../../DESIGN.md) + [`DESIGN.dark.md`](../../../DESIGN.dark.md) stub these as commented LEVEL placeholders (formerly under `apps/web/` pre-V1.98); V1.70 assigns concrete values when implementing the canvas.

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

### 3.8 Relationship to V1.67 Daemon API convergence

The canvas is a heavy Daemon API consumer: every graph node binds to list/detail data, every inspector needs typed update operations, and every execution overlay depends on consistent session/status responses. Therefore V1.67 P0 is not incidental hygiene; it is the foundation for V1.68 canvas work:

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
- Keep runtime/file/CLI identifiers as **preset** in V1.67 and until an explicit breaking-change plan authorizes a rename. This includes preset YAML, existing Daemon API routes, generated DTO names, and CLI command names.
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

### 4.5 Canvas entry defaults — World entry vs Work entry (V1.122 Draft overlay)

> **Product lock (V1.122).** Inverts the IA so an author meets a World's history first, not its entity graph. Work entry is unchanged.

The Canvas shell hosts four peer surfaces, but the **default surface on entry** differs by scope:

| Entry context | Route | Default surface | Peer surfaces (one click away via Canvas shell nav) |
| --- | --- | --- | --- |
| **World entry** (`/worlds/:worldId`) | `/worlds/:worldId` → **Timeline** (e.g. `/timeline` or index redirect); Worlds list pick-target updates from today's `/kb` to Timeline | **Timeline (World-building hero)** | Outline (Timeline-companion), Strategy, World KB |
| **Work entry** (`/works/:workId`) | `/works/:workId` → **Outline** (V1.118 canvas-first work shell — **unchanged this iteration**) | **Outline (Timeline-companion)** | (Work-scoped peers per V1.118) |

Reachability rules (MUST hold after P1):

1. **Outline is always one click away** from Timeline — no dead-end hero.
2. **World KB remains a peer** — the entity graph is not deleted; it only loses default World-entry status.
3. **Work Outline is not demoted** — authors writing chapters still open Works → Outline; Timeline is the World-building instrument, not a replacement for chapter planning.
4. **Empty Timeline is honest** — if a World has no `block_type=event` entities yet, show an empty-state explaining the spine (not a blank canvas or a silent redirect to World KB).
5. **No Outline→Timeline silent redirect** — authors who open a Work must not be bounced to World Timeline.

This rule makes concrete the domain spine-vs-projection model: **World + Timeline are the spine** (truth of the narrative universe); **Work + Outline + Manuscript are projections** onto that spine. Authors should feel: *World first for World building; Work first for chapter writing.* The optional Fork-badge header chrome (§3.3.2 sidecar) may render from `WorldState` on the Timeline surface.

### 4.6 User stories (steering loop)

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
- **V1.72 β — shipped (V1.72 P0)**: Outline+Timeline Canvas β slice — Work → Volume → Chapter → Scene/Beat graph projection + timeline lane + foreshadow edges + 3 structured patch routes (`outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event`) + `outline_revision:` markdown frontmatter graphRevision + structured conflict error + UI retry/merge (outline-flavored copy) + non-spatial alternate views + atomic outline markdown persistence. `@42ch/nexus-contracts` 0.7.0 → 0.8.0 (additive outline DTOs). 8 outline/timeline DESIGN.md canvas-write tokens added. See V1.72 compass [`v1.72/delivery-compass.md`](../../iterations/v1.72/delivery-compass.md).
- **V1.73 β — shipped (V1.73 Track A)**: Canvas World KB surface (Draft §3.3 surface 3) promoted through additive World KB DTOs and 4 Daemon API routes: graph projection, candidates projection, `kb.patch_entity`, and `kb.promote_candidate`. Builds on entity-scope-model §5.5 promotion state machine, Canvas Shell from V1.70, Strategy β write patterns from V1.71, and Outline+Timeline β patterns from V1.72. Uses per-row OCC (`expected_version` → `version`) and structured 409/422 error DTOs.
- **V1.74 β — shipped**: World KB relationships surface — first-class typed relationship edges/CRUD beyond V1.73 source-anchor provenance projection, with `world_kb.patch_relationship`, per-row OCC on `kb_relationships.revision`, directed + `symmetric` read projections, `WorldKbRelationshipKind` + `custom_label`, and complete non-spatial relationship table parity.
- **V1.76 γ — shipped**: World KB relationship γ — extraction-driven + author-curated + confidence-weighted. `nexus.llm.extract` proposes relationship candidates from chapter text (entity pairs + `relation_type` + `confidence` + `source_quote`); suggestions land behind a `needs_review=1` / `source='extraction'` gate (entity-scope-model §5.6.7). Suggested edges are dashed/default-hidden (`?include_suggested=true` surfaces them); confirmed edges are solid/default-visible. Symmetric reverse projection still derives from one stored row. Confidence uses the PM-locked stepped bands (low <0.4 / mid 0.4–<0.7 / high ≥0.7 → stroke 1/2/3px + opacity 30/60/100% + DESIGN.md red/amber/green badge tokens). The Suggested pane is the author's sole triage surface (per-row Promote/Delete + bulk Promote all).
