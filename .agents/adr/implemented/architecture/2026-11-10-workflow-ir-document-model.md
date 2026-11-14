# ADR: Workflow IR Document Model — Shared Strategy Orchestration Contract

Status: implemented — P0 IR schema/import and P1 validate-on-IR shipped; P4 dual-read and P5 IR-primary authoring remain undecided here
Date: 2026-11-10
Scope: Workflow IR as the shared typed document for strategy orchestration: document shape and carriers, candidate/approved/applied identity, canonical content hashing, lossless import from `preset.yaml`, and validation parity through the shared semantic facade. Canvas composition, pattern packages, dual-read loading, IR-primary authoring, and retirement of the hand-authored YAML main path belong to [Canvas Composition and Pattern Packages](../../proposed/architecture/2026-11-10-canvas-composition-and-pattern-packages.md).
Source: split from the archived Canvas + Workflow IR redesign and Dynamic Workflow Generation and Orchestration Simulation records

## Problem

Generators, validators, simulators, Canvas, and pattern tooling lacked one typed strategy document. YAML was the authoring SSOT, while TypeScript `buildStrategyGraph` and the Rust loader independently reconstructed meaning from it. Repeated YAML round-trips could not carry candidate state, composition provenance, effect summaries, and other IR-only information without loss.

The existing preset format nevertheless contains executable behavior that a replacement document must preserve. A schema that omitted signals, roles, initial actions, CLI arguments, output bindings, exit conditions, context updates, convergence, or timeouts would be a semantic regression rather than a new model.

## Decision

### 1. Shared typed document

Workflow IR is the common typed representation used by generators, validators, simulators, Canvas projections, pattern tooling, and execution-intake adapters. It is not a developer-only artifact. JSON Schema under `schemas/daemon-api/` is the schema truth source; generated Rust and TypeScript types are projections produced through `pnpm run codegen` and checked by `schema_drift_detection` / `check-wire-drift.sh`. A second hand-written DTO family is not allowed.

Rust owns validation, simulation, expansion, Apply identity freezing, and execution truth. TypeScript consumes generated DTOs for Canvas and review surfaces. Workflow IR does not absorb Work-tree artifacts, Memory, KnowledgeStore, World KB bodies, or SessionStorage state; Work output remains the separate [Work Artifact IR](../feature/2026-11-10-work-artifact-ir.md).

The retained graph-flow adapter remains the sole execution backend. P0/P1 define the shared document and its validation, not a second orchestration runtime and not a replacement execution intake.

### 2. Document kinds and identity

| Kind | Meaning | Mutability |
| --- | --- | --- |
| `WorkflowCandidate` | Draft IR under Generate/Compose; not admitted to production execution | Mutable through candidate patches and scoped regeneration |
| `WorkflowApproved` | Explicitly approved snapshot frozen for Apply | Immutable; changes require a new candidate |
| `WorkflowApplied` | Installed identity in the engine source-freeze record; eligible for schedule admission | Immutable; reload requires a new Apply |

Candidate, approved, and applied records are distinct. Candidate → Applied promotion is never silent. Apply persists the approved IR identity and content hash consistently with orchestration-engine source-freeze rules. Recovery refuses missing or changed source bytes; rollback installs a previous approved snapshot through a new Apply rather than rewriting a hash.

### 3. Core document shape and carriers

```text
WorkflowDocument {
  ir_version: u32
  id: string
  revision: u32
  state: candidate | approved | applied_ref
  meta: { description, kind: creator|system, author?, license?, homepage?,
          run_intents[], requires_capabilities[], cli_args? }
  entry: NodeId
  initial_action?: Action
  terminal: NodeId[]
  nodes: Node[]
  edges: Edge[]
  nests: NestBinding[]
  assets: AssetRef[]
  signals: SignalBinding[]
  roles: RoleDefinition[]
  effect_summary: EffectClass
  provenance: Provenance
}

Node {
  id: NodeId
  kind: OuterState | InnerStep | PatternSlot | NestGroup | Join | Terminal | ManualWait | Judge
  ui?: { label, description }
  effect_class: pure | read | write_candidate | external
}

Edge {
  id: EdgeId
  from: NodeId
  to: NodeId
  kind: linear | go | nogo | conditional | default | depends_on
}

NestBinding {
  id: NestId
  parent_node: NodeId
  child: PatternPin | WorkflowRef
  expansion: defer_until_apply | expand_in_candidate
  pin_policy: exact | range_locked_at_approve
}

PatternPin { registry_id, version, content_hash? }
Provenance { source: generate|compose|import_preset, agent_run_id?, parent_candidate_id? }
```

`OuterState` carries enter/exit behavior, optional convergence, context updates, merge behavior, timeout and timeout handling. `InnerStep` carries its step kind, asset reference, tool policy, and dependency hints. `PatternSlot` carries a pattern pin and slot bindings; `NestGroup` identifies a child workflow and its collapsed view. UI labels and descriptions are Canvas chrome and never affect execution semantics.

Signals preserve pause and force-transition behavior. Roles preserve system prompt files, recommended skills, and node agent bindings. `initial_action` seeds `core_context`; `meta.cli_args`, `meta.homepage`, run intents, capabilities, and gate attachments retain their existing ownership. Enqueue-time gates remain owned by the schedule contract even where the document has a carrier.

Nests are part of the type model so import, validation, and simulation can account for them. Their Canvas composition operations and package publication are not shipped by this decision. Cycles are invalid; nesting exposes child effect classes to the parent rollup and never hides `write_candidate` or `external`. Expansion, when used, is deterministic for a pin set and maps into the same outer/inner graph model understood by graph-flow. Scoped regeneration merges by nest id into one candidate instead of creating an unreconciled shadow graph.

### 4. Relation to `preset.yaml`

`preset.yaml` + templates is an import/export compatibility representation. P0 ships lossless import into IR; this record does not decide that production loads both formats (P4), that new authoring is IR-primary (P5), or that the hand-authored YAML main path is retired (P6).

| PresetManifest | Workflow IR |
| --- | --- |
| `preset.*` header | `meta` + `id` + `ir_version` / document revision |
| `states[]` | `OuterState` nodes + linear/conditional edges |
| `exit_when: manual` | `ManualWait` / manual exit → run-time `WaitingForInput` |
| `exit_when: llm_judge` | `Judge` node + go/nogo edges |
| `exit_when: rule` / `graph_complete` | Corresponding `OuterState.exit_when` variants |
| `enter: inner_graph` | NestGroup or expanded InnerStep subgraph |
| `inner_graphs.*.nodes` | InnerStep nodes + `depends_on` edges |
| `inner_graphs.*.output_binding` | Named state output binding |
| `requires_capabilities`, `run_intents`, `gates` | Metadata and gate attachments; enqueue ownership unchanged |
| `signals[]` | Executable pause / force-transition bindings |
| `roles[]` + `GraphNode.agent` | Roles and node role bindings |
| `preset.initial_action` | `initial_action` |
| `preset.cli_args` | `meta.cli_args` |
| `preset.homepage` | `meta.homepage` |
| Per-state context, merge/converge, timeout fields | `OuterState` fields or documented equivalents |
| `template_file` references | `assets[]` paths or content hashes |
| No preset source | Nests, PatternSlot, lifecycle state, effect summary, provenance |

Every executable field in a valid preset imports without silent semantic loss. UI-only fields may be synthesized. The reference imports round-trip `game-narrative` and `react-trpg-turn`, including signals, roles, output bindings, all four exit kinds, and per-state behavior. Export of fully expanded IR produces a preset bundle accepted by the existing validator; IR with unexpanded PatternSlots expands first or refuses export with structured diagnostics.

### 5. Validate-on-IR parity

The shared semantic facade accepts IR directly. The YAML path imports to IR and then uses the same validation core rather than maintaining an independent grammar. Validation covers capability presence, reachable states, template references, cycle policy, and IR structural rules, with structured diagnostics.

The existing `nexus42 preset validate` command owns the entry point; an alias or subcommand may expose IR validation, but there is no second implementation leaf named only `nexus42 workflow validate`. P1 parity covers the reference samples and preserves the YAML validation behavior through import→validate. The IR→Task mapping table records parity with orchestration-engine §8.2 for the later loader decision; it does not itself ship P4 dual-read.

### 6. Hashing and provenance

Canonical content identity covers the executable IR document and referenced assets. Approved identity is bound to the exact content that was validated and simulated. Patches after either step require a new hash and fresh validation/simulation before approval. Applied source identity preserves imported preset provenance where applicable and refuses silent source replacement during recovery.

Pattern/asset hash locking at Approve and hash verification at Apply belong to the pattern-package decision; P0/P1 do not claim that pinned pattern packages have shipped. Both stages shipped later — approval freeze in v1.239, Apply in v1.241 — and [the composition spec](../../../../docs/specs/orchestration/strategy-composition.md) owns them.

## Alternatives considered

- **Keep hand-authored YAML as the universal authoring model.** Rejected for the shared document: parallel TypeScript and Rust projections drift, and YAML cannot losslessly carry IR-only candidate, composition, effect, and provenance data.
- **Maintain handwritten Rust and TypeScript DTOs beside the schema.** Rejected: generated projections and drift checks provide one schema truth source.
- **Import only a convenient preset subset.** Rejected: dropping executable fields silently changes existing strategies. Compatibility import must cover the carrier table.
- **Create a separate YAML validator and IR validator.** Rejected: YAML imports into the shared semantic core so parity does not depend on two implementations.
- **Add a standalone `nexus42 workflow validate` implementation.** Rejected: the existing `preset validate` entry owns the facade without a competing CLI leaf.
- **Replace graph-flow or merge Work Artifact IR into this document.** Rejected: execution remains single-backend and Work output has a separate storage/commit SSOT.
- **Treat P0/P1 as proof of dual-read or IR-primary authoring.** Rejected: schema/import and validation do not ship production intake or change the primary authoring path.

## Consequences

P0/P1 provide a typed, generated contract and lossless preset import with validation parity on the reference bundles. Existing preset behavior remains representable, and later loader or Canvas work can consume the same document without inventing another grammar.

The decision costs a schema/codegen dependency and a canonical identity contract. Human-diff encoding, gate placement, and compatibility export behavior must remain explicit rather than becoming accidental properties of a serializer. Source freeze makes changed bytes fail closed, and rollback requires installing a prior approved snapshot.

This decision deliberately stops before P3–P6. Pattern packages with pinned hashes, Canvas compose, dual-read loading, IR-primary authoring, and retirement of the hand-authored YAML main path remain in the linked proposed record.
