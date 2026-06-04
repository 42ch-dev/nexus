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

| Tool ID | Access | Handler summary | FL-E relevance | Admission rule |
| --- | --- | --- | --- | --- |
| `nexus.context.whoami` | R | Active `creator_id`, workspace slug | Agent session bootstrap | Requires active creator; no path args; per-session read limit; audit `info`; callable by ACP agent + schedule |
| `nexus.workspace.info` | R | Roots, flags, linked world ref | Path policy | Requires active creator; only returns workspace-bounded roots and flags; per-session read limit; audit `info`; callable by ACP agent + schedule |
| `nexus.work.get` | R | Work row + stage fields | Stage-aware context | Requires active creator and Work ownership match; `work_id` must resolve inside current workspace/creator; per-work read limit; audit `info`; callable by ACP agent + schedule |
| `nexus.work.patch` | W | Append inspiration; optional stage metadata fields allowed by policy | Continue without CLI | Requires active creator and Work ownership match; no path traversal fields; policy must grant write; write-rate limited per Work; audit `write`; callable by ACP agent only unless schedule explicitly carries a write grant |
| `nexus.orchestration.schedule_status` | R | Schedules linked to `work_id` | Debug / agent planning | Requires active creator and Work ownership match; status scoped to linked schedules only; per-session read limit; audit `info`; callable by ACP agent + schedule |
| `nexus.context.assemble` | R | Local assemble-moment slice **or** `policy_blocked` | Context verification | Requires active creator; local/read-only assembly only; policy gate blocks platform-paused cloud path; expensive-read rate limit; audit `info` or `policy_blocked`; callable by ACP agent + schedule |

### 4.1 `nexus.context.assemble` (V1.34)

When `metadata.platform_integration` is paused:

- Return structured error: `POLICY_BLOCKED` with stable reason `PLATFORM_PAUSED`.
- **Do not** call cloud HTTP from tool handler.
- The handler may still return a local/read-only assemble-moment subset when the local assembler can satisfy the request without platform HTTP. If the requested shape requires platform-only data while paused, fail closed with `POLICY_BLOCKED` rather than returning partial cloud-shaped data.

When local assemble-moment path is available:

- Return same shape as CLI `platform context assemble-moment` local subset (read-only).

Cloud/full platform assemble → **DF-55**.

### 4.2 Existing host tools (V1.33 baseline)

| Tool ID | Notes |
| --- | --- |
| `fs/read_text_file` | Workspace-bounded |
| `fs/write_text_file` | Workspace-bounded |

V1.34 registry **includes** fs tools and `nexus.*` in one dispatch table (P4).

### 4.3 Admission pipeline

Every tool execution entrypoint — HTTP tool execute, internal agent-host route, and worker upcall — must pass through the same five gates before dispatching a handler:

1. **Tool id allowlist** — `tool_name` must match one of the six V1.34 `nexus.*` ids above or the two V1.33 `fs/*` baseline ids. Unknown ids fail with `NOT_SUPPORTED` and must not reach handler code.
2. **Creator active** — `WorkspaceState` must carry exactly one active creator context. Missing, inactive, or ambiguous creator state fails with `FORBIDDEN` for creator-scoped `nexus.*` tools.
3. **Workspace bounds** — any path-like parameter or entity lookup must remain inside the active workspace and active creator boundary. Cross-creator Work ids, schedule ids, or filesystem paths fail with `FORBIDDEN`.
4. **`permissions.toml` / policy** — when workspace `.nexus42/permissions.toml` exists, the requested tool id and access mode must be granted. Write tools default-deny if not granted. Policy-denied platform-only behavior uses `POLICY_BLOCKED` when the request is otherwise valid but currently paused by policy.
5. **Audit log** — record the decision and outcome before returning to the caller: tool id, caller kind, active creator, workspace slug/root hash, request id/session id, access class, allowed/denied, error code if any, and redacted parameter summary.

The pipeline is fail-closed. Later gates must not weaken earlier gate decisions, and handler code must not implement a second private admission path.

### 4.4 `nexus.work.patch` allowlist

`nexus.work.patch` is the only V1.34 `nexus.*` write tool. It exists for small, policy-mediated Work updates during an agent session, not for direct stage control.

Allowed patch fields:

- `title` — replace the Work title string after normal length/empty validation.
- `inspiration_log` — append-only entries; each entry must include agent-visible text plus optional source/correlation metadata. Replacement, deletion, or arbitrary JSON merge is not allowed.
- `stage_metadata` — update only policy-approved metadata keys that do **not** advance the FL-E state machine, such as `agent_notes`, `research_summary_ref`, `draft_outline_ref`, `review_summary_ref`, and `last_agent_tool_request_id`.

Rejected examples:

- `current_stage`, `stage`, `stage_status`, `stage_started_at`, or `stage_completed_at` — direct stage mutation is forbidden; use the stage-advance Local API / CLI path defined by [creator-workflow-fl-e.md](creator-workflow-fl-e.md).
- `creator_id`, `workspace_id`, `work_id`, or ownership fields — cross-creator reassignment is forbidden.
- `run_intents`, schedule rows, preset ids, or capability grants — preset routing remains under orchestration policy, not an agent patch.
- Manuscript/body replacement fields outside the `inspiration_log` append surface — full content persistence remains outside P3/P4 minimal tool scope.

Invalid or rejected fields fail with `INVALID_INPUT` when malformed and `FORBIDDEN` when they would bypass creator/workspace/stage policy.

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

Cross-reference with [orchestration-engine.md](orchestration-engine.md) §6.4: `worker/agent_tool_request` carries `{ tool_name, args, request_id }` from the worker to the daemon, and **every** `tool_name` in the `nexus.*` namespace or V1.33 `fs/*` baseline must be admitted through this spec's registry. The worker upcall is an entrypoint into the registry, not a second registry.

`worker/agent_tool_request` parameters:

```json
{ "tool_name": "nexus.work.get", "args": { }, "request_id": "..." }
```

P4 **must** map `tool_name` through the same registry as `POST /v1/local/acp/tool/execute` and internal agent-host route.

### 7.1 Single dispatch table invariant

There is exactly one normative dispatch table for P4: `HostToolExecutor` owns tool id admission and handler lookup for all eight V1.34 ids (six `nexus.*` plus two `fs/*`). Whether the request arrives through daemon HTTP tool execute, the internal agent-host route, or `worker/agent_tool_request`, the entrypoint must normalize request fields into the same internal request shape and call the same dispatch table.

Required consequences:

- No duplicate `match tool_name` table in worker IPC handling.
- No CLI subprocess fallback for any `nexus.*` id.
- No schedule-only handler path that bypasses `permissions.toml`, active-creator checks, workspace bounds, or audit logging.
- If a handler is not in the table, every entrypoint returns `NOT_SUPPORTED` consistently.

### 7.2 Cross-creator isolation

The registry applies the V1.32 `SEC-V131-01` pattern: caller-supplied ids are never sufficient authorization. For every `nexus.*` request, the daemon must bind the request to the active creator context first, then verify referenced Works, schedules, workspace roots, and context assembly inputs belong to that same creator/workspace boundary.

Worker sessions must not cache or reuse tool grants across creators. A worker started for creator A cannot invoke `nexus.work.get`, `nexus.work.patch`, `nexus.orchestration.schedule_status`, or `nexus.context.assemble` against creator B's entities, even when it supplies a syntactically valid id. Cross-creator attempts fail with `FORBIDDEN` and produce an audit row.

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

## 10. Test vectors

### TV-3 — `nexus.context.assemble` policy-blocked while platform paused

Request:

```json
{
  "tool_name": "nexus.context.assemble",
  "parameters": { "work_id": "wrk_local_1", "requires_platform": true },
  "session_id": "acp_sess_1"
}
```

Workspace metadata has `platform_integration = "paused"` and the requested shape requires platform-only assembly.

Expected result:

```json
{
  "success": false,
  "error": {
    "code": "POLICY_BLOCKED",
    "reason": "PLATFORM_PAUSED"
  }
}
```

Required side effects: no platform HTTP attempt; audit row recorded with `audit_level = "policy_blocked"`.

---

*Normative agent tool bridge for V1.34. Implementation: P3 (spec/registry design), P4 (code).*
