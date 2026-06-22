# Nexus Local Runtime Boundary

**Status**: Normative  
**Document class**: Master  

## 0. Document position

This document defines boundaries between:

- `nexus42` CLI（产品名 Nexus；见 [`cli-spec.md`](./cli-spec.md) §0.1）
- daemon runtime mode (single-binary `nexus42`)
- Nexus Local API / IPC
- ACP sessions
- Skills compatibility layer

It preserves the ACP client-only topology from nexus-platform `v1-spec/architecture.md` §6.2.1 and the single-binary daemon runtime boundary (see nexus-platform `v1-spec/adr/adr-026-single-binary-daemon-runtime-and-hybrid-agent-host.md`).

ACP Registry 默认上游索引与仓库入口见 [`registry-integration.md`](./registry-integration.md) §0.1、[`references-learnings.md`](../../references-learnings.md) §0.1。

Logical `nexus.*` capabilities are shared with platform-hosted creators; this document only defines the **local** runtime boundary, not a separate capability model.

---

## 1. Frozen topology recap

| Component | ACP role | Notes |
| --- | --- | --- |
| User-owned agent | **ACP Agent** | Hosts tools/resources; executes model calls |
| Nexus CLI / runtime | **ACP Client** | Spawns/connects agent; negotiates capabilities |
| daemon runtime mode | None | Local supervisor; must not be advertised as ACP Agent |
| Nexus Local API | None | Loopback IPC for CLI↔daemon and automation |

---

## 2. Process model

### 2.1 One-shot CLI

Examples: `auth`, `doctor`, `sync pull`, `config`

- May start a short-lived internal client for a single operation
- Should not require daemon for basic commands unless long-lived state is needed

### 2.2 Daemon runtime mode (`nexus42` internal process mode)

Owns:

- Workspace-scoped SQLite open handles
- Long-lived agent session supervision
- Local IPC listener

Does **not** own platform sync or registration (see [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md)).

Does not own:

- Human confirmation UX for destructive actions

### 2.3 Managed `nexus-agent-host` (Hybrid)

Daemon runtime hosts agent sessions through a managed host subsystem with these constraints:

1. **Managed-only**: no unmanaged attach mode in v1 scope.
2. **Hybrid providers**: ACP-backed and native CLI-backed adapters are both allowed.
3. **Normalized capability contract**: runtime dispatches via host capability contract, not provider-specific protocol shape.
4. **ACP role invariant**: host integration does not make daemon runtime an ACP Agent/Server.

---

## 3. Nexus Local API vs ACP

### 3.1 Why Local API exists

ACP is for agent integration. Nexus still needs a stable internal interface for:

- CLI talking to daemon without spawning agents
- Local automation or IDE plugins that should not pretend to be ACP Agents

### 3.2 Local API characteristics

- Loopback-only by default
- Minimal surface: workspace status, daemon health, orchestration/agent-host, local KB/memory — **no** sync or platform registration proxy (see [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md))
- Auth: OS user boundary, with optional token / IPC artifacts under **`$HOME/.nexus42/run/`** (or workspace-scoped subpaths still rooted at `$HOME/.nexus42/`, never under `<workspace>/`)
- Versioned schema: all stable endpoints live under `/v1/local/*` so TS / Rust codegen can share one contract

### 3.2.1 Local API endpoint families

The Local API is the **codegen-ready** internal contract between CLI, daemon, and local automation.

**Routing policy (long-term):** [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md) §5. **Removal acceptance:** [v1.21 delivery compass](../../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md).

| Endpoint / family | Status on daemon | Notes |
| --- | --- | --- |
| `GET /v1/local/runtime/health` | Active | Unguarded liveness route. |
| `GET /v1/local/runtime/status` | Active | Unguarded diagnostic route. |
| `GET /v1/local/daemon/status` | Active | Unguarded daemon lifecycle snapshot. |
| `GET /v1/local/workspace`, `POST /v1/local/workspace/init` | Active | Legacy single-workspace info/init routes. |
| `POST /v1/local/workspace/open`, `POST /v1/local/workspace/commit` | **Active (V1.56 P0)** | Workspace session open/commit with file-level OCC (SHA-256 content hash). Sessions persisted in `SQLite` `workspace_sessions` table; survive daemon restart; expire per TTL (default 5 min). `open` returns file hashes for all tracked files. `commit` validates `changes[]` manifest against session snapshot; rejects on hash mismatch (409 HASH_CONFLICT). See `concurrency.md` §OCC. |
| `GET|POST /v1/local/workspaces`, `GET|PUT /v1/local/workspaces/active` | Active | Workspace list/create and active workspace selection. |
| `GET /v1/local/creators`, `GET /v1/local/creators/{creator_id}`, `GET|PUT /v1/local/creators/active`, `POST /v1/local/creators/{creator_id}:logout` | Active | Local creator status/selection/logout only; registration remains CLI/cloud-line. |
| `GET /v1/local/references` | Active | Local reference list via `nexus-local-db`; not `nexus-knowledge` persistence. |
| `GET|POST /v1/local/kb/entries`, `GET|DELETE /v1/local/kb/entries/{entry_id}` | Active (`scope=work` only) | CLI local work KB file index; not World KB (`nexus-kb`). See audit KCA-003 C2. |
| `GET|POST /v1/local/memory/pending-review`, `GET /v1/local/memory/pending-review/count`, `DELETE /v1/local/memory/pending-review/{id}` | Active | Creator-memory pending review routes. |
| `GET|POST /v1/local/presets`, `POST /v1/local/presets:validate`, `POST /v1/local/presets/{id}:reload` | Active | Local preset management. |
| `/v1/local/orchestration/*` | Active | Sessions, capabilities, presets, schedules, core-context, history, and signal routes registered in `orchestration_routes()`. |
| `/v1/local/agent-host/*` | Active | Health, providers, sessions, operations, cancel, events SSE, and internal tool-executions routes. |
| `GET /v1/local/monitoring/pool` | Active | Protected monitoring route. |
| `POST /v1/local/context/assemble` | **Retired (KCA-002 B2)** | Not registered in `api/mod.rs`; context assembly stays CLI in-process through `nexus-moment-context-assembly`, not daemon Local API. |
| `GET /v1/local/research/sources` | **NotImplemented / Retired from active table** | Not registered in `api/mod.rs`; do not list as active until a handler exists. |
| `POST /v1/local/research/scan` | **NotImplemented / Retired from active table** | Not registered in `api/mod.rs`; do not list as active until a handler exists. |
| `POST /v1/local/agent-sessions/restart` | **Retired** | Not registered; shipped agent session control lives under `/v1/local/agent-host/*`. |
| `POST /v1/local/sync/push`, `POST /v1/local/sync/pull`, `POST /v1/local/sync/retry` | **Retired** | **Cloud line:** `nexus42 sync …` → `nexus-cloud-sync`; daemon sync routes removed in V1.21. |

Each write-style endpoint should accept a small request envelope:

```json
{
  "request_id": "req_xxx",
  "workspace_id": "wrk_xxx",
  "actor": "cli"
}
```

Each response should return:

```json
{
  "request_id": "req_xxx",
  "success": true,
  "error_code": null,
  "details": {}
}
```

Rules:

- `request_id` is caller-generated and traceable in logs
- `workspace_id` is mandatory for workspace-scoped actions
- `error_code` should align with sync / conflict schemas where applicable
- Research-specific routes may use the `/v1/local/*` namespace only after they are registered in the daemon router.
- **V1.24 KCA-002 B2:** `POST /v1/local/context/assemble` is retired from the daemon Local API. CLI/platform context assembly should call `nexus-moment-context-assembly` in-process rather than proxying through the daemon.
- **V1.2**：若请求体支持可选 **`as_of`**，Local 与 Platform HTTP **须**共享 **同一**字段语义与校验；不得仅在一侧出现私有历史参数。

### 3.3 Forbidden patterns

- Exposing Nexus Local API as a public ACP endpoint
- Implementing Nexus tools by re-entering ACP as Agent from daemon
- Shipping ad-hoc CLI-only request/response shapes that bypass the versioned Local API contract

### 3.4 Relationship diagram

```text
CLI --Local API--> daemon runtime mode --ACP Client--> ACP Agent
  |
  +-- sync / register / platform HTTP --> nexus-cloud-sync --> Platform HTTPS
  |
  +-- Registry fetch (CLI or daemon-local cache refresh)
```

---

## 4. Data and secret boundaries

### 4.1 Secrets

- Refresh/access tokens should use a credential store when possible
- Logs must redact tokens

### 4.2 Filesystem

- Manuscript access goes through whitelist enforcement inside runtime services backing ACP tools
- **`$HOME/.nexus42/`** contains operational data; agents should not be given blanket read access
- When `output_manuscript=false`, runtime may skip manuscript file creation while still allowing structured deltas and Story summaries to flow

### 4.3 SQLite

- Local working copy, outbox, session metadata
- Not a substitute for platform graph authority

---

## 5. Skills mapping (`ACP-first`, `skills-second`)

### 5.1 Purpose

Skills packages let ecosystems without ACP call Nexus operations through their native tools/skills model.

### 5.2 Mapping rules

- 1:1 name alignment: skill IDs mirror `nexus.*` capability IDs wherever possible
- Version alignment: skill manifest embeds `nexus.acp_contract_version`
- Behavior alignment: skills call Local API or CLI subprocess; they do not redefine semantics

### 5.3 Non-goals

- Skills are not a replacement for Registry + ACP handshake for ACP-capable agents
- Skills must not claim ACP Agent status for Nexus daemon

### 5.4 Export artifacts (retired V1.53)

V1.53 cancelled the skills-export CLI/spec line (DF-50). Nexus keeps the static committed skills model and runtime sync/link behavior, but this Master no longer defines an export/verify command contract.

---

## 6. Security considerations

### 6.1 Threat model highlights

- Local malicious agent may attempt filesystem exfiltration
- Local malicious process may talk to Local API if socket permissions are weak
- Remote agent transport expands attack surface

### 6.2 Controls

- Path sandbox for manuscript tools
- Explicit confirmations for publish/fork/destructive resets
- No silent full manuscript upload
- Idempotent sync with outbox and user-visible conflict reporting
- Degraded modes instead of silent failure/success

### 6.3 Observability

- `trace.correlation` links agent tool invocations, sync attempts, and daemon events
- `nexus42 debug dump-workspace` produces a support bundle with redaction rules

---

## 7. Operational boundaries

| Action | Preferred path |
| --- | --- |
| Agent reasons & writes manuscript via tools | ACP session |
| User/script checks daemon health | Local API / CLI |
| Sync structured deltas to platform | `nexus42 sync …` → **`nexus-cloud-sync`** (CLI/cloud line; not daemon Local API) |
| Discover agents | Registry integration |
| Non-ACP tool ecosystem | Skills -> CLI/Local API |

---

## 8. Open items

- Whether loopback TCP is allowed on shared machines
- Multi-workspace daemon strategy vs one-daemon-multi-workspace
- Whether the frozen `/v1/local/*` envelope should be JSON-over-HTTP only or also mirrored on unix socket RPC

---

## V1.57 P1 Draft overlay: 3-caller adapter topology

**Status**: Draft (V1.57 P1)  
**Plan**: `2026-06-22-v1.57-daemon-refactor-and-caller-adapters`

### Updated topology diagram

```text
┌─────────────────────────────────────────────────────────┐
│                   Caller entry points                    │
│  ┌──────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │CLI       │  │Worker        │  │HTTP               │  │
│  │host-call │  │agent_tool    │  │ToolExecuteRequest │  │
│  │<tool_id> │  │_request IPC  │  │POST /v1/local/... │  │
│  └────┬─────┘  └──────┬───────┘  └────────┬──────────┘  │
│       │               │                   │              │
│       ▼               ▼                   ▼              │
│  ┌─────────────────────────────────────────────────────┐ │
│  │         HostToolExecutor (3-caller adapter)         │ │
│  │  normalize → admission_pipeline (5 gates)           │ │
│  │           → CapabilityRegistry::dispatch(tool_id)   │ │
│  │           → audit_tool_execution                    │ │
│  └──────────────────────┬──────────────────────────────┘ │
│                         │                                 │
│                         ▼                                 │
│  ┌─────────────────────────────────────────────────────┐ │
│  │       CapabilityRegistry (in daemon-runtime)         │ │
│  │  20 registered host tools: nexus.* + fs/*            │ │
│  │  Handler bindings → host_tool_handlers               │ │
│  └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### Notes

- **3 caller entry points**: CLI subcommand (`host-call`), worker IPC
  (`agent_tool_request`), HTTP POST (`ToolExecuteRequest`). All normalize
  to the same internal shape and dispatch through a single registry.
- **Single dispatch invariant**: All three paths call
  `CapabilityRegistry::dispatch(tool_id, input)`. No alternate execution
  paths bypass admission gating or audit logging.
- **`host-call` subcommand** (V1.57 P1): Debug-only CLI entry.
  `nexus42 host-call <tool_id> --args <json>` → daemon IPC → registry dispatch.
- **CdnConfig** (V1.57 P1): Constructor-injected; no global `RwLock`.
