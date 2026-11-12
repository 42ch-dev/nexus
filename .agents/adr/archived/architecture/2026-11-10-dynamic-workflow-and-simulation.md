# ADR: Dynamic Workflow Generation and Orchestration Simulation

Status: implemented — split into three records on 2026-11-13
Archived: 2026-11-13 — split into three records
Superseded-by: ../../implemented/architecture/2026-11-10-workflow-ir-document-model.md, ../../implemented/architecture/2026-11-10-strategy-simulation-fence.md, ../../proposed/architecture/2026-11-10-canvas-composition-and-pattern-packages.md
Date: 2026-11-10
Scope: Product and engine direction for strategy workflows: **Workflow IR as a core universal mechanism** (not a developer-only tool). End users MAY **nest or compose** IR (including published patterns); developers primarily **author and publish reusable patterns** (subgraph / template packages). Lifecycle is **Generate or Compose → validate → simulate → approve → apply** onto the existing graph-flow path, with `preset.yaml` retained as a **compat import/export**. Strategy Canvas is a **composition surface** (composition + HITL are the duty; explainability is the applied-IR inspector view) — neither a mandatory from-scratch drag-edit-only authoring path nor a pure read-only viewer of Agent output.
Source: migrated from the retired tracked specs corpus

## Problem

Creators and developers need orchestration strategies (capability routing + prompt graphs) without being forced either to hand-author every edge on a canvas or to treat Agent drafts as opaque, non-composable blobs.

Today:

- Presets are filesystem bundles (`preset.yaml` + templates) loaded into graph-flow (outer state machine + inner DAGs).
- Strategy Canvas today projects the preset graph primarily as a **developer explainability** view ([STRATEGY.md](../../../../STRATEGY.md)); the target is a **composition surface** (see Goals).
- Validation already exists (`nexus42 preset validate` / shared semantic facade).
- Runtime already has durable execution and human-wait gates ([orchestration-engine.md](../../../../docs/specs/orchestration/orchestration-engine.md) §15).

Gaps:

1. There is no first-class **generate → validate → simulate → approve → apply** loop (Agent-assisted draft production + sandbox simulation before production apply).
2. Workflow representation is not yet positioned as a **universal IR** that end users can nest/compose and that developers package as reusable **patterns**.
3. Canvas is not yet designed as a **composition surface** (controlled nest/combine of patterns and drafts) — risk of collapsing into either “must drag from zero” or “read-only Agent dump.”

---

## Decision

### 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — not implement authority. Nothing here schedules work or claims a shipped feature. |
| **Document class** | Exploration |
| **Scope** | Product and engine direction for strategy workflows: **Workflow IR as a core universal mechanism** (not a developer-only tool). End users MAY **nest or compose** IR (including published patterns); developers primarily **author and publish reusable patterns** (subgraph / template packages). Lifecycle is **Generate or Compose → validate → simulate → approve → apply** onto the existing graph-flow path, with `preset.yaml` retained as a **compat import/export**. Strategy Canvas is a **composition surface** (composition + HITL are the duty; explainability is the applied-IR inspector view) — neither a mandatory from-scratch drag-edit-only authoring path nor a pure read-only viewer of Agent output. |
| **Coordinates with** | [orchestration-engine.md](../../../../docs/specs/orchestration/orchestration-engine.md) (graph-flow + current preset intake), [docs/strategy-authoring.md](../../../../docs/strategy-authoring.md), [STRATEGY.md](../../../../STRATEGY.md) (Harness / Canvas pillars), [lightweight-core-modular-packaging.md](../../proposed/architecture/2026-11-10-lightweight-core-modular-packaging.md) (crate packaging implications), Canvas + Workflow IR redesign ([PR #483](https://github.com/42ch-dev/nexus/pull/483); schema, Canvas ops, P4/P6 intake cutover), Work Artifact IR ([PR #485](https://github.com/42ch-dev/nexus/pull/485); sibling, Work-output SSOT only — not this IR) |
| **Merge / land order** | Merge **this PR (#482) first**, then [#483](https://github.com/42ch-dev/nexus/pull/483). [#485](https://github.com/42ch-dev/nexus/pull/485) is independent and MAY proceed in parallel. Neither merge confers implement authority until Draft/Master promotion. |

The key words MUST, MUST NOT, SHOULD, and MAY are RFC 2119 terms for a conforming implementation of this proposal only.

---

### 2. Goals

1. **Universal Workflow IR.** IR is a **core shared mechanism** for generators, validators, simulators, Canvas, pattern packages, and the execution intake projection. It MUST NOT be framed as a developer-only artifact.
2. **End-user nest / compose.** End users MAY nest or compose IR units (including published patterns and Agent drafts) into a single candidate workflow, subject to validation and depth / policy limits (see Open questions).
3. **Developer patterns.** Developers primarily **author, version, and publish patterns** — reusable subgraph / template packages — rather than being the only party allowed to touch IR. Patterns are first-class inputs to Compose.
4. **Generate or Compose authoring.** A candidate IR MAY originate from Agent generation **or** from user-driven composition (nesting / combining patterns and drafts). Both paths feed the same validate → simulate → approve → apply pipeline.
5. **Canvas is a composition surface.** Composition and HITL are the **duty**: Canvas SHOULD support controlled nest/compose, constrained patches, and an explicit approve UX. Explainability is the **view** (applied-IR inspector: projection of IR + simulation annotations). Users MUST NOT be required to drag-edit from scratch as the sole authoring path. Canvas MUST NOT be the only entry (CLI / Agent / programmatic Compose remain valid). Canvas MUST NOT be reduced to a pure read-only inspection of Agent output (that would discard the composition duty).
6. **Safe landing.** Candidate workflows MUST NOT silently apply to production schedules or live sessions. Apply MUST be an explicit approve step after validation and simulation, bound to the checked content hash.
7. **One execution backend.** Simulation and production MUST share graph semantics with the retained graph-flow engine; this proposal MUST NOT introduce a second orchestration runtime.
8. **Modular packaging.** Workflow IR MUST be a stable, thinly dependent crate/API shared by generators, Canvas, and pattern runtime. The simulator SHOULD be an optional feature package so production Connect / CLI default graphs need not compile it. Patterns SHOULD version independently of the host.

### 3. Proposed lifecycle

```text
Intent / requirements  ──or──  Patterns + drafts (user nest/compose)
        │                              │
        └──────────────┬───────────────┘
                       ▼
        Generate or Compose → Workflow IR candidate
                       │
                       ▼
              Validate (semantic + capability registry
                        + nest/compose policy)
                       │ fail ──► regenerate / repair / recompose
                       ▼ pass
              Simulate (dry-run trajectory, wait points,
                        effect classes; nested effects rolled up)
                       │
                       ▼
              Approve (HITL on TS Canvas / review UI;
                       optional local IR patches; still one IR truth)
                       │ reject ──► regenerate, recompose, or abandon
                       ▼ accept
              Apply (install preset projection / bind schedule;
                     content-hash freeze)
                       │
                       ▼
              Execute (existing durable engine + WaitingForInput gates)
```

| Stage | Normative rule |
| --- | --- |
| **Generate or Compose** | Output is a **candidate** IR artifact (and MAY emit a preset projection). **Generate:** Agent produces a draft via Host / ACP. **Compose:** user nests or combines patterns and/or drafts (Canvas or equivalent). MUST NOT open a driven production session as a side effect of generate/compose alone. Local generation inside a compose session MUST still converge on a **single IR truth** for the candidate (no divergent shadow graphs). |
| **Validate** | MUST reuse or extend the shared preset/IR semantic validation facade (capabilities present, reachable states, template refs, cycle policy) **plus** nest/compose policy (depth limits, pattern pin resolution, effect-class rollup consistency). Failures return structured diagnostics. |
| **Simulate** | MUST run in an isolated context with stubbed / recorded capabilities. MUST NOT perform world-settling writes, ACP mutations that commit narrative state, or cloud sync. MUST emit a trajectory (nodes visited, predicted waits, effect class tags), including **rolled-up effect classes from nested subgraphs**. |
| **Approve** | MUST be an explicit human (or policy-bound) accept, bound to the content hash that was validated and simulated. Canvas MAY show IR + simulation results, support constrained patches, and surface nest/compose structure. Patches after Validate/Simulate MUST re-run Validate and Simulate; results from an earlier graph MUST NOT approve a later one. Silent auto-apply is forbidden. |
| **Apply** | Persists an approved **Workflow IR** identity with frozen source identity / content hash consistent with orchestration-engine recovery rules. A `preset.yaml` projection MAY be written as compat export. Switching execution intake off YAML follows [#483](https://github.com/42ch-dev/nexus/pull/483) §7 P4/P6. Only then may schedule admission / run start use it. |

---

### 4. Relationship to presets, patterns, and graph-flow

| Concept | Role |
| --- | --- |
| **Workflow IR** | **Universal** shared representation for generators, validators, simulators, Canvas compose/projection, pattern packages, and execution intake. Not developer-only. |
| **Pattern packages** | Versioned reusable subgraph / template packages published primarily by developers (and potentially advanced creators). Consumed by Compose (nest/combine) and by generators as building blocks. SHOULD resolve by pin (exact / range policy TBD — Open questions). |
| **preset.yaml + templates** | **Compat import/export** (and a migration-period projection). Workflow IR is the **authoring + review SSOT** for generate/compose/validate/simulate/approve (scope and schema in [#483](https://github.com/42ch-dev/nexus/pull/483)). Switching the execution intake off YAML is gated by #483 §7 P4 (dual-read) and P6 (hand-authored YAML main path retired; **import/export retained**). |
| **graph-flow** | Sole execution backend (outer state machine + inner DAGs). Simulator SHOULD interpret the same graph construction rules with dry-run task adapters. Nested IR MUST flatten or bind into the same engine model at Apply (or at a defined pre-Apply expansion step). |
| **Strategy Canvas (TS)** | **Composition surface**: composition + HITL are the duty; explainability is the applied-IR inspector view. Supports controlled nest/compose of patterns and drafts, projection of IR + simulation annotations, constrained patch / approve UX. **Not** the only authoring entry; **not** a pure read-only dump of Agent output; **not** drag-from-scratch-only. |

**Authority product forms (cross-link only):** strategy side = **Workflow IR** (candidate / approved / applied; persistent encoding is decided in [#483](https://github.com/42ch-dev/nexus/pull/483) §9). Work-output side = **Work Artifact IR** ([#485](https://github.com/42ch-dev/nexus/pull/485); separate document and type). `preset.yaml` appears only on the compat/import/export path.

---

### 5. Safety invariants

1. **No silent production apply.** Candidate → Approved is a state transition that MUST require an explicit approve action.
2. **Simulation effect fence.** Capability invocations during simulate MUST be classified (`pure`, `read`, `write-candidate`, `external`). Write-candidate and external calls MUST be stubbed or recorded only.
3. **Nested effect rollup.** When IR nests subgraphs / patterns, child effect-class tags MUST bubble up to the parent candidate so Approve and Simulate see the worst-case / union fence — nesting MUST NOT hide write-candidate or external effects.
4. **Complementary human gates.** Approve gates *adoption of a graph*; engine `WaitingForInput` gates *continuation of a run*. Neither replaces the other.
5. **Source freeze after Apply.** Recovery MUST refuse silent source swap (same discipline as orchestration-engine §15 content-hash / source identity). Pattern pins used at Apply MUST be recorded in the frozen identity story (exact pin / hash policy is detailed in [#483](https://github.com/42ch-dev/nexus/pull/483)).
6. **User-facing layer stays TypeScript.** Review UI and Canvas remain TS consumers of generated contracts; IR schema truth is JSON Schema under `schemas/daemon-api/` (see #483); Rust owns validation, simulation, and execution truth.

Detailed safety and migration gates live in [#483](https://github.com/42ch-dev/nexus/pull/483) §7–§8 so the two documents do not drift. This section is the direction-level fence only.

---

### 6. Modularization impact

| Package | Dependency posture | Publish intent |
| --- | --- | --- |
| `workflow-ir` (name TBD) | Minimal: serde types, schema constants, pure validators, nest/compose structural rules | Stable crate + schema; **shared by** generators, Canvas, pattern runtime, and validators — not only by full orchestration |
| Pattern packages | IR subgraph + templates + manifest; independent semver | Publishable units; Compose and generators depend on pins, not host rebuilds |
| `orchestration` runtime | Optional feature or separate version track; loads IR/preset → graph-flow; expands nested patterns at defined boundary | Existing crate; avoid pulling simulator into default Connect graph |
| `orchestration-sim` (or `orchestration` feature `simulate`) | Depends on IR + stub capability ports; understands nested effect rollup | Dev / review / Control Room; omit from minimal runtime |
| Generator tooling | Agent-host capability or thin tool crate | High churn; must not force `nexus-core` rebuilds for prompt iteration |
| TS Canvas / service | Consumes codegen DTOs only; compose UX + projection | Locked user-facing stack |

This aligns with [lightweight-core-modular-packaging.md](../../proposed/architecture/2026-11-10-lightweight-core-modular-packaging.md): IR is a spine-adjacent stable API; patterns are independently versioned feature content; the simulator is a feature package for review graphs (companion §5), so default production graphs need not compile it.

---

### 7. Suggested dual-track roadmap (non-binding)

| Phase | Packaging track (A) | Dynamic workflow track (B) |
| --- | --- | --- |
| 0 | Measure compile fan-out | Inventory validate / Canvas / wait gates vs generate+simulate+compose gaps |
| 1 | Thin narrative core | IR schema covering a preset subset + candidate/approved states; pattern manifest sketch |
| 2 | Independent version tracks for orch / wasm / agent | Validate-on-IR + simulator MVP (no Apply); nest depth policy draft |
| 3 | Module SDK publish hardening | Generate **or** Compose → Approve → Apply; **Canvas compose/nest MVP** (controlled combine of patterns + drafts) |
| 4 | Feature-unification / DX | Simulation effect taxonomy + nested effect rollup; schedule admission integration; pattern pin recording at Apply |
| 5 | Optional WIT/Component ABI V2 | Optional IR V2 / richer pattern marketplace — neither blocks V1 |

---

### 8. Open questions

1. Is Workflow IR a strict superset of `PresetManifest`, or a distinct document with a lossy/lossless preset projection?
2. Where does generation live operationally — builtin capability, Develop-hub tool, or CLI-only first?
3. What minimum simulation fidelity is “honest enough” for Approve (node reachability only vs stubbed LLM judge branches)?
4. Who may Approve in content-creator vs developer entrances (and does composing a published pattern require a different trust / Approve policy than generating a novel graph)?
5. What nest depth / cycle policy is safe for Compose (hard max depth, DAG-only nesting, or allow bounded recursion with expansion at Apply)?
6. Pattern version pins vs hot-update: exact pin at Compose-time, re-resolve at Apply, or allow controlled hot-reload after Apply with source-freeze exceptions?
7. During a compose session, how do **local / partial Agent regenerations** of a nested region stay merged into a **single IR truth** without forking shadow candidates the user cannot reconcile?

For iteration `v1.236-adr-foundation`, these seven questions are answered in `.mstar/iterations/v1.236-adr-foundation/specs/workflow-ir-decisions.md`, the round-1 decision record; YAML/Canvas/execution switches remain governed by the redesign ADR's phase plan.

---

### 9. References

- [orchestration-engine.md](../../../../docs/specs/orchestration/orchestration-engine.md)
- [docs/strategy-authoring.md](../../../../docs/strategy-authoring.md)
- [STRATEGY.md](../../../../STRATEGY.md)
- graph-flow (rs-graph-llm) pinned per crate at `=0.8.1` in `crates/nexus-core/Cargo.toml` and `crates/nexus-orchestration/Cargo.toml` (not a root `[workspace.dependencies]` entry; engine Master historically cites 0.8.0)
- Canvas + Workflow IR redesign — [PR #483](https://github.com/42ch-dev/nexus/pull/483) (detailed schema, Canvas ops, P4/P6; merge after this PR)
- Work Artifact IR — [PR #485](https://github.com/42ch-dev/nexus/pull/485) (sibling Work-output SSOT; independent, MAY proceed in parallel)

## Alternatives considered

Not stated as a separate comparison in the migrated proposal; where the body names a rejected direction, that record stays in the proposal.

## Consequences

Not stated as a separate acceptance list; where the proposal defines a landing or promotion sequence, it stays in the proposal.

## Risks

- Replacing graph-flow as the sole execution backend, or removing the preset bundle as an **import/compat export** format. (What *is* being replaced is “`preset.yaml` as the only authoring SSOT” — Workflow IR takes that role; see §4 and [#483](https://github.com/42ch-dev/nexus/pull/483).)
- A first-party “player” app or second runtime ([STRATEGY.md](../../../../STRATEGY.md) standing decisions).
- Guaranteeing identical LLM quality across providers; generation quality is product evaluation, not engine identity.
- Exactly-once external side effects during simulation (simulation MUST stub or record, never commit).
- Making Canvas the *only* entry (CLI / Agent / programmatic Compose remain valid).
- Unbounded recursive nesting without policy (depth and effect bubbling are design constraints, not open-ended).
- Absorbing Work-tree artifacts or knowledge/memory stores into this Workflow IR ([PR #485](https://github.com/42ch-dev/nexus/pull/485) is a sibling for Work output).

---
