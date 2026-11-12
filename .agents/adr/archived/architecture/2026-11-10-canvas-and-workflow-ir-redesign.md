# ADR: Canvas and Workflow IR Redesign — Strategy Orchestration Replacement

Status: implemented — split into three records on 2026-11-13
Archived: 2026-11-13 — split into three records
Superseded-by: ../../implemented/architecture/2026-11-10-workflow-ir-document-model.md, ../../implemented/architecture/2026-11-10-strategy-simulation-fence.md, ../../proposed/architecture/2026-11-10-canvas-composition-and-pattern-packages.md
Date: 2026-11-10
Scope: Detailed design for a **Canvas + Workflow IR** stack that **MAY completely replace** today's strategy-orchestration authoring and intake path (hand-authored `preset.yaml` → loader → graph-flow; Strategy Canvas as YAML explainability + node-granular patch). Lifecycle remains **Generate or Compose → Validate → Simulate → Approve → Apply → Execute**.
Source: migrated from the retired tracked specs corpus

## Problem

1. **Authoring SSOT is YAML, not a universal IR.** Generators, Canvas, validators, and pattern packages cannot share one typed document without repeatedly round-tripping through preset YAML and losing nest/compose semantics.
2. **Canvas projection is a parallel grammar.** TS `buildStrategyGraph` reconstructs graph meaning from parsed YAML; Rust loader reconstructs graph-flow Tasks from the same YAML. Two projections drift; neither owns compose/nest as first-class ops.
3. **Compose is missing.** Users cannot nest published patterns or Agent drafts as IR units; developers cannot ship versioned pattern packages as the primary reuse mechanism (today reuse = copy a sample directory and rename `preset.id`).
4. **No candidate lifecycle.** There is validate-on-disk and patch-on-installed-bundle, but no draft/approved IR states, no simulation fence, no explicit Approve before production Apply.
5. **Explainability-first Canvas locks the wrong ceiling.** Extending β patches forever still treats the human as a YAML surgeon; it does not unlock the Canvas **composition** duty the product direction now requires (#482 owner ruling). Explainability remains a **view** (applied-IR inspector), not the only job of Canvas.
6. **Migration debt concentrates in the loader.** Any IR design that only "emits preset.yaml forever" cannot retire the hand-authored main path. Dual-read is transitional; **import/export of `preset.yaml` bundles is retained** as a compatibility layer, not deleted at P6.

---

## Decision

### 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — not implement authority. Nothing here schedules work or claims a shipped feature. |
| **Document class** | Exploration |
| **Scope** | Detailed design for a **Canvas + Workflow IR** stack that **MAY completely replace** today's strategy-orchestration authoring and intake path (hand-authored `preset.yaml` → loader → graph-flow; Strategy Canvas as YAML explainability + node-granular patch). Lifecycle remains **Generate or Compose → Validate → Simulate → Approve → Apply → Execute**. |
| **Coordinates with** | [orchestration-engine.md](../../../../docs/specs/orchestration/orchestration-engine.md) (current Master: graph-flow + preset SSOT), [canvas-strategy-surface.md](../../../../docs/specs/surfaces/canvas-strategy-surface.md) (Strategy Canvas β), [docs/strategy-authoring.md](../../../../docs/strategy-authoring.md), [STRATEGY.md](../../../../STRATEGY.md) (Harness / Canvas pillars), `dynamic-workflow-and-simulation.md` ([PR #482](https://github.com/42ch-dev/nexus/pull/482) direction brief; path `.agents/adr/proposed/orchestration/` once merged — **this file is the detailed redesign**) |
| **Relationship to #482** | `dynamic-workflow-and-simulation.md` ([PR #482](https://github.com/42ch-dev/nexus/pull/482)) states product direction (universal IR, nest/compose, patterns, lifecycle, safety fences). **This document** answers *how* to redesign Canvas + IR so the **current strategy orchestration implementation can be fully retired** after a staged migration — schema, UX, APIs, loader replacement, acceptance criteria. |
| **Recommended merge / land order** | See §0.1 — merge **#482 before #483**; start implementation only after both are accepted as direction + detailed design (still not implement authority until Draft/Master promotion). |
| **Boundary vs Work artifacts** | Workflow IR is the SSOT for **strategy orchestration only** (the strategy graph). Durable Work-tree output under `Works/<work_ref>/` is **out of this document**. Today's implement authority for those trees remains the Shipped profile Masters ([workflow-profile.md](../../../../docs/specs/novel-writing/workflow-profile.md) and essay/script/game-bible siblings). A **sibling Exploration** ([PR #485](https://github.com/42ch-dev/nexus/pull/485)) proposes a Work Artifact IR + read/write ports as the *target* SSOT for that tree; it is **not yet merged and is not current authority**. This file MUST NOT absorb that design or cite its path as if it already lived in this tree. If Simulate would write Work artifacts, it MUST go through that future port's stub (no real `Works/` writes). |

The key words MUST, MUST NOT, SHOULD, and MAY are RFC 2119 terms for a conforming implementation of this proposal only (if adopted).

**Not implement authority.** Per [`docs/specs/README.md`](../../../../docs/specs/README.md), Exploration under `proposed/` never outranks the active orchestration Master. Adoption requires a later Draft/Master promotion and link-migration sweep.

### 1. Current strategy orchestration inventory (evidence)

#### 1.1 How strategies are authored today

Evidence: [docs/strategy-authoring.md](../../../../docs/strategy-authoring.md), samples under `strategy-samples/`.

- A **strategy** is a filesystem **preset bundle**: `preset.yaml` + `templates/`.
- Manifest shape is `PresetManifest` in `crates/nexus-contracts/src/local/orchestration/preset.rs` (`preset` header, `states`, optional `inner_graphs`, `signals`, `roles`).
- Authors hand-edit YAML; validation is `nexus42 preset validate` / `strategy-samples/validate.sh` calling the shared semantic facade (`loader_validate_manifest_compat` + path safety + assets-in-bundle).
- Two execution surfaces (same docs): **integrator path** (partner backend owns LLM + Connect writes) and optional **creator-host path** (install under `~/.nexus42/presets/<id>/`, schedules via service API).

#### 1.2 How strategies become executable graphs

Evidence: [orchestration-engine.md](../../../../docs/specs/orchestration/orchestration-engine.md) §3–§4, §8; `crates/nexus-preset` loader; `crates/nexus-orchestration` graph-flow adapter.

| Step | Today's owner | Coupling |
| --- | --- | --- |
| Parse YAML → `PresetManifest` | `nexus-preset` / contracts local types | YAML schema **is** the authoring SSOT |
| Semantic validate | Shared facade (§7.6) | Reachability, caps, template refs |
| Construct outer + inner `graph_flow::Graph` | Loader mapping rules §8.2 | Every state/edge kind maps to Tasks (`CapabilityTask`, `InnerGraphTask`, `JudgeTask`, `ManualWaitTask`, `AcpPromptTask`, …) |
| Source freeze | `PresetSourceIdentity` + content hash over manifest + referenced templates (`source_identity.rs`, engine §15 item 6) | Recovery refuses silent source swap |
| Execute | `GraphFlowEngine` over pinned `graph-flow` (`=0.8.1` literals in `crates/nexus-core` and `crates/nexus-orchestration` Cargo.toml; Master historically cites 0.8.0) | Sole execution backend; `WaitingForInput` + `wait_id` CAS for human continuation (engine §15 item 4) |

**Graph-of-graphs** is already the runtime model: outer state machine + inner DAGs as child Sessions (§3.4). Nesting exists at *execution* granularity; it is **not** a first-class *authoring* composition of reusable published patterns.

#### 1.3 How Strategy Canvas projects and edits today

Evidence: [canvas-strategy-surface.md](../../../../docs/specs/surfaces/canvas-strategy-surface.md) §1.1, §3.2; `apps/web/src/lib/canvas/use-strategy-data.ts`, `strategy-graph.ts`, `components/canvas/strategy-canvas/*`.

- **Read path:** fetch preset YAML → `parsePresetYaml` → `buildStrategyGraph` / adapter `projectGraph` → React Flow nodes/edges (outer states, inner-graph groups via `parentId`, converge joins, live session overlay).
- **Write path:** node-granular patches only — `strategy.patch_state`, `strategy.patch_transition`, `strategy.patch_prompt_template` with `base_revision` CAS against YAML `revision:`.
- **Product framing today ([STRATEGY.md](../../../../STRATEGY.md)):** Strategy canvas is the **developer explainability** surface for preset orchestration — not a universal compose surface for end-user nest/combine of patterns.
- **Idea-steer** exists as a steering affordance; it does **not** establish a Generate → Validate → Simulate → Approve → Apply pipeline over a candidate IR.

### 2. Target architecture

```text
                 ┌─────────────────────────────────────────┐
                 │  TS surfaces (locked user-facing stack) │
                 │  Strategy Canvas = Compose + HITL +     │
                 │  explainability; CLI / Agent also entry │
                 └───────────────────┬─────────────────────┘
                                     │ codegen DTOs (from schemas/daemon-api)
                                     ▼
   Generate / Compose ──► Workflow IR candidate
                          (JSON Schema SSOT → generated Rust/TS types)
                                     │
                    Validate ◄───────┤ nest/compose policy
                    Simulate ◄───────┤ stubbed effect fence
                    Approve  ◄───────┤ explicit HITL
                                     ▼
                          Apply (freeze identity)
                                     │
              ┌──────────────────────┼──────────────────────┐
              │ transitional         │ target               │
              ▼                      ▼                      │
     preset.yaml projection    IR → graph construction     │
     (compat / migrate)        (same Task semantics)       │
              │                      │                      │
              └──────────┬───────────┘                      │
                         ▼                                  │
              graph-flow execution (ONE backend)            │
              + WaitingForInput run gates                   │
                         ▲                                  │
              patterns expand at Apply (or defined          │
              pre-Apply expansion); pins recorded ──────────┘
```

#### 2.1 Normative target rules

1. **Workflow IR MUST be the authoring and review SSOT** for new strategy work (generate, compose, validate, simulate, approve).
2. **`preset.yaml` + templates MUST demote** to a **compat import/export + migration-period projection**. They are not the long-term human authoring SSOT, and they are **not** deleted: partner/integrator bundles remain importable and exportable after the hand-authored YAML *main path* is retired.
3. **Execution semantics MUST remain single-backend:** the retained graph-flow adapter (`OrchestrationEngine` / `GraphFlowEngine`). This redesign MUST NOT introduce a second orchestration runtime. Simulator MUST reuse the same graph construction rules with dry-run task adapters.
4. **JSON Schema is the IR truth source.** The candidate/approved document schema MUST land in `schemas/daemon-api/` and go through `pnpm run codegen` plus `schema_drift_detection` / `check-wire-drift.sh`. Generated Rust and TS types are projections of that schema — a second hand-written DTO family is an anti-pattern. TypeScript owns Canvas UX only. Rust owns validation, simulation, expansion, Apply identity freeze, and execution truth ([STRATEGY.md](../../../../STRATEGY.md) / #482 posture).
5. **Patterns are first-class packages.** Developers primarily publish patterns; end users nest/compose patterns and drafts on Canvas (and equivalent APIs).
6. **Work artifacts are out of this IR.** Chapter bodies, outlines, review/quality logs, and other files under `Works/<work_ref>/` stay under today's profile Masters until/unless [PR #485](https://github.com/42ch-dev/nexus/pull/485) (sibling Exploration, **not yet merged, not current authority**) is promoted. This document MUST NOT absorb that SSOT. Simulator dry-run MUST stub any Work-artifact write it would otherwise perform.

#### 2.2 What "completely replace" means

After migration acceptance (§7.4), the product MUST be able to:

- Author and ship strategies **without** hand-editing `preset.yaml` as the primary path.
- Drive Canvas reads/writes from **IR DTOs**, not from re-parsing raw YAML as the sole projection source.
- Load production runs from **IR (or expanded IR)** into graph-flow without requiring the legacy YAML→Graph mapper as the only intake.
- Keep legacy and partner bundles runnable through an explicit **import/compat export** path that is **retained** after the hand-authored YAML main path is retired.

---

### 3. Workflow IR detailed model

#### 3.1 Document kinds and lifecycle states

| Kind | Meaning | Mutable? |
| --- | --- | --- |
| `WorkflowCandidate` | Draft IR under Generate/Compose; not executable for production admission | Yes (compose patches, local regen merges) |
| `WorkflowApproved` | Explicitly approved snapshot; content-hash frozen for Apply | No (new candidate required to change) |
| `WorkflowApplied` | Installed identity recorded in engine source-freeze story; eligible for schedule admission | No (reload = new Apply) |

A conforming implementation MUST persist candidate vs approved distinctly. Silent promotion Candidate → Applied MUST NOT occur.

#### 3.2 Core IR shape (conceptual schema)

Names are exploratory until Draft promotion. The locking SSOT is JSON Schema under `schemas/daemon-api/` (codegen + wire-drift checks); Rust/TS structs are generated from it, not hand-maintained in parallel.

```text
WorkflowDocument {
  ir_version: u32
  id: string                 # strategy id; same id rules as preset.id today
  revision: u32              # CAS for Canvas / API patches on candidates
  state: candidate | approved | applied_ref
  meta: { description, kind: creator|system, author?, license?, homepage?,
          run_intents[], requires_capabilities[], cli_args? }
  entry: NodeId              # replaces preset.initial
  initial_action?: Action    # seeds core_context (today preset.initial_action)
  terminal: NodeId[]         # one or more terminals; v1 MAY keep single terminal for preset parity
  nodes: Node[]
  edges: Edge[]
  nests: NestBinding[]       # first-class composition (patterns / child workflows)
  assets: AssetRef[]         # templates, prompts — content-addressed or bundle-relative
  signals: SignalBinding[]   # executable; pause / force-transition (today preset.signals)
  roles: RoleDefinition[]    # executable; system_prompt_file / recommended_skills / node agent bind
  effect_summary: EffectClass  # rolled-up fence for Simulate/Approve
  provenance: Provenance     # generate vs compose lineage, pattern pins
}

Node {
  id: NodeId
  kind: enum
    | OuterState { enter: EnterOp[], exit_when: ExitWhen, converge?: Converge }
    | InnerStep { step_kind: acp_prompt|capability|…, template?: AssetRef, tool_policy?, depends_hint? }
    | PatternSlot { pattern_ref: PatternPin, bind: SlotBindings }  # unexpanded
    | NestGroup { child_workflow_id, collapsed_view }
    | Join | Terminal | ManualWait | Judge
  ui?: { label, description }   # Canvas chrome; MUST NOT affect execution semantics
  effect_class: pure|read|write_candidate|external
}

Edge {
  id: EdgeId
  from: NodeId
  to: NodeId
  kind: linear | go | nogo | conditional { when: Expr } | default | depends_on
}

NestBinding {
  id: NestId
  parent_node: NodeId          # PatternSlot or NestGroup
  child: PatternPin | WorkflowRef
  expansion: defer_until_apply | expand_in_candidate
  pin_policy: exact | range_locked_at_approve
}

PatternPin { registry_id, version: semver|hash, content_hash?: blake3 }
Provenance { source: generate|compose|import_preset, agent_run_id?, parent_candidate_id? }
```

#### 3.3 Nesting and composition semantics

1. End users MAY nest patterns and candidate fragments subject to **max depth** (Open question default proposal: **depth ≤ 3** unexpanded nests; cycles MUST be rejected).
2. Nested children MUST expose `effect_class`; parent `effect_summary` MUST be the **lattice join** (worst case) of children — nesting MUST NOT hide `write_candidate` or `external`.
3. At Apply (or a defined pre-Apply expansion step), PatternSlots MUST expand into concrete nodes/edges in the **same** outer/inner model graph-flow already understands (flatten nests into outer states + inner graphs / child sessions). Expansion MUST be deterministic for a given pin set.
4. Local Agent regeneration of a nested region during Compose MUST merge into the **single candidate IR** (replace subtree by nest id); implementations MUST NOT fork unreconciled shadow candidates.

#### 3.4 Mapping to today's `PresetManifest` (compat)

| PresetManifest | Workflow IR |
| --- | --- |
| `preset.*` header | `meta` + `id` + `ir_version` / document revision |
| `states[]` | `OuterState` nodes + linear/conditional edges |
| `exit_when: manual` | `ManualWait` / exit_when manual → run-time `WaitingForInput` |
| `exit_when: llm_judge` | `Judge` node + go/nogo edges |
| `enter: inner_graph` | NestGroup or expanded InnerStep subgraph |
| `inner_graphs.*.nodes` | InnerStep nodes + `depends_on` edges |
| `requires_capabilities` / `run_intents` / `gates` | `meta` + gate attachments (gates remain enqueue-time; unchanged ownership in schedule Master) |
| `signals[]` | `signals[]` (on_receive pause / force-transition and equivalents MUST be preserved) |
| `roles[]` + `GraphNode.agent` | `roles[]` and node role bindings (`system_prompt_file`, `recommended_skills`) |
| `preset.initial_action` | `initial_action` (core_context seeding) |
| `preset.cli_args` | `meta.cli_args` |
| `preset.homepage` | `meta.homepage` (UI/docs; MAY be dropped only if documented as non-executable) |
| `inner_graphs.*.output_binding` | Nest/subgraph export binding onto a named state output (MUST be preserved; used by `react-trpg-turn`) |
| `exit_when: rule` / `exit_when: graph_complete` | `exit_when` variants on OuterState (table MUST cover all four kinds: manual, rule, llm_judge, graph_complete) |
| per-state `context_update`, `merge` / `converge`, `timeout_ms` / `on_timeout` | fields on `OuterState` (or a documented equivalent). MUST NOT be silently dropped. |
| templates via `template_file` | `assets[]` with path or content hash |
| — (IR-only; no preset source) | `nests[]`, PatternSlot, candidate/approved states, effect_summary, provenance |

**Lossless import:** every valid today-preset MUST import to IR without semantic loss for **executable** fields. The rows above are the required carrier list (including fields the first draft omitted). UI-only fields MAY be synthesized. If a later draft cannot carry a named executable field, the MUST is demoted for **that field only** and the loss MUST be listed here and in P0 exit criteria — a silent drop of `signals`, `roles`, `initial_action`, `cli_args`, `output_binding`, or an `ExitWhen` kind is not lossless.

**Projection export (transitional):** IR without PatternSlots (fully expanded) MUST export to `preset.yaml` + templates that pass today's validator. IR that still contains unexpanded PatternSlots MUST either expand before export or refuse export with structured diagnostics.

#### 3.5 Dual-read and cutover

| Phase | Intake | Notes |
| --- | --- | --- |
| M1 Dual-read | Loader accepts **IR document** *or* preset bundle; both validate through a shared semantic core | Canvas MAY still read YAML via projection |
| M2 IR-primary write | New edits / Generate / Compose write IR; YAML export is derived | Patches target IR revision CAS |
| M3 IR-primary execute | Production admission prefers applied IR identity; YAML-only bundles auto-import on load | Telemetry on remaining YAML-only |
| M4 Retire hand-authored YAML main path | Hand-authored YAML path deprecated; **import/export retained**; legacy mapper feature-flagged off in default builds | Acceptance §7.4 |

---

### 4. Pattern packages

#### 4.1 Package layout (proposal)

```text
my-pattern/
  pattern.toml          # or pattern.yaml — manifest
  workflow.ir.json      # (or .cbor) IR fragment: nodes/edges/assets relative
  templates/            # optional prompt assets
  README.md             # human docs (non-normative)
```

**Manifest fields (minimum):**

| Field | Rule |
| --- | --- |
| `id` | Registry id (`[a-z][a-z0-9._-]*`) |
| `version` | Semver |
| `ir_compat` | Min/max `ir_version` |
| `description` | Human label |
| `effect_class` | Declared; MUST match validator rollup of fragment |
| `slots` | Named bind parameters (strings, asset refs, capability names) |
| `requires_capabilities` | Union into parent on instantiate |

#### 4.2 Publish, instantiate, pin vs hot-update

1. **Publish** SHOULD be an independent versioned artifact (crate/registry later; v1 MAY be filesystem + content hash under `~/.nexus42/patterns/` analogous to presets).
2. **Instantiate** on Compose creates a `PatternSlot` with **exact pin** default (`id` + `version` + optional `content_hash` at compose time).
3. **At Approve**, pins MUST be locked. Approve MUST record `content_hash` for **every** resolved pattern and its assets (exact pins included — omitting the hash is not allowed on an approved snapshot). Range pins, if allowed later, MUST resolve to those exact recorded hashes. **Apply MUST verify** the recorded hashes; a filesystem/registry object that changed under the same version MUST fail closed.
4. **Hot-update after Apply** MUST NOT silently change frozen source identity (engine §15 item 6). A new Apply (new approved snapshot) is required — same discipline as preset content-hash freeze.
5. Developers author and publish patterns as a first-class channel. End users consume published patterns. Content creators MAY publish only through a **policy-bound / developer-approved channel**, and only after the same validate/simulate gates. §5.5 follows this rule (no second, weaker publish path).

---

### 5. Canvas detailed design (composition surface)

#### 5.1 Information architecture

Strategy Canvas remains a `CanvasSurfaceKind = 'strategy'` peer under the shared Canvas Shell ([canvas-strategy-surface.md](../../../../docs/specs/surfaces/canvas-strategy-surface.md) §3.3.1), but its **adapter contract changes**:

| Concern | Today | Target |
| --- | --- | --- |
| Graph source | Parsed YAML → `buildStrategyGraph` | IR DTO → `projectIrGraph` (single projection owned by adapter; Rust MAY supply canonical layout hints) |
| Primary ops | patch state / transition / prompt | **Compose ops**: nest pattern, unnest, replace subtree, bind slot, generate-into-region, approve, simulate |
| Secondary ops | — | Advanced edge/node edit (maps to IR patches; still CAS) |
| Overlay | Session status | Session status **plus** simulation trajectory overlay (candidate mode) |
| Alt view | Node/edge lists | Candidate outline: nests tree, pins, effect summary, diagnostics |

#### 5.2 Dual entry: Generate | Compose

```text
┌──────────────── Strategy Hub ────────────────┐
│  [Generate from intent]  [Compose blank/from │
│   Agent draft]            pattern library]   │
│  [Import legacy preset]  [Open applied]      │
└──────────────────────────────────────────────┘
```

- **Generate:** user states intent → Host/ACP generator returns `WorkflowCandidate` → Canvas opens in review/compose mode (not production).
- **Compose:** user starts from blank, pattern library, or imported preset-as-IR → nest/combine as first-class ops.
- Both entries MUST land on the **same** candidate document and lifecycle.

Users MUST NOT be required to drag every edge from scratch. Users MUST NOT be limited to read-only inspection of Agent output.

#### 5.3 Nest visualization and composition ops

**First-class compose ops (creator-tier):**

- Add pattern from library → PatternSlot node (collapsed by default).
- Nest selected subgraph into a NestGroup / extract selection to new pattern draft (dev-tier).
- Bind slot parameters in inspector.
- Expand/collapse nested region; drill-in subflow (reuse React Flow `parentId` / group — already proven for inner graphs).
- Replace nested region via scoped Generate (merge by nest id).

**Advanced edit ops (developer-tier / power toggle):**

- Rewire edges, edit exit_when, edit templates, converge strategy — equivalent to today's patches but against IR.

#### 5.4 Simulate overlay and Approve / Apply UX

1. **Simulate** runs server-side; Canvas paints trajectory (visited, predicted waits, effect badges) without enabling production run controls.
2. **Approve** is an explicit modal: show effect_summary, **recorded** pin/asset hashes, validation report, and simulation summary for **this** document hash; confirm creates an immutable approved snapshot. Constrained patches after Validate/Simulate MUST re-run Validate and Simulate and bind approval to the new content hash — results from an earlier graph MUST NOT approve a later one.
3. **Apply** installs frozen identity (IR applied record + optional YAML projection during dual-read) and **checks** the hashes recorded at Approve; only then may schedule admission bind it.
4. Run-time **WaitingForInput** remains a separate gate on execution — Approve does not resume runs; wait continuation does not Apply graphs.

#### 5.5 Permission layering

| Role | Canvas powers |
| --- | --- |
| Content creator | Generate, Compose with published patterns, nest/unnest, bind slots, simulate, approve (policy-bound), apply to own schedules; MAY publish patterns only via the policy-bound / developer-approved channel in §4.2 |
| Developer | All creator powers + advanced edge edit, first-class author/publish of patterns, import legacy presets, view raw IR |
| System | Embedded/system workflows remain read-only except through controlled release channels |

#### 5.6 Retiring "explainability-only" Strategy Canvas

| Step | Action |
| --- | --- |
| C1 | Ship IR read projection alongside YAML projection; feature-flag compose ops |
| C2 | Default new Strategy routes to candidate compose UX; keep YAML patch API for installed legacy bundles |
| C3 | Move developer explainability to "Applied IR inspector" (read-only overlay on frozen IR) rather than YAML-first editing |
| C4 | Remove YAML-only `buildStrategyGraph` path once dual-read ends; delete or quarantine preset-yaml parser from the hot path |

[STRATEGY.md](../../../../STRATEGY.md) wording SHOULD later update Strategy canvas from "developer explainability" to "composition + HITL + explainability" when this Exploration promotes — that edit is **out of band** for this Proposed file's implement authority.

---

### 6. Lifecycle pipeline and API boundaries

```text
Generate | Compose → Validate → Simulate → Approve → Apply → Execute
```

| Stage | TS responsibility | Rust responsibility | API sketch (exploratory; **keep `/strategies`**) |
| --- | --- | --- | --- |
| Generate | Intent form; show progress | Generator capability / tool; emit candidate IR | `POST /v1/daemon/strategies/generate` |
| Compose | Nest/combine UX; scoped regen | IR patch apply + merge; pattern resolve | `POST /v1/daemon/strategies/{id}/compose` |
| Validate | Render diagnostics | Shared semantic + nest policy facade | `POST /v1/daemon/strategies/{id}/validate` (collection validate MAY be `POST /v1/daemon/strategies/validate`) |
| Simulate | Overlay trajectory | Dry-run engine; stub ports; effect rollup | `POST /v1/daemon/strategies/{id}/simulate` |
| Approve | Explicit confirm UI | Snapshot + recorded hashes; pin lock | `POST /v1/daemon/strategies/{id}/approve` |
| Apply | Success / conflict UX | Persist applied identity; optional YAML export; schedule bind prep | `POST /v1/daemon/strategies/{id}/apply` |
| Execute | Live overlay / waits | Existing engine + wait_id CAS | Existing schedules/sessions routes |

Route shape follows [daemon-api-surface-conventions.md](../../../../docs/specs/runtime/daemon-api-surface-conventions.md) (path suffixes, not colon-action routes such as `/workflows:generate`). **PD-4** locks the internal identifier `/strategies` and `preset` wire field names; renaming the resource to `workflows` is a **separate product decision** and is out of this PR. This sketch keeps `/strategies`.

**Rules:**

- Generate/Compose MUST NOT open driven production sessions as a side effect.
- Validate/Simulate MUST be side-effect-free w.r.t. World KB settlement and cloud sync.
- Approve/Apply MUST be authorized, audited, and CAS-guarded on `revision` / content hash.
- Execute MUST continue to refuse missing/changed source bytes (engine §15 item 6).

---

### 7. Migration plan — fully refactor current implementation

**Prerequisite sequencing:** follow §0.1 (merge #482 → merge this document → **Draft/Master promotion** → then P0…). The table below is the *implementation* ladder after those proposals are accepted **and promoted**; merging the Explorations alone is not a license to start P0.

#### 7.1 Phase plan

| Phase | Replace / add | Exit criteria |
| --- | --- | --- |
| **P0 Inventory + IR crate / schema stub** | JSON Schema in `schemas/daemon-api/` + generated types; import from `PresetManifest` covering §3.4 carriers; start the IR→Task mapping table | Round-trip samples `game-narrative`, `react-trpg-turn` with executable fields preserved (`signals`, `roles`, `output_binding`, …) |
| **P1 Validate-on-IR** | Shared facade accepts IR; YAML path calls import→validate. **CLI:** hang the new entry off existing `nexus42 preset validate` (alias or subcommand) — do **not** create a second implementation leaf named only `nexus42 workflow validate`. | `preset validate` (and any alias) parity on samples; IR→Task mapping table drafted |
| **P2 Simulator MVP** | Optional package/feature; stub write/external | Trajectory JSON for one sample; no Apply |
| **P3 Canvas compose MVP** | IR projection + nest pattern + approve UI (flagged) | Creator can compose two patterns → candidate → simulate → approve in staging |
| **P4 Dual-read loader** | Engine loads applied IR **or** preset bundle | Existing schedules on YAML presets unbroken |
| **P5 IR-primary authoring** | Docs + samples authored as IR; YAML export for compat | New strategies land as IR in repo |
| **P6 Retire hand-authored YAML main path** | Deprecate hand-edit + YAML patch as the default authoring/intake path; **keep import/export** of `preset.yaml` bundles (partner/integrator path) | Acceptance §7.4 green; feature flag default off for the hand-authored YAML *mapper* in new installs; import tool remains |

#### 7.2 Components to retire or replace

| Component | Fate |
| --- | --- |
| Hand-authored `preset.yaml` as **primary** authoring | Replace with IR authoring + pattern compose |
| TS `parsePresetYaml` + `buildStrategyGraph` as **sole** Canvas source | Replace with IR DTO projection; keep import adapter temporarily |
| `strategy.patch_*` YAML mutators as **sole** write path | Replace with IR compose/patch ops; legacy patches only for unmigrated bundles |
| Loader YAML→Graph as **sole** intake | Replace with IR→Graph (same Task mapping semantics as [orchestration-engine.md](../../../../docs/specs/orchestration/orchestration-engine.md) §8.2, documented in the P0/P1 IR→Task table); YAML via import |
| Copy-sample-directory reuse | Replace with pattern packages + pins |
| Strategy Canvas "explainability only" product framing | Replace with composition + HITL + applied-IR inspector |
| `crates/nexus-preset` (manifest, loader, validation, `PresetSourceIdentity`) | **Do not** leave a second grammar beside a new `workflow-ir` crate. `nexus-preset` becomes the **import/compat loader** (keep `PresetSourceIdentity` content-hash semantics). The shared semantic-validation facade expands to accept IR. After **P4**, YAML and IR MUST NOT be two independently evolving grammars — YAML is a projection/import of IR. |

**Must keep:** graph-flow backend; `OrchestrationEngine` trait; SessionStorage; WaitingForInput/`wait_id`; capability registry; source identity freeze concept (extended to IR + pins); integrator vs creator-host execution surface split; `preset.yaml` **import/export** as a documented compat format.

#### 7.3 Rollback and coexistence

- Dual-read window MUST allow emergency re-enable of YAML intake via config flag.
- Applied IR stores provenance linking to imported preset id when migrated.
- Coexistence: YAML-only bundles continue until P6; mixed workspaces allowed.
- Rollback of a bad Apply = install previous approved snapshot (new Apply), never silent hash rewrite.

#### 7.4 Acceptance — "old strategy orchestration path can be turned off"

All MUST hold on a reference workspace:

1. Samples and embedded creator strategies run from **applied IR** with YAML intake flag **off**.
2. Strategy Canvas primary route never calls `parsePresetYaml` except inside the legacy import wizard.
3. No CI job requires hand-maintained `preset.yaml` for new strategies (export MAY exist as a generated artifact).
4. Compose + Generate paths produce candidates that pass validate/simulate/approve/apply.
5. Engine recovery still refuses source mismatch for IR content hash + **recorded** pattern/asset pins.
6. Documented kill-switch re-enables YAML *intake* for one minor release after default-off (then removed). Import/export of bundles remains after that window.
7. **Partner / integrator bundles** documented in [docs/strategy-authoring.md](../../../../docs/strategy-authoring.md) remain usable: a deprecation note + import tool MUST convert an existing external bundle so it passes validate and simulate. Cutting the public bundle format without this path is a breaking change this plan MUST NOT silently ship.

---

### 8. Safety invariants

1. **No silent Apply.** Candidate → Approved → Applied transitions MUST be explicit. Auto-apply on generate/compose/validate/simulate MUST NOT exist.
2. **Simulation stubs.** During Simulate, `write_candidate` and `external` capabilities MUST be stubbed or recorded only — never World-settling commits, never cloud sync, and never a live write through a Work Artifact port (`Works/` tree; [PR #485](https://github.com/42ch-dev/nexus/pull/485)).
3. **Nested effect rollup.** Child effect classes MUST bubble to parent `effect_summary` (lattice join). Approve UI MUST show the rolled-up fence.
4. **Source freeze.** After Apply, recovery MUST refuse missing/changed IR bytes or pattern pin hashes (same spirit as orchestration-engine §15 item 6).
5. **Complementary human gates.** Approve gates *graph adoption*; `WaitingForInput` gates *run continuation*. Neither replaces the other.
6. **Single execution semantics.** Simulator and production MUST share graph construction rules; dry-run differs only in capability ports and persistence isolation.
7. **TS/Rust boundary.** User-facing Canvas stays TypeScript; Rust owns IR truth.

---

### 10. Non-goals

- A second orchestration runtime or in-memory-only LangChain-style pipeline.
- A first-party "player" app ([STRATEGY.md](../../../../STRATEGY.md)).
- Guaranteeing LLM generation quality parity across providers.
- Exactly-once external side effects in simulation or historical repair beyond engine §15 non-goals.
- Replacing Outline / Timeline / World KB canvas surfaces (Strategy surface only).
- Owning Work-tree artifacts (`Works/<work_ref>/Stories`, `Outlines`, `Logs`, and sibling profiles). Today's authority remains the Shipped profile Masters. [PR #485](https://github.com/42ch-dev/nexus/pull/485) is a **sibling Exploration** (not yet merged, not current authority) for a future Work Artifact IR; this Workflow IR MUST NOT absorb that work.
- Actor/Viewpoint preset IR (explicitly out of scope in engine §15 item 7).
- Implementing this Exploration without a later Draft/Master promotion.

---

### 11. References

- [`docs/specs/README.md`](../../../../docs/specs/README.md) — Exploration / proposed lifecycle rules
- [orchestration-engine.md](../../../../docs/specs/orchestration/orchestration-engine.md) — graph-flow integration, preset loader §8, WaitingForInput + source freeze §15
- [canvas-strategy-surface.md](../../../../docs/specs/surfaces/canvas-strategy-surface.md) — Strategy Canvas β, React Flow mapping §3.2, adapter recipe §3.3.1
- [docs/strategy-authoring.md](../../../../docs/strategy-authoring.md) — bundle layout, validator, patch CAS
- [STRATEGY.md](../../../../STRATEGY.md) — Harness / Canvas pillars
- `dynamic-workflow-and-simulation.md` ([PR #482](https://github.com/42ch-dev/nexus/pull/482)) — direction brief; this doc is the detailed redesign companion
- Work Artifact IR — [PR #485](https://github.com/42ch-dev/nexus/pull/485) (sibling Exploration, **not yet merged, not current authority**; Work-tree SSOT today is the Shipped profile Masters)
- [daemon-api-surface-conventions.md](../../../../docs/specs/runtime/daemon-api-surface-conventions.md) — route shape; this sketch stays on `/strategies` (PD-4)
- `crates/nexus-contracts/src/local/orchestration/preset.rs` — `PresetManifest` types today
- `crates/nexus-preset/src/{loader,source_identity,validation}.rs` — load + content hash; target: import/compat loader
- `apps/web/src/lib/canvas/{strategy-graph.ts,use-strategy-data.ts}` + `components/canvas/strategy-canvas/*` — YAML projection + patches
- `strategy-samples/{game-narrative,react-trpg-turn}` — reference bundles for import/round-trip
- graph-flow (rs-graph-llm) — execution backend pinned per crate (`crates/nexus-core`, `crates/nexus-orchestration`, `=0.8.1`)

---

*End of Exploration. Promote only via Draft/Master lifecycle + link sweep per specs AGENTS.md.*

## Alternatives considered

Not stated as a separate comparison in the migrated proposal; where the body names a rejected direction, that record stays in the proposal.

## Consequences

This section is **maintainer guidance** for how to land the two Explorations and then the §7 migration phases. It does not schedule work by itself.

##### Proposal merge order

| Order | Artifact | Why this order |
| --- | --- | --- |
| **1** | [PR #482](https://github.com/42ch-dev/nexus/pull/482) direction (`dynamic-workflow-and-simulation.md` + packaging companion) | Thin consensus: universal IR, end-user nest/compose, developer patterns, Generate\|Compose lifecycle, TS/Rust boundary, safety fences, modular packaging. Locks product constraints before schema hardens. |
| **2** | **This PR (#483)** detailed redesign | Depends on #482's product locks. Filling IR schema, Canvas ops, dual-read cutover, and acceptance *before* direction is merged risks freezing the wrong ceiling (e.g. YAML-forever projection, explainability-only Canvas). |
| **3** | Resolve §9 open questions that #482/#483 share | Nest depth, IR encoding, generator home, simulate fidelity, Approve policy — decide against #482 constraints, record answers here or in a follow-up Proposed/Draft. |
| **4** | Draft/Master promotion (later, separate PR) | Only then does this design outrank active Masters; link-migration sweep per AGENTS.md. |

For iteration `v1.236-adr-foundation`, these seven questions are answered in `.mstar/iterations/v1.236-adr-foundation/specs/workflow-ir-decisions.md`, the round-1 decision record; YAML/Canvas/execution switches remain governed by the redesign ADR's phase plan.

**Anti-pattern:** merging #483 first “because it is more detailed,” or implementing P0 while #482 is still contested. Review of #483 MAY happen in parallel with #482 review; **merge and treat-as-constraint** SHOULD be #482 → #483.

**Optional review tactic:** if maintainers want the replacement story narrated first, review #483 for understanding, then merge #482 as the short direction (merge still does **not** confer implement authority), then merge #483 — repository merge order still #482 then #483.

##### Implementation land order (after both Explorations are merged **and** this design is promoted)

Do **not** run packaging track and IR-replace track as two unrelated rewrites. Merging #482/#483 into `proposed/` is **not** implement authority. Prefer:

```text
#482 merged (direction)
        │
        ▼
#483 merged (detailed redesign)
        │
        ▼
Draft/Master promotion (separate PR; required before implement authority)
        │
        ▼
Decide remaining §9 / §3.4 blockers for P0/P1 (encoding; import field coverage)
        │
        ▼
§7 P0  workflow-ir stub + lossless import of samples
        │
        ▼
§7 P1  Validate-on-IR (shared facade; produces IR→Task mapping table)
        │
        ├──────────────────────────────┐
        ▼                              ▼
§7 P2 Simulator MVP              Packaging A-track
(dry-run, no Apply)              (thin core / features;
                                 may start after P0 if
                                 rebuild pain dominates)
        │
        ▼
§7 P3 Canvas compose MVP (flagged UI MAY start after P1;
 exit / default-on require P2 simulate)
        │
        ▼
§7 P4 Dual-read loader (YAML presets unbroken; import/export retained)
        │
        ▼
§7 P5 IR-primary authoring (new strategies as IR)
        │
        ▼
§7 P6 Retire hand-authored YAML main path (import/export kept; acceptance §7.4)
```

| Gate | MUST hold before next step |
| --- | --- |
| Before **P0** | #482 + #483 merged; this design **promoted to Draft/Master** (a named written waiver of implement-authority is the only alternative); sample list for round-trip agreed; §9 encoding and §3.4 import coverage decided for the stub |
| Before **P3** default-on / P3 exit | P1 validate parity on samples; **P2** trajectory available (P3's compose→simulate→approve exit needs the simulator); compose ops behind flag until then. Flagged compose UI MAY start after P1. |
| Before **P4** | IR→Graph Task mapping table (**P0/P1 deliverable**) reviewed against [orchestration-engine.md](../../../../docs/specs/orchestration/orchestration-engine.md) §8.2 YAML→Task semantics |
| Before **P6** | §7.4 acceptance checklist green on a reference workspace; kill-switch documented; partner-bundle import path documented |

**Parallelism that is OK:** Spoke-style packaging / crate fan-out (lightweight-core track from #482) MAY proceed in parallel with P0–P2 **as long as** it does not invent a second authoring SSOT or a second runtime. **Not OK:** shipping Generate→Apply to production before Simulate fence + explicit Approve exist.

---

## Risks

1. Exact on-disk IR encoding (JSON vs CBOR vs YAML-IR) for human diff friendliness vs hash stability.
2. Exact enqueue-time vs document placement for gates (carriers exist in §3.2/`meta`; schedule Master ownership is unchanged). `cli_args` has an IR carrier in `meta.cli_args`.
3. Hard max nest depth and whether `expand_in_candidate` vs `defer_until_apply` is the default for published patterns.
4. Generator operational home (builtin capability vs Develop-hub tool vs CLI-first) — same open item as #482.
5. Minimum simulation fidelity for Approve honesty (reachability-only vs stubbed judge branch exploration).
6. Creator vs developer Approve policy when composing third-party patterns (trust labels / signatures deferred with preset registry).
7. Whether exported YAML during dual-read is git-tracked generated output or local-only install artifact.
