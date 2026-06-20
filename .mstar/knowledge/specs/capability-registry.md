# Capability Registry — Draft Overlay v1

**Status**: Draft (V1.53 P-1 — initial framework; details iterate in P0)  
**Document class**: Master overlay (pending P-last promote decision)  
**Created**: 2026-06-20  
**Last updated**: 2026-06-20 (V1.53 P-1)  
**Scope**: Runtime SSOT for Nexus `nexus.*` capability dispatch  
**Coordinates with**: [acp-capability-set.md](acp-capability-set.md), [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md), [acp-client-tech-spec.md](acp-client-tech-spec.md), [orchestration-engine.md](orchestration-engine.md)  
**Iteration compass**: [v1.53-capability-surface-completion-and-skills-cli-cleanup-delivery-compass-v1.md](../../iterations/v1.53-capability-surface-completion-and-skills-cli-cleanup-delivery-compass-v1.md)

---

## 0. Document position

This Draft overlay defines the target runtime registry shape for Nexus `nexus.*` capability dispatch. It does **not** replace [acp-capability-set.md](acp-capability-set.md): the capability-set spec remains the logical catalog (capability id + one-line description). This registry spec is the runtime SSOT for handler binding, ACP wire shape, failure mode, and test-vector coverage.

Non-overlap rule: **catalog = ID + one-liner**; **registry = handler + wire + failure mode + test vector**.

---

## 1. Scope / non-goals

### 1.1 Scope

- Registry fields needed to route `nexus.*` capabilities consistently.
- Authority chain between catalog, bridge, ACP tech spec, orchestration, and runtime handler code.
- Promote-decision checklist for P-last.

### 1.2 Non-goals

- Full field semantics in P-1; P0 owns details.
- New ACP wire protocol design outside existing ACP-client topology.
- Platform REST contracts, cloud publish, standalone MCP, or third-party registry.
- Skills-export CLI compatibility; DF-50 is Cancelled.

---

## 2. Registry field skeleton

| Field | One-line meaning | P0 detail status |
| --- | --- | --- |
| `id` | Stable `nexus.*` capability id. | Deferred to P0 for exact naming and aliases. |
| `access` | Read/write/policy classification used by admission and audit. | Deferred to P0. |
| `admission` | Ordered fail-closed gates before handler dispatch. | Deferred to P0. |
| `handler` | Runtime handler binding or adapter entrypoint. | Deferred to P0. |
| `ACP wire` | Request/response/failure envelope exposed to ACP-facing callers. | Deferred to P0. |
| `failure mode` | Stable error code/reason contract for denied or failed execution. | Deferred to P0. |
| `handler test vector` | Required success/failure/admission test vector proving the registry row. | Deferred to P0. |

---

## 3. Authority chain

1. Repo root `AGENTS.md` and active iteration compass define scope and local-first boundaries.
2. `acp-capability-set.md` defines the logical capability catalog.
3. This Draft overlay defines the runtime registry contract for active V1.53 work.
4. `agent-nexus-tool-bridge.md` defines mediated external-agent tool invocation and admission invariants.
5. `acp-client-tech-spec.md` and `orchestration-engine.md` define ACP client topology and schedule/tool request participation.
6. Runtime implementation must not create a second dispatch table for the same `nexus.*` id.

---

## 4. Boundaries with existing specs

| Existing spec | Boundary |
| --- | --- |
| `acp-capability-set.md` | Logical catalog only; no runtime dispatch authority. |
| `agent-nexus-tool-bridge.md` | Entrypoint/admission history and V1.34 minimal bridge; V1.53 registry can become the shared runtime SSOT underneath it. |
| `acp-client-tech-spec.md` | ACP client behavior and handshake; registry rows may reference wire details but do not redefine ACP. |
| `orchestration-engine.md` | Schedules and worker tool requests; registry may serve schedule-initiated tool dispatch but does not replace preset grammar. |
| `cli-spec.md` | User-visible commands; capability registry is not a CLI command tree. |

---

## 5. Acceptance (spec-level)

Promote decision checklist for P-last:

- [ ] P0 has filled field semantics for all registry fields.
- [ ] P0 has recorded explicit cutover triggers and no lingering dual dispatch path.
- [ ] P1 has added five read-heavy registry rows and handler test vectors.
- [ ] `acp-capability-set.md` remains catalog-only and points here for runtime SSOT.
- [ ] `agent-nexus-tool-bridge.md` references the registry seam without reviving skills-export.
- [ ] P-last decides whether this overlay is promoted into a Master or retained as a Draft overlay with a successor plan.
