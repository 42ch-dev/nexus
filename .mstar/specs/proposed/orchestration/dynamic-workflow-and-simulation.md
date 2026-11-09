---
module: nexus orchestration / harness
date: 2026-11-10
status: proposed
problem_type: design
category: orchestration
severity: info
tags:
  - workflow
  - preset
  - graph-flow
  - simulation
  - hitl
  - agent-generation
---

# Dynamic Workflow Generation and Orchestration Simulation

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — not implement authority. Nothing here schedules work or claims a shipped feature. |
| **Document class** | Exploration |
| **Scope** | Product and engine direction for strategy workflows: Agent-generated workflow IR, validation, dry-run simulation, human approve, then apply onto the existing preset / graph-flow execution path. Strategy Canvas remains an inspection and HITL adjustment surface, not a mandatory drag-edit authoring path. |
| **Coordinates with** | [orchestration-engine.md](../../active/orchestration/orchestration-engine.md) (graph-flow + preset SSOT), [docs/strategy-authoring.md](../../../../docs/strategy-authoring.md), [STRATEGY.md](../../../../STRATEGY.md) (Harness / Canvas pillars), [lightweight-core-modular-packaging.md](../architecture/lightweight-core-modular-packaging.md) (crate packaging implications) |

The key words MUST, MUST NOT, SHOULD, and MAY are RFC 2119 terms for a conforming implementation of this proposal only.

---

## 1. Problem

Creators and developers need orchestration strategies (capability routing + prompt graphs) without being forced to hand-author every edge on a canvas.

Today:

- Presets are filesystem bundles (`preset.yaml` + templates) loaded into graph-flow (outer state machine + inner DAGs).
- Strategy Canvas projects the preset graph for **developer explainability** ([STRATEGY.md](../../../../STRATEGY.md)).
- Validation already exists (`nexus42 preset validate` / shared semantic facade).
- Runtime already has durable execution and human-wait gates ([orchestration-engine.md](../../active/orchestration/orchestration-engine.md) §15).

Gap: there is no first-class **generate → validate → simulate → approve → apply** loop. Authors either write YAML by hand or are steered toward canvas editing. Agent assistance for *producing* a candidate workflow, and a **sandbox simulation before production apply**, are missing as product contracts.

---

## 2. Goals

1. **Generate-first authoring.** Agents MAY produce a Workflow IR draft from a requirements / intent description via the existing Host / ACP plane.
2. **Canvas is inspection + HITL, not mandatory drag-edit.** Users MUST NOT be required to drag-edit the strategy canvas to create a workflow. Canvas SHOULD remain the explainability and adjustment surface.
3. **Safe landing.** Generated workflows MUST NOT silently apply to production schedules or live sessions. Apply MUST be an explicit approve step after validation and simulation.
4. **One execution backend.** Simulation and production MUST share graph semantics with the retained graph-flow engine; this proposal MUST NOT introduce a second orchestration runtime.
5. **Modular packaging.** Workflow IR MUST be a stable, thinly dependent crate/API. The simulator SHOULD be an optional feature package so production Connect / CLI default graphs need not compile it.

### 2.1 Non-goals

- Replacing preset bundles or graph-flow.
- A first-party “player” app or second runtime ([STRATEGY.md](../../../../STRATEGY.md) standing decisions).
- Guaranteeing identical LLM quality across providers; generation quality is product evaluation, not engine identity.
- Exactly-once external side effects during simulation (simulation MUST stub or record, never commit).

---

## 3. Proposed lifecycle

```text
Intent / requirements
        │
        ▼
   Generate (Agent → Workflow IR draft)
        │
        ▼
   Validate (semantic + capability registry)
        │ fail ──► regenerate / repair
        ▼ pass
   Simulate (dry-run trajectory, wait points, effect classes)
        │
        ▼
   Approve (HITL on TS Canvas / review UI; optional local IR patches)
        │ reject ──► regenerate or abandon
        ▼ accept
   Apply (install preset projection / bind schedule; content-hash freeze)
        │
        ▼
   Execute (existing durable engine + WaitingForInput gates)
```

| Stage | Normative rule |
| --- | --- |
| **Generate** | Output is a **draft** IR artifact (and MAY emit a preset projection). MUST NOT open a driven production session as a side effect of generation alone. |
| **Validate** | MUST reuse or extend the shared preset/IR semantic validation facade (capabilities present, reachable states, template refs, cycle policy). Failures return structured diagnostics. |
| **Simulate** | MUST run in an isolated context with stubbed / recorded capabilities. MUST NOT perform world-settling writes, ACP mutations that commit narrative state, or cloud sync. MUST emit a trajectory (nodes visited, predicted waits, effect class tags). |
| **Approve** | MUST be an explicit human (or policy-bound) accept. Canvas MAY show IR + simulation results and accept constrained patches. Silent auto-apply is forbidden. |
| **Apply** | Persists an approved preset (or equivalent) with frozen source identity / content hash consistent with orchestration-engine recovery rules. Only then may schedule admission / run start use it. |

---

## 4. Relationship to presets and graph-flow

| Concept | Role |
| --- | --- |
| **Workflow IR** | Shared stable representation for generators, validators, simulators, and TS projections. |
| **preset.yaml + templates** | One persistence / loader projection of IR (compatibility with today’s loader). IR is the authoring SSOT for the generate path; loader remains the execution intake until an explicit alternate intake is accepted. |
| **graph-flow** | Sole execution backend (outer state machine + inner DAGs). Simulator SHOULD interpret the same graph construction rules with dry-run task adapters. |
| **Strategy Canvas (TS)** | Read-mostly projection of IR / preset + simulation annotations; optional patch UX. Not the only authoring entry. |

---

## 5. Safety invariants

1. **No silent production apply.** Draft → Approved is a state transition that MUST require an explicit approve action.
2. **Simulation effect fence.** Capability invocations during simulate MUST be classified (`pure`, `read`, `write-candidate`, `external`). Write-candidate and external calls MUST be stubbed or recorded only.
3. **Complementary human gates.** Approve gates *adoption of a graph*; engine `WaitingForInput` gates *continuation of a run*. Neither replaces the other.
4. **Source freeze after Apply.** Recovery MUST refuse silent source swap (same discipline as orchestration-engine §15 content-hash / source identity).
5. **User-facing layer stays TypeScript.** Review UI and Canvas remain TS consumers of generated contracts; Rust owns IR, validation, simulation, and execution truth.

---

## 6. Modularization impact

| Package | Dependency posture | Publish intent |
| --- | --- | --- |
| `workflow-ir` (name TBD) | Minimal: serde types, schema constants, pure validators | Stable crate + schema; generators and UI depend here, not on full orchestration |
| `orchestration` runtime | Optional feature or separate version track; loads IR/preset → graph-flow | Existing crate; avoid pulling simulator into default Connect graph |
| `orchestration-sim` (or `orchestration` feature `simulate`) | Depends on IR + stub capability ports | Dev / review / Control Room; omit from minimal runtime |
| Generator tooling | Agent-host capability or thin tool crate | High churn; must not force `nexus-core` rebuilds for prompt iteration |
| TS Canvas / service | Consumes codegen DTOs only | Locked user-facing stack |

This aligns with [lightweight-core-modular-packaging.md](../architecture/lightweight-core-modular-packaging.md): IR is spine-adjacent stable API; simulator is a feature package for “fast iteration without whole-graph rebuild.”

---

## 7. Suggested dual-track roadmap (non-binding)

| Phase | Packaging track (A) | Dynamic workflow track (B) |
| --- | --- | --- |
| 0 | Measure compile fan-out | Inventory validate / Canvas / wait gates vs generate+simulate gaps |
| 1 | Thin narrative core | IR schema covering a preset subset + draft/approved states |
| 2 | Independent version tracks for orch / wasm / agent | Validate-on-IR + simulator MVP (no Apply) |
| 3 | Module SDK publish hardening | Generate → Approve → Apply; TS inspection UI |
| 4 | Feature-unification / DX | Simulation effect taxonomy + schedule admission integration |
| 5 | Optional WIT/Component ABI V2 | Optional IR V2 — neither blocks V1 |

---

## 8. Open questions

1. Is Workflow IR a strict superset of `PresetManifest`, or a distinct document with a lossy/lossless preset projection?
2. Where does generation live operationally — builtin capability, Develop-hub tool, or CLI-only first?
3. What minimum simulation fidelity is “honest enough” for Approve (node reachability only vs stubbed LLM judge branches)?
4. Who may Approve in content-creator vs developer entrances?

---

## 9. References

- [orchestration-engine.md](../../active/orchestration/orchestration-engine.md)
- [docs/strategy-authoring.md](../../../../docs/strategy-authoring.md)
- [STRATEGY.md](../../../../STRATEGY.md)
- graph-flow (rs-graph-llm) pinned in workspace (`=0.8.1` in current Cargo.toml; engine Master historically cites 0.8.0 — pin is workspace SSOT)
