# Nexus ACP Capability Set

**Status**: Normative  
**Document class**: Master  

## 0. Document position

This document defines the **minimal ACP-facing capability contract** between **Nexus CLI runtime (ACP Client)** and **user-owned local agents (ACP Agent)**. It is a functional contract layer; it does not freeze ACP wire-level RPC method names or JSON shapes.

The same logical `nexus.*` IDs may appear in **platform REST** contracts for **naming alignment** with CLI capability surfaces; **Nexus Platform does not expose the ACP wire protocol** — ACP is **CLI / local-runtime only** (Nexus as ACP Client ↔ user agents as ACP Agent).

**Normative architecture baseline**:

- **Nexus runtime** participates on the ACP wire only as ACP Client.
- **User-side agent** is the ACP Agent.
- **daemon runtime / daemon** is a local helper / supervisor. It is not an ACP Agent, not an ACP Server, and must not be advertised via ACP Registry as an agent.

Related docs: nexus-platform `v1-spec/architecture.md`, [`cli-spec.md`](./cli-spec.md).

**Naming note**: CLI executable **`nexus42`**; local supervisor is the **daemon runtime** (single-binary mode via `nexus42 daemon start`, crate `nexus-daemon-runtime`). Product name **Nexus** (42ch / Creative Hub). **`nexus.*`** is the stable logical capability ID prefix; capability IDs need not match executable names.

## 0.5 Runtime registry pointer (V1.53)

This spec is the logical catalog for `nexus.*` capabilities. Each entry lists the capability id and a one-line description. **It is not the runtime source of truth for dispatch.** The runtime SSOT is [`capability-registry.md`](capability-registry.md) (Draft overlay, V1.53).

---

## 1. Goals and non-goals

### 1.1 Goals

- Provide a **minimal, implementable** capability surface for V1.0 story/world advancement.
- Make **capability naming and versioning** stable for registry filtering and skills mapping.
- Document `initialize` handshake assumptions so clients and agents agree on negotiation, correlation, and safety gates.

### 1.2 Non-goals

- Replacing or duplicating the ACP specification.
- Defining platform HTTP APIs or sync delta schemas.
- Declaring Nexus daemon endpoints as ACP capabilities.

---

## 2. Topology

```text
[User] -> [nexus42 / daemon runtime] --ACP Client--> [Local/Remote ACP Agent]
               |
               +-> [Nexus Local API / IPC]
               +-> [ACP Registry]
```

Hard rule: any “serve ACP to external consumers” pattern is out of scope for Nexus v1.

---

## 3. Capability naming and versioning

### 3.1 Naming convention

Capabilities are identified by dot-separated hierarchical names owned by Nexus:

- Prefix: `nexus.*`
- Grouping mirrors CLI Spec sections: context, world, timeline, sync, manuscript, publish, observability

Examples:

- `nexus.context.whoami`
- `nexus.world.snapshot.get`
- `nexus.world.delta.propose`
- `nexus.timeline.event.append`
- `nexus.fork.create`
- `nexus.sync.push`
- `nexus.manuscript.read_range`
- `nexus.publish.chapter`
- `nexus.observability.health`

### 3.2 Versioning

Each capability carries a semver-style contract version independent of Nexus CLI semver:

- **Major**: breaking input/output semantics or side effects
- **Minor**: backward-compatible additions
- **Patch**: documentation or clarification changes

Agents must declare supported `(capability_id, major)` pairs at minimum. Clients may request minor features if present.

### 3.3 Capability sets

| Profile | Purpose | Notes |
| --- | --- | --- |
| `nexus.profile.minimal` | Smoke tests / doctor | read-only context + health |
| `nexus.profile.writer` | V1.0 default | world read + delta propose + manuscript bounded write + sync helpers |
| `nexus.profile.publisher` | Explicit publish flows | includes `publish.*` gated capabilities |

---

## 4. Minimal capability set

> **V1.34 implementation subset:** Daemon host tools for `nexus.context.whoami`, `nexus.workspace.info`, `nexus.work.get`, `nexus.work.patch`, `nexus.orchestration.schedule_status`, and `nexus.context.assemble` (local/read-only or `policy_blocked`) are specified in [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md). Remaining IDs in this section remain **logical contract only** until DF-46.

### 4.1 Context

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.context.whoami` | yes | Resolve active Nexus profile / creator context |
| `nexus.workspace.info` | yes | Workspace root, linked world ref, environment flags |
| `nexus.workspace.paths` | yes | Enumerate allowed roots from the active preset: e.g. V1.36 `novel-writing` **`Works/<work_ref>/`** (正文 under **`Stories/`**), **`.nexus42/references/<run-id>/`**, workspace `.agents/skills/`, and other policy-defined subtrees — see [novel-writing/workflow-profile.md](./novel-writing/workflow-profile.md) |
| `nexus.context.assemble` | yes | Assemble a stable writing context from confirmed KB / canon timeline / memory slices |

### 4.2 World read

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.world.snapshot.get` | yes | Consistent read of structured world snapshot |
| `nexus.world.state.query` | yes | Query KB/timeline slices needed for reasoning |
| `nexus.timeline.recent.get` | yes | Fetch recent timeline tail for continuity |
| `nexus.kb_snapshot.read` | optional | Focused KB snapshot read; conceptual equivalent of `kb_snapshot_read` in notes/design docs |

### 4.3 World mutation

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.world.delta.propose` | yes | Produce structured proposed delta package |
| `nexus.world.delta.apply` | optional | Apply staged deltas locally under policy |
| `nexus.timeline.event.append` | yes | Append new events; must not silently rewrite canon history |
| `nexus.fork.create` | optional | Explicit branch creation when rewrite-past is intended |

Permission note:

- Public / invited / private world access is determined by world policy, membership, and pairing state; capability presence alone does not grant private access.

### 4.4 Sync

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.sync.prepare_push` | yes | Build idempotent push bundle metadata |
| `nexus.sync.push` | yes | Submit structured deltas via runtime-owned client |
| `nexus.sync.pull` | optional | Agent-triggered pull |
| `nexus.sync.status` | yes | Surface outbox / conflicts / cursors |

Platform credentials remain in Nexus runtime, not exported to the agent process in v1.

### 4.5 Manuscript

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.manuscript.list` | yes | List manuscript files under whitelist |
| `nexus.manuscript.read_range` | yes | Read a bounded range for prompting |
| `nexus.manuscript.write` | yes | Write only within whitelist paths and size quotas |
| `nexus.manuscript.phase.get` | yes | Read current manuscript phase |
| `nexus.manuscript.phase.set` | optional | Move between brainstorm / draft / review / finalize with runtime checks |

Runtime note:

- If `output_manuscript=false`, `nexus.manuscript.write` may be skipped by policy, but the capability contract remains available and unchanged.

### 4.6 Publish

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.publish.chapter` | optional | User-attested publish flow for a chapter artifact |
| `nexus.publish.story` | optional | User-attested publish flow for a story artifact |

### 4.6A Research

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.research.query` | optional | Query local-only `ReferenceSource` index / excerpts without syncing source material |

### 4.7 Observability

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.trace.correlation` | yes | Propagate correlation IDs across tool calls |
| `nexus.runtime.health` | yes | Agent-visible health, registry reachability, sync state |

---

## 5. `initialize` handshake assumptions

### 5.1 Preconditions

- Transport is established, with JSON-RPC over stdio as V1.0 default.
- Client has selected an agent identity from Registry or local override.

### 5.2 Required negotiation topics

During `initialize` or equivalent bootstrap:

1. ACP protocol version compatibility.
2. `nexus.acp_contract_version` announced by client.
3. Capability intersection returned by agent.
4. Profile selection.
5. Workspace binding and whitelist digest.
6. Safety mode confirmation.

### 5.3 Session metadata

- Every session has a `session_correlation_id`.
- Registry-provided signatures or checksums should be stored if available.

### 5.4 Degraded handshake

If handshake succeeds but capability set is incomplete:

- Client enters **Degraded** mode with explicit user-visible reason.
- Operations depending on missing capabilities are blocked.

---

## 6. Invariants and forbidden behaviors

- No canon history silent rewrite.
- No promotion of provisional facts to shared canon without explicit confirmation path.
- No arbitrary filesystem access.
- No secret exfiltration by default.
- No ACP role inversion.

---

## 7. Change management

- Bump `nexus.acp_contract_version` on breaking capability semantics.
- Registry entries should pin compatible contract version ranges.
- V1.53 retires the skills-export CLI/spec line (DF-50 Cancelled); runtime capability dispatch consistency is governed by [`capability-registry.md`](capability-registry.md).

---

## 8. Open items

- Map each logical `nexus.*` capability to concrete ACP tool/resource schemas.
- Decide whether `world.delta.apply` is agent-side or runtime-side by default.
- Define maximum manuscript read/write quotas and default timeouts.
