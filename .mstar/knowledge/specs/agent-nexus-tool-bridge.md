# Agent Nexus Tool Bridge — Normative Specification v1

**Status**: Draft (V1.34 — target Shipped with iteration close)  
**Created**: 2026-06-04  
**Scope**: How **external ACP Agents** invoke selected **Nexus logical capabilities** (`nexus.*`) through the **daemon** — parallel to, not replacing, preset orchestration  
**Coordinates with**:

- [acp-capability-set.md](acp-capability-set.md) — full logical capability catalog (mostly deferred DF-46)
- [agent-host.md](agent-host.md) — Managed-only host, mediation invariants
- [orchestration-engine.md](orchestration-engine.md) — `worker/agent_tool_request` IPC
- [local-runtime-boundary.md](local-runtime-boundary.md) — CLI vs daemon vs Agent topology
- [creator-workflow-fl-e.md](creator-workflow-fl-e.md) — FL-E stages; Work read/patch tools

**Iteration compass**: [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md)

---

## 1. Purpose

Preset orchestration drives **multi-step** creative work via schedules and capabilities (`acp.prompt`, `judge.llm`, `creator.read_memory`, …). External LLM agents can also **actively** request Nexus context during a session (tool calls). V1.34 defines a **single mediated path** through the daemon so that:

- Permissions and workspace boundaries match Local API rules.
- Audit trails exist for agent-initiated mutations.
- The same handlers can serve HTTP tool execute and worker upcalls.

**Non-goals V1.34**: spawn `nexus42` per tool (DF-48); standalone MCP server (DF-49); full `acp-capability-set` implementation (DF-46).

---

## 2. Frozen decisions

| # | Decision |
| --- | --- |
| 1 | **Execution surface** = `nexus-daemon-runtime` `HostToolExecutor` + Local API handlers |
| 2 | **Tool IDs** = `nexus.<domain>.<action>` aligned with [acp-capability-set.md](acp-capability-set.md) where applicable |
| 3 | **CLI** is not an agent tool transport |
| 4 | **Orchestration capabilities** and **host tools** may share handler implementations (registry) but different admission paths |
| 5 | **`nexus.context.assemble`**: local/read-only or `policy_blocked` when platform paused (DF-55 for cloud) |

---

## 3. Topology

```text
[External ACP Agent]
        │ tool call (ACP wire)
        ▼
[nexus42 worker / nexus-acp-host]
        │ worker/agent_tool_request  OR  HTTP POST tool-executions
        ▼
[HostToolExecutor]
        │ permission.toml + workspace path rules + active creator
        ▼
[Handler registry → Local API / domain services]
```

Preset path (unchanged):

```text
[Schedule / Session] → capability (ep.acp.prompt) → worker IPC → Agent
```

The two paths **must not** share session secrets across creators (IDOR prevention per V1.32 `SEC-V131-01` pattern).

---

## 4. V1.34 minimal tool registry

| Tool ID | Access | Handler summary | FL-E relevance |
| --- | --- | --- | --- |
| `nexus.context.whoami` | R | Active `creator_id`, workspace slug | Agent session bootstrap |
| `nexus.workspace.info` | R | Roots, flags, linked world ref | Path policy |
| `nexus.work.get` | R | Work row + stage fields | Stage-aware context |
| `nexus.work.patch` | W | Append inspiration; optional stage metadata fields allowed by policy | Continue without CLI |
| `nexus.orchestration.schedule_status` | R | Schedules linked to `work_id` | Debug / agent planning |
| `nexus.context.assemble` | R | Local assemble-moment slice **or** `policy_blocked` | Context verification |

### 4.1 `nexus.context.assemble` (V1.34)

When `metadata.platform_integration` is paused:

- Return structured error: `policy_blocked` / `PLATFORM_PAUSED` (stable code in contracts).
- **Do not** call cloud HTTP from tool handler.

When local assemble-moment path is available:

- Return same shape as CLI `platform context assemble-moment` local subset (read-only).

Cloud/full platform assemble → **DF-55**.

### 4.2 Existing host tools (V1.33 baseline)

| Tool ID | Notes |
| --- | --- |
| `fs/read_text_file` | Workspace-bounded |
| `fs/write_text_file` | Workspace-bounded |

V1.34 registry **includes** fs tools and `nexus.*` in one dispatch table (P4).

---

## 5. Request / response contract (normative shape)

Wire JSON types live in `nexus-contracts` when codegen’d; until then:

**Execute request** (HTTP or IPC):

```json
{
  "tool_name": "nexus.work.get",
  "parameters": { "work_id": "wrk_..." },
  "session_id": "optional-acp-session"
}
```

**Success**:

```json
{
  "success": true,
  "result": { }
}
```

**Failure** (examples):

| Code | When |
| --- | --- |
| `FORBIDDEN` | Cross-creator, path outside workspace |
| `POLICY_BLOCKED` | Platform-only capability while paused |
| `NOT_SUPPORTED` | Unknown tool id (DF-46 surface) |
| `INVALID_INPUT` | Schema validation |

---

## 6. Permissions

1. Load `permissions.toml` under workspace `.nexus42/` when present (existing HostToolExecutor behavior).
2. Default-deny for `nexus.work.patch` if policy file exists and tool not granted.
3. All `nexus.*` tools require **active creator** context on `WorkspaceState` (same as `/v1/local/works`).

Audit: append row to tool audit log (existing ACP tool audit table pattern).

---

## 7. Worker upcall unification

`worker/agent_tool_request` parameters:

```json
{ "tool_name": "nexus.work.get", "args": { }, "request_id": "..." }
```

P4 **must** map `tool_name` through the same registry as `POST /v1/local/acp/tool/execute` and internal agent-host route.

If P4 completes unification, **do not** register DF-47. If partial, register DF-47 in deferred tracker.

---

## 8. Skills export (L1)

[skills-export-compatibility.md](skills-export-compatibility.md) maps logical `nexus.*` ids to host tools at **L1 Wrapped Skills**. V1.34 documents the minimal matrix in P3; full publish matrix remains **DF-50**.

---

## 9. Acceptance (spec-level)

1. Compass §2.3 tool list matches this §4 table.
2. No normative requirement for agents to invoke CLI subprocesses.
3. `policy_blocked` behavior for assemble documented and testable.
4. orchestration-engine cross-references §7 upcall unification.

---

*Normative agent tool bridge for V1.34. Implementation: P3 (spec/registry design), P4 (code).*
