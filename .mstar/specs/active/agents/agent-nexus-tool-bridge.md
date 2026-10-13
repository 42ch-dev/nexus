# Agent Nexus Tool Bridge — Normative Specification v1

**Status**: Normative — retained core tool-execution contract
**Document class**: Master  
**Created**: 2026-06-04
**Scope**: Retained admission and dispatch of **Nexus logical capabilities** (`nexus.*`) through `nexus-core`.
**Coordinates with**:

- [acp-capability-set.md](acp-capability-set.md) — full logical capability catalog (mostly deferred DF-46)
- [agent-host.md](agent-host.md) — Managed-only host, mediation invariants
- [orchestration-engine.md](../orchestration/orchestration-engine.md) — schedule tool dispatch
- [local-runtime-boundary.md](../architecture/local-runtime-boundary.md) — current core / TS service / Agent ownership
- [creator-workflow.md](../creator/creator-workflow.md) — FL-E stages; Work read/patch tools

---

## 1. Purpose

Preset orchestration drives **multi-step** creative work via schedules and capabilities (`acp.prompt`, `judge.llm`, `creator.read_memory`, …). External LLM agents can also **actively** request Nexus context during a session (tool calls). All such execution funnels through a **single mediated path** in `nexus-core` so that:

- Permissions and workspace boundaries are enforced uniformly.
- Audit trails exist for agent-initiated mutations.
- The same handlers serve the API tool-execute route and the schedule lane.

**Non-goals (durable deferrals)**: spawn `nexus42` per tool (DF-48); standalone MCP server (DF-49); full `acp-capability-set` implementation (DF-46).

---

## 2. Current execution authority

The retained execution authority is [`crates/nexus-core/src/execution/capabilities.rs`](../../../../crates/nexus-core/src/execution/capabilities.rs):

- `execute_tool(&ToolContext, &ToolExecuteRequest)` runs admission, calls `host_tool_registry().dispatch(...)`, and audits both success and refusal. `ToolContext` supplies resolved core/domain inputs.
- The allowlist is derived from the **same dispatch spine**: static builtin row → admitted peer tool → admitted user capability from the live orchestration registry. `spine_resolves` and `dispatch` share that order; unresolved IDs return lowercase `not_supported` without invoking a handler. Builtin creator/workspace/permission gates and per-handler ownership checks remain enforced; peer/user tools retain their own admission boundaries.
- `host_tool_registry()` is a process-global `LazyLock` registry with **30 static tools (28 `nexus.*` + 2 `fs/*`)**. The current roster, exclusion of `nexus.profile.*` metadata, and caller surface are recorded in [`retained_peer_contracts.rs`](../../../../crates/nexus-core/tests/retained_peer_contracts.rs), `host_tool_registry_roster_is_the_declared_nexus_surface`.
- The core's host-tool registry and the [`nexus-orchestration` `CapabilityRegistry`](../../../../crates/nexus-orchestration/src/capability/mod.rs) are distinct. The latter owns orchestration capabilities; it is not a second static `nexus.*` dispatch table. [capability-registry.md](capability-registry.md) is the shipped registry contract.
