# Canvas / Compute Readiness Research (V1.116 P2)

> Iteration-scoped research brief for V1.116 P2. **Research + writing only —
> no production code.** Not a normative `{SPECS_DIR}` Master. Output stays in
> this iteration workspace until a later iteration promotes chosen work.

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-13-v1.116-canvas-compute-readiness-research` |
| **Tier** | Should |
| **Audience** | Next-iteration PM + architect (direction pick) |
| **primary plan** | `.mstar/plans/2026-07-13-v1.116-canvas-compute-readiness-research.md` |
| **Output artifact** | This file (filled by P2 execution) + three candidate skeleton sections |

## Problem framing

V1.114 made Canvas and Compute **visible as foundations**. V1.115 made those
foundations **honest and reusable** (adapter complete across product
orchestrators; compute manifest single-source). The roadmap names three
deep-dive candidates for a later iteration:

1. **Strategy `onConnect`** — inner-graph groups (canvas capability depth)
2. **Compute state editor** — human-writable module `body.state` (compute depth)
3. **5th canvas surface** — compute graph / session replay on the adapter

Before PM picks one for V1.117+, V1.116 must answer: **are the foundations
actually ready, or are there hidden gaps that would explode mid-implement?**

This plan produces **evidence**, not a new product surface.

## User value

| Who | Why they care |
| --- | --- |
| **Next-iteration PM** | Can enter Prepare on a chosen deep-dive with gap list + skeleton already drafted — no blind direction pick. |
| **Architects / implementers** | Know which foundation claims are verified vs aspirational; avoid rediscovering adapter/compute gaps mid-sprint. |
| **Authors (deferred)** | Next capability wave lands on proven rails instead of half-finished foundations. |

## Goals

1. **Canvas adapter extensibility audit** — can a 4th/5th orchestrator adopt
   `useCanvasSurface` / `CanvasSurfaceAdapter` cleanly after V1.115?
2. **Compute state-write readiness audit** — is module state still read-mostly?
   What write boundary / OCC / conflict model is missing for a state editor?
3. **Gap analysis** for each of the three candidates (wire/API/UI/domain).
4. **Recommended priority ordering** with product + risk rationale.
5. **Three candidate spec skeletons** (Problem / Scope / key Interfaces / open
   questions) — enough for next iteration to enter Prepare without a blank page.

## Non-goals

- Implementing any candidate
- Full Draft / normative Master specs (skeletons only)
- Wire schema changes or production code
- Writing new `{KNOWLEDGE_DIR}` docs (iteration-close compound only)
- Closing residual burn-down outside readiness implications

## Why Should (not Must)

P2 is **Should**:

- It does **not** unblock authors on first launch (that is P0).
- It does **not** unblock daily maintainer honesty (that is P1).
- It **does** prevent a blind deep-dive next iteration — high leverage, not a
  ship blocker for V1.116 author-facing Done.

If capacity forces a cut, **cut P2 before P0/P1**. Prefer shipping a thinner
readiness note over skipping P0 detection honesty.

## Target state (research Done)

A single readiness document (this file, completed) that a reader who was not
in grill-me can use to:

1. Understand foundation readiness for Canvas and Compute.
2. Compare three candidates with explicit gaps.
3. See a recommended pick order with rationale.
4. Open a skeleton and start Prepare for the chosen candidate.

## Acceptance criteria (PM/maintainer-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P2-1** | Readiness assessment delivered in this workspace file | File exists under `v1.116/specs/canvas-compute-readiness.md` with audit sections filled (not stub headings only) |
| **AC-P2-2** | Canvas adapter extensibility verdict is evidence-based | Section cites concrete code/spec paths; states ready / ready-with-gaps / not-ready |
| **AC-P2-3** | Compute state-write readiness verdict is evidence-based | Section cites runtime/schema paths; states what write path exists or is missing |
| **AC-P2-4** | Three candidate skeletons present | Each of Strategy `onConnect`, compute state editor, 5th surface has Problem / Scope / key Interfaces / open questions |
| **AC-P2-5** | Recommended priority ordering with rationale | Explicit ordered list + why #1 over #2/#3 for next iteration |
| **AC-P2-6** | Compass roadmap "Immediate next" remains aligned | Compass references this readiness output as the pick enabler |

## Candidate inventory (locked — skeletons only)

| Candidate | Pillar | Product question |
| --- | --- | --- |
| Strategy `onConnect` for inner-graph groups | Canvas depth | Can authors wire group connections on Strategy without a second graph model? |
| Compute state editor | Compute depth | Can authors inspect and edit module state in human-readable form safely? |
| 5th canvas surface (compute graph / session replay) | Canvas breadth | Can a new surface reuse the V1.115 adapter recipe without forking shell code? |

## Skeleton template (each candidate)

Each skeleton section must include:

```markdown
### Candidate: <name>

#### Problem
#### Scope (in / out)
#### Key interfaces (known or hypothesized)
#### Open questions (for next Prepare)
#### Dependencies on foundation gaps (from audits above)
```

## Product decisions (locked)

| Decision | Choice | Rationale |
| --- | --- | --- |
| Deliverable shape | Readiness spec + 3 skeletons (one file OK) | Grill-me #4 |
| Implementation | None this iteration | Stabilize + research only |
| Priority tier | Should | Not author first-impression; enables next pick |
| Artifact boundary | `v1.116/specs/` only | No premature `{SPECS_DIR}` / knowledge promotion |

## Architect decisions (seat 2 — resolved)

### AD-1: Evidence-bar checklist for readiness verdicts

Every readiness verdict ("ready" / "ready-with-gaps" / "not-ready") must be
backed by concrete evidence. The checklist:

| Verdict | Bar (ALL must hold) |
| --- | --- |
| **ready** | (1) Production code path exists and is exercised by ≥1 passing test; (2) Interface contract is typed (not `any`/`untyped`); (3) At least one product orchestrator/consumer uses it end-to-end today; (4) No open residual blocks the candidate's write path |
| **ready-with-gaps** | (1) Core code path exists; (2) But: missing test coverage, OR missing typed boundary at the extension point, OR one open residual affects the candidate but has a documented workaround |
| **not-ready** | (1) Code path does NOT exist, OR (2) exists but is stub/`todo!()`, OR (3) has a blocking residual with no workaround, OR (4) the extension surface would require a breaking change to the existing contract |

**Canvas adapter extensibility audit — code paths to cite:**

| Audit dimension | Code path to read |
| --- | --- |
| Adapter contract | `apps/web/src/canvas/` — `useCanvasSurface` hook + `CanvasSurfaceAdapter` interface (typed props, edge/node ops, layout trigger) |
| Existing consumers | Strategy, Outline+Timeline, World KB orchestrators — verify each implements the adapter cleanly |
| Extension point | What a 4th/5th orchestrator must provide: node types, edge types, layout fn, panel components. Is this a typed interface or ad-hoc? |
| Knowledge pattern | `.mstar/knowledge/architecture-patterns/canvas-surface-implementation-pattern.md` — V1.115 recipe |
| Spec | `.mstar/specs/canvas-strategy-surface.md` — normative contract |

**Compute state-write readiness audit — code paths to cite:**

| Audit dimension | Code path to read |
| --- | --- |
| Current runtime | `crates/nexus-daemon-runtime/src/api/handlers/` — compute module routes (are they read-only GET, or is there a write POST?) |
| Module state shape | `crates/nexus-contracts/` — `ModuleManifest` / `ModuleDetail` / module `body.state` typed shape |
| Write boundary | Does any handler accept state mutations? Is there an OCC/version field? Conflict resolution? |
| Compute ABI | `.mstar/specs/` — compute module manifest / ABI spec (is `body.state` documented as mutable?) |

### AD-2: Candidate skeleton depth

Each candidate skeleton must include **key interfaces** — not just names, but
the hypothesized function/component signatures a Prepare-phase spec would
start from. Examples:

- **Strategy `onConnect`:** `onConnect(sourceNodeId, targetNodeId, edgeConfig)
  => void | ConflictResult` — what wire message? What canvas event?
- **Compute state editor:** `PATCH /v1/daemon/compute/modules/{id}/state`
  with `If-Match` header (OCC)? Or a command-style `SetState` operation?
- **5th surface:** `CanvasSurfaceAdapter` implementation for compute graph —
  what are the node/edge types? What layout algorithm?

These are **hypothesized**, not final — but they give the next PM a concrete
starting point instead of a blank page.

### AD-3: Recommendation strength

Present a **decision matrix** (not just an ordered list) so PM can re-evaluate
if product priorities shift. Columns: candidate, foundation readiness, effort
estimate (S/M/L), author impact (high/med/low), risk of mid-implement gap
discovery. Then state the architect's recommended #1 pick with rationale —
but PM owns the final call.

## Mapping to plan tasks

| AC | Plan tasks |
| --- | --- |
| AC-P2-1..3 | T1 Canvas audit + T2 Compute audit |
| AC-P2-4..5 | T3 gap analysis + skeletons + priority |
| AC-P2-6 | Compass alignment (already drafted; confirm at research close) |

---

## Research output (filled during P2 execute)

### Canvas adapter extensibility audit

**Verdict: ready-with-gaps**

The adapter contract is typed, generic, and proven by all three product
surfaces. A 4th/5th orchestrator can adopt it cleanly, but two documented gaps
prevent an unqualified "ready."

#### Evidence

| Audit dimension | Code path cited | Finding |
| --- | --- | --- |
| Adapter contract | `apps/web/src/components/canvas/canvas-surface-adapter.ts` (48 lines) | Typed generic `CanvasSurfaceAdapter<TGraph, TNodeData, TEdgeData>` with 8 members: `surfaceKind`, `projectGraph`, `nodeTypes`, `edgeTypes?`, `layoutOptions?`, `adaptConflict?`, `renderInspector?`, `renderAltView?`, `summarizeGraph`. Fully typed — no `any` in the contract. |
| Composition hook | `apps/web/src/components/canvas/use-canvas-surface.ts` (163 lines) | `useCanvasSurface(adapter, queryResult)` owns projection memo, conflict state, alt-view toggle, inspector selection, viewport caching, and layout delegation. Generic over `<TGraph, TNodeData, TEdgeData>` — a new surface needs no hook changes. |
| Auto-layout | `apps/web/src/components/canvas/use-auto-layout.ts` (230 lines) | `useAutoLayout(nodes, edges, options?)` — dagre opt-in via `layoutOptions`. Manual-override detection, `relayout()`, `hasSuppliedPositions` (W003). Tested in `__tests__/use-auto-layout.test.ts`. |
| Existing consumers (3 of 3) | `strategy-canvas/strategy-canvas-adapter.tsx`, `outline-canvas/outline-canvas-adapter.tsx`, `world-kb/world-kb-canvas-adapter.tsx` | All three product surfaces migrated by V1.115 (knowledge layer 14). Each implements the adapter via a stable factory reading from a mutable `ctxRef`. |
| Recipe documented | `.mstar/specs/canvas-strategy-surface.md` §3.3.1 (lines 141–219) | Interface, hook composition steps, 6-step "add a new surface" recipe, worked examples (Strategy + World KB). |
| Knowledge pattern | `.mstar/knowledge/architecture-patterns/canvas-surface-implementation-pattern.md` layers 12–14 | Adapter extraction (V1.114), dagre auto-layout (V1.114), contract convergence (V1.115 — W001 passthrough, W002 node-param leak, W003 supplied-positions). |
| Tests | `__tests__/use-canvas-surface.test.ts` (293+ lines), `__tests__/use-auto-layout.test.ts`, per-surface adapter tests | Hook behavior exercised: projection memo, conflict auto-populate, alt-view, selection. Adapter implementations tested per-surface. |

#### Gaps (why "ready-with-gaps", not "ready")

1. **`CanvasSurfaceKind` is a closed union** (`canvas-surface-adapter.ts` lines 6–10):
   `'strategy' | 'outline' | 'world-kb-entities' | 'world-kb-relationships'`.
   A 5th surface MUST manually add a value here. This is a one-line edit, not a
   guarded extension surface — no test enforces that every `surfaceKind` has a
   corresponding adapter or viewport key.

2. **`world-kb-relationships` is a reserved-but-unimplemented orchestrator**
   (knowledge layer 14): the kind exists in the union but relationship editing
   currently lives inside the World KB entity adapter's edge inspector. So there
   are 3 adapters for 4 enum values today.

3. **No adapter conformance guardrail.** The W001 (passthrough `projectGraph`)
   and W002 (ignored `node` param) convergence failures (knowledge layer 14)
   were caught in QC, not by an automated test. A new adapter copying the
   interface could repeat these bugs. The lessons are in the knowledge doc but
   not enforced as a test contract (e.g., "renderInspector routes from
   `node.data`, not `ctxRef.current.selection`").

4. **Dagre compound-graph crash** (knowledge layer 14): Outline keeps
   `layoutOptions: undefined` because dagre crashes on its compound structure. A
   5th surface with compound graphs cannot safely opt into dagre without hitting
   this — it must supply its own positions (`hasSuppliedPositions: true`) or
   avoid compound nesting.

#### What a 4th/5th orchestrator must provide

Per the §3.3.1 recipe (`canvas-strategy-surface.md` lines 203–214):

1. Define `TGraph` (daemon DTO shape) + `TNodeData` / `TEdgeData` types.
2. Implement `CanvasSurfaceAdapter` — at minimum `projectGraph`, `nodeTypes`,
   `summarizeGraph`; optionally `edgeTypes`, `layoutOptions`, `adaptConflict`,
   `renderInspector`, `renderAltView`.
3. Use a stable factory (`useMemo([])`) reading mutable values from a `ctxRef`
   so the projection memo is not invalidated.
4. Call `useCanvasSurface(adapter, queryResult)` in the orchestrator.
5. Add the new `CanvasSurfaceKind` value.
6. Wire the structured write boundary (§3.5) if the surface edits.

The hook, layout, and shell need **zero changes** for a read-only 5th surface.

---

### Compute state-write readiness audit

**Verdict: not-ready**

There is no write path, no module-instance state resource, and no conflict
model for compute module state. The foundation is read-only manifest discovery
only.

#### Evidence

| Audit dimension | Code path cited | Finding |
| --- | --- | --- |
| Runtime handlers | `crates/nexus-daemon-runtime/src/api/handlers/compute_modules.rs` (53 lines) | **Two handlers, both GET.** `list_modules` → `GET /v1/daemon/compute/modules`; `get_module` → `GET /v1/daemon/compute/modules/{module_id}`. No POST/PATCH/PUT/DELETE. Module-level doc comment (line 2): *"Read-only endpoints for discovering installed WASM compute modules."* |
| Route registration | `crates/nexus-daemon-runtime/src/api/mod.rs` `compute_routes()` (lines 481–491) | Router binds only `get(handlers::compute_modules::list_modules)` and `get(handlers::compute_modules::get_module)`. Comment (line 480): *"Read-only discovery endpoints."* |
| Module detail shape | `schemas/daemon-api/compute/module-detail.schema.json` (59 lines) | The `ComputeModuleDetail` is the **manifest.json** shape. Fields: `module_id`, `name`, `version`, `nexus_abi_version`, `required_key_block_types`, `compute_export`, `init_export`, `schemas` (shape descriptors), `host_functions`, limits. There is **no `body.state` field** — `schemas.key_block_state` is a JSON Schema *fragment describing what KeyBlock state should look like*, not state data. |
| Module summary shape | `schemas/daemon-api/compute/module-summary.schema.json` (25 lines) | `ComputeModuleSummary`: `module_id`, `name`, `version`, `required_key_block_types`, `status` (`ok`/`broken`). No state field. |
| ABI state model | `.mstar/specs/compute-module-abi.md` §1 (line 24) | *"A compute module is a **stateless pure function**. Each invocation receives a fresh envelope and returns a ComputeOutput."* Per-invocation sandbox; fresh wasmtime `Store` + `Instance` per call; no cross-call state. |
| Where state lives | `compute-input.schema.json` / `compute-output.schema.json` | State lives on **KeyBlock bodies** (`key_blocks[].body.state`), not on modules. `ComputeOutput.state_delta` applies `add`/`sub`/`set` to KeyBlock body paths (e.g., `character.current_hp`). The module declares state *shapes* (manifest `schemas.key_block_state`); it holds no state. |
| Spec confirms read-only | `compute-module-abi.md` §7.5 (line 404) | *"exposed read-only through the daemon registry API."* §7.6 (line 418): all manifest fields are wire-promoted; no runtime-only split today. |

#### What a state editor would need (from scratch)

The "compute state editor" candidate has a **conceptual problem**: compute
modules are stateless. There is no module-instance state to read or write. The
state that *does* exist (KeyBlock `body.state`) is already editable through the
World KB canvas (knowledge layer 2: `kb_key_blocks.revision` + OCC +
`world_kb.patch_entity`). A "compute state editor" must therefore resolve one
of two framings before any code is written:

- **B1 — KeyBlock state editor (compute-aware):** surface the `body.state` of
  computable KeyBlocks, validated against the declaring module's
  `manifest.schemas.key_block_state`. This *partially exists* via World KB but
  lacks: (a) compute-schema validation at edit time, (b) a view showing which
  module declared each state field. **Foundation: partial** (KeyBlock patch +
  OCC exists; compute-schema validation does not).

- **B2 — Module manifest/invocation editor:** edit module wiring, invocation
  params, or reconfigure module-to-capability bindings. This is configuration
  editing, not state editing, and would need an entirely new daemon write
  surface. **Foundation: none.**

Either framing needs: a new write route (POST/PATCH on a new resource), an OCC
field on the target entity, a conflict model (reuse the layer 2 pattern), and a
typed DTO. None of these exist for compute modules today.

---

### Candidate: Strategy `onConnect` (inner-graph groups)

#### Problem

V1.109 shipped spatial edge editing (`onConnect`) for **outer** state
transitions (knowledge layer 8): draft edge → edge inspector →
`patch_transition(op:"create")` → conflict modal. But **inner-graph groups** —
compound `strategy-group` nodes containing `strategy-inner` child nodes with
`depends_on` edges — have **no spatial editing path**. Authors cannot create,
rewire, or delete inner-graph edges by dragging or via keyboard.

Evidence:
- `edge-create-dialog.tsx` (keyboard alternative) offers only
  `'next' | 'branch' | 'default'` — the outer transition kinds. No
  `depends_on`.
- `strategy-graph.ts` lines 221–244: inner-graph `depends_on` edges are a
  **read-only projection** from `manifest.inner_graphs[graphId].nodes[].depends_on`.
  No write op exists for them.
- `StrategyEdgeData.transitionKind` (strategy-graph.ts line 51) includes
  `'depends_on'` in the type union, but no daemon `patch_*` route accepts it as
  a create target.

#### Scope (in / out)

**In:**
- Wire DTO + daemon op for creating/updating/deleting inner-graph edges
  (`depends_on` and potentially a new "group connection" kind for cross-group
  links).
- Spatial `onConnect` path for inner nodes (draft → inspector → commit).
- Keyboard alternative (extend `edge-create-dialog` or a sibling) for
  inner-graph edges.
- Conflict modal reuse (command-aware retry, layer 8 pattern).
- Adapter: the `localEdges` draft-merge mechanism
  (`strategy-canvas-adapter.tsx` lines 137–147) is already in place and
  reusable — no adapter contract change needed.

**Out:**
- Editing inner-graph node identity / `kind` / `template_file` (that is preset
  YAML body editing, a separate concern).
- Cross-surface edge kinds (this is Strategy-only).
- Persisted node positions (still ephemeral per V1.114 layer 13).

#### Key interfaces (hypothesized)

```ts
// Wire: extend the existing patch_transition DTO or add a sibling.
// Hypothesis A — additive op on existing route:
interface PatchInnerGraphEdgeRequest {
  preset_id: string;
  graph_id: string;           // inner_graphs key
  op: 'create' | 'update' | 'delete';
  source_node_id: string;     // inner node id
  target_node_id: string;     // inner node id
  expected_revision: number;  // OCC (reuse preset revision)
}

// Daemon:
// POST /v1/daemon/presets/{id}/inner-graphs/{graphId}/edges
//   → 200 InnerGraphEdge | 409 StrategyConflictError

// Canvas: onConnect handler (spatial) — reuses localEdges draft-merge.
function onInnerConnect(connection: Connection): void {
  // Draft into ctxRef.current.localEdges (no daemon call yet).
  // Open inner-edge inspector → author sets dependency kind.
  // On commit: patchInnerGraphEdge(...) → 409 → conflict modal (retry callback).
}

// Canvas: keyboard alternative.
interface InnerEdgeCreateDialogArgs {
  graphId: string;
  sourceNodeId: string;
  targetNodeId: string;
}
```

#### Open questions (for next Prepare)

1. **Does the daemon preset validator already support inner-graph edge
   mutations**, or does `patch_transition` need a parallel `patch_inner_edge`
   route? (Read `crates/nexus-orchestration` preset handler before locking
   scope.)
2. **Cross-group connections**: should an inner node in group A connect to an
   inner node in group B? If so, the edge kind may need to be a new
   `'cross_group'` rather than `'depends_on'` (which is intra-group today).
3. **Dagre compound layout**: inner-graph nodes use `parentId` + `extent:
   'parent'`. Does adding edges between them trigger the dagre compound crash
   (knowledge layer 14 gap 4)? If so, inner-graph onConnect may need
   `hasSuppliedPositions: true` or a layout workaround.
4. **Conflict modal flavor**: inner-graph edges have no "current state" body to
   show side-by-side (they're structural, like transitions). Does the
   StrategyConflict modal need a structural variant?

#### Dependencies on foundation gaps

- **Adapter: ready** — `localEdges` draft-merge already exists and is used for
  outer-edge drafts. No contract change.
- **Wire: gap** — no op for inner-graph edge creation. Additive (same
  schema→codegen→Rust one-commit pattern as V1.109's `op` field).
- **Dagre compound: risk** — may need the same `layoutOptions: undefined`
  workaround as Outline, or `hasSuppliedPositions`.
- **Conflict modal: ready** — reuse layer 8 command-aware retry.

---

### Candidate: Compute state editor

#### Problem

Authors want to inspect and edit compute module state in human-readable form
safely. But compute modules are **stateless pure functions**
(`compute-module-abi.md` §1) — there is no module-instance state to edit. The
candidate as stated has a conceptual ambiguity that must be resolved before
implementation.

#### Scope (in / out)

**In (after reframing — see Open questions):**
- Clarify whether the target is (B1) compute-aware KeyBlock state editing, or
  (B2) module manifest/invocation editing.
- If B1: surface computable KeyBlock `body.state`, validate against the
  declaring module's `manifest.schemas.key_block_state` at edit time.
- If B2: new daemon write surface for module configuration.
- Conflict model (OCC + modal) per knowledge layer 2.

**Out:**
- Editing the WASM module itself (binary, exports, host functions).
- Editing `ComputeOutput.state_delta` semantics (that is the module's runtime
  contract, not an author-editable surface).
- Live module invocation / re-running compute from the editor (separate
  orchestration concern).

#### Key interfaces (hypothesized — B1 framing)

```ts
// B1: compute-aware KeyBlock state editor.
// The KeyBlock patch surface already exists (world_kb.patch_entity);
// the new work is compute-schema validation + a compute-aware view.

// Hypothesized daemon addition: validate state against module schema on patch.
// (May not need a new route — extend the existing patch_entity validator
//  to cross-check against manifest.schemas.key_block_state when the block
//  is computable.)

// Canvas: a compute-state inspector (not a full surface).
interface ComputeStateInspectorProps {
  keyBlockId: string;
  blockType: string;               // e.g. "character"
  state: Record<string, unknown>;  // body.state
  declaringModule: ModuleSummary | null;  // which module owns this schema
  stateSchema: JsonSchemaFragment | null; // manifest.schemas.key_block_state[blockType]
  expectedRevision: number;
  onPatch: (delta: StateDelta[]) => Promise<void>;
}
```

#### Open questions (for next Prepare)

1. **Conceptual framing (blocking):** is this a KeyBlock state editor with
   compute-schema awareness (B1), or a new module-configuration editor (B2)?
   The candidate name says "module `body.state`" but modules have no body.state
   — KeyBlocks do. **This must be resolved before any code.**
2. **If B1:** does the existing `world_kb.patch_entity` validator need to
   cross-reference `manifest.schemas.key_block_state`? Where does the daemon
   load module manifests to perform this validation?
3. **If B1:** is this a new canvas surface, or an inspector inside the existing
   World KB surface? (The adapter is ready for a 5th surface, but an inspector
   may be simpler.)
4. **OCC scope:** KeyBlocks already have OCC (`kb_key_blocks.revision`). Is
   there a new concurrency threat from compute module writes
   (`ComputeOutput.state_delta`) vs. human edits that the current OCC does not
   cover?

#### Dependencies on foundation gaps

- **Write path: not-ready** — no compute-specific write route exists. If B1,
  the KeyBlock patch route exists but lacks compute-schema validation. If B2,
  nothing exists.
- **State resource: not-ready** — no module-instance state exists in the ABI.
  Must reframe to KeyBlock state (B1) or module config (B2).
- **Conflict model: ready (if B1)** — `kb_key_blocks.revision` + layer 2 OCC +
  conflict modal. Not-ready if B2 (new resource, new OCC field).
- **Adapter: ready-with-gaps** — if this becomes a 5th canvas surface, the
  adapter contract supports it; if it's an inspector, no adapter change needed.

---

### Candidate: 5th canvas surface (compute graph or session replay)

#### Problem

Can a new surface reuse the V1.115 adapter recipe without forking shell code?
Two candidate domains: (a) **compute graph** — static dependency graph of
installed compute modules and their key-block-type flows; (b) **session
replay** — temporal replay of orchestration steps within a session.

#### Scope (in / out)

**In:**
- Add a new `CanvasSurfaceKind` value (e.g. `'compute-graph'` or
  `'session-replay'`).
- Implement `CanvasSurfaceAdapter` for the chosen domain.
- `projectGraph`: daemon DTO → React Flow nodes/edges.
- `nodeTypes` / `edgeTypes`: custom rendering.
- `summarizeGraph`: a11y summary.
- Optionally `renderInspector`, `renderAltView`, `layoutOptions`.

**Out:**
- Write boundary (both candidate domains are read-only in their first
  iteration).
- Editing module wiring or session state from the graph.
- Real-time session streaming (replay is a snapshot or stepped playback, not a
  live feed).

#### Key interfaces (hypothesized)

```ts
// Compute graph surface — static module dependency view.
interface ComputeGraphPayload {
  modules: ComputeModuleSummary[];  // from GET /v1/daemon/compute/modules
  // Key-block-type flows derived from required_key_block_types.
}

interface ComputeGraphNodeData extends Record<string, unknown> {
  moduleId: string;
  moduleName: string;
  requiredKeyBlockTypes: string[];
  status: 'ok' | 'broken';
}

interface ComputeGraphEdgeData extends Record<string, unknown> {
  flowKind: 'consumes' | 'produces';  // key-block-type flow direction
}

// Session replay surface — temporal orchestration step view.
interface SessionReplayPayload {
  sessionId: string;
  steps: Array<{
    stepId: string;
    taskType: string;        // capability / inner_graph / acp_prompt
    status: 'pending' | 'running' | 'done' | 'failed';
    startedAt: string;
    completedAt?: string;
    dependsOn: string[];
  }>;
}

// Adapter implementation (same shape for both):
const adapter: CanvasSurfaceAdapter<ComputeGraphPayload, ComputeGraphNodeData, ComputeGraphEdgeData> = {
  surfaceKind: 'compute-graph',  // ← new CanvasSurfaceKind value
  nodeTypes: computeGraphNodeTypes,
  projectGraph(payload) { /* map modules → nodes, key-block-type flows → edges */ },
  summarizeGraph(payload) { /* a11y string */ },
  // layoutOptions: undefined (supply own positions, avoid dagre compound crash)
  //                OR { hasSuppliedPositions: true } if dagre is desired.
};
```

#### Open questions (for next Prepare)

1. **Which domain first?** Compute graph is low-value (static, small — a
   handful of modules). Session replay is medium-value (debugging/trust) but
   needs a session-step read endpoint that may not exist yet.
2. **Session step endpoint:** does `GET /v1/daemon/orchestration/sessions/{id}`
   return per-step detail with dependencies, or only a summary? (Read
   `crates/nexus-daemon-runtime/src/api/handlers/orchestration/` before
   locking scope.)
3. **Read-only confirmation:** neither candidate needs a write boundary in V1.
  Confirm no `adaptConflict` / `renderInspector` with save is needed.
4. **Layout strategy:** both are likely small graphs (<20 nodes). Supply own
  positions (grid/layered) and set `hasSuppliedPositions: true`, or opt into
  dagre and risk the compound crash if nested.

#### Dependencies on foundation gaps

- **Adapter: ready-with-gaps** — contract + hook + recipe documented. The only
  required edit is adding the `CanvasSurfaceKind` value (trivial). The dagre
  compound crash (gap 4) is a risk if the graph is nested.
- **Domain projection: greenfield** — no existing projection for compute
  dependency flows or session step graphs. This is the real work.
- **Write boundary: N/A** — both candidates are read-only in V1.
- **Data source:** compute graph reuses `GET /v1/daemon/compute/modules`
  (exists). Session replay may need a new/enhanced session-detail endpoint.

---

### Recommended priority ordering

#### Decision matrix

| Candidate | Foundation readiness | Effort | Author impact | Risk of mid-implement gap | Overall |
| --- | --- | --- | --- | --- | --- |
| **A: Strategy inner-graph `onConnect`** | ready-with-gaps (adapter + localEdges ready; wire op is the only gap) | **M** | **High** — completes spatial editing for the most-used canvas surface | **Low-Med** — V1.109 layer 8 pattern is proven; extension is additive; dagre compound is a known workaround | **#1** |
| **C: 5th surface (session replay)** | ready-with-gaps (adapter ready; surfaceKind edit trivial) | **M–L** | Medium — debugging/trust tooling, not first-impression | Medium — domain projection is greenfield; session-step endpoint may not exist | **#2** |
| **C: 5th surface (compute graph)** | ready-with-gaps (adapter ready) | **S–M** | Low — static, small graph; nice-to-have | Low — reuses existing compute modules endpoint | **#3** (or defer) |
| **B: Compute state editor** | **not-ready** (no state resource, no write path, conceptual reframing needed) | **L** | Low-Med — nice-to-have, not blocking authors | **High** — "what IS module state?" is unresolved; may not be the right abstraction | **Defer** |

#### Architect's recommended #1 pick: Candidate A (Strategy inner-graph `onConnect`)

**Rationale:**

1. **Highest foundation readiness.** The adapter's `localEdges` draft-merge
   (`strategy-canvas-adapter.tsx` lines 137–147) is already implemented and
   used for outer-edge drafts. The V1.109 layer 8 pattern (draft → inspector →
   commit → conflict) is proven end-to-end. The only missing piece is the wire
   op for inner-graph edges — an additive schema→codegen→Rust change following
   the exact V1.109 precedent.

2. **Highest author impact.** Strategy is the most-used canvas surface (it
   drives the orchestration state machine). Completing spatial editing for
   inner-graph groups removes a capability gap authors hit when their presets
   use `inner_graphs` with `depends_on` — currently those edges are read-only
   projections with no authoring path.

3. **Lowest mid-implement risk.** The gap (wire op) is well-understood and
   additive. The dagre compound crash (gap 4) is a known issue with a known
   workaround (`hasSuppliedPositions: true` or `layoutOptions: undefined`). The
   conflict modal is reusable as-is (command-aware retry, layer 8).

4. **Effort is Medium, not Large.** The adapter, hook, draft mechanism, and
   conflict modal are all in place. The work is: one wire DTO + one daemon op +
   one `onConnect`/keyboard handler + one inner-edge inspector. This is a
   focused iteration, not a foundation-building one.

#### #2 pick (if debugging tooling is prioritized): Candidate C — session replay

Session replay is read-only (no write boundary, no conflict model), so it is
lower-risk than it appears. The adapter is ready. The real question is whether
the session-detail endpoint returns per-step dependency data. If it does, this
is a clean Medium-effort iteration that adds high-trust debugging value. If it
doesn't, the endpoint work pushes it to Large.

#### Defer: Candidate B (compute state editor)

This candidate should not be picked until the conceptual framing is resolved
(B1 vs B2). Picking it blind risks building a surface for an abstraction that
doesn't match the architecture (modules are stateless; state is on KeyBlocks).
If the next iteration wants compute depth, **reframe first** in Prepare:
clarify whether the goal is compute-aware KeyBlock state editing (B1, partial
foundation) or module configuration editing (B2, no foundation).
