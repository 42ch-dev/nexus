# Nexus Local Runtime Boundary

**Status**: Normative for retained wire/ACP boundaries; daemon host-process sections are historical (retired in v1.193 P2)  
**Document class**: Master  

## 0. Document position

> **Retired daemon host model (v1.193 P2).** The `nexus-daemon-runtime` crate and integrated `nexus42 daemon` process mode are deleted. Retained `/v1/daemon/*` wire families are served by the standalone TypeScript service, `apps/nexus-service`, over `nexus-core`; Electron owns the app-managed service lifecycle and is the desktop host since v1.192. An independently owned service may be attached without transferring ownership to the app. The Connect host is `nexus-runtime`, an independent bin of `apps/nexus42`, not a daemon mode. Current topology authority: [rust-core-service-boundary.md](rust-core-service-boundary.md); desktop lifecycle: [desktop-shell.md](desktop-shell.md). The original scope outline below and explicitly marked process sections remain historical, not setup instructions.

This document defines boundaries between:

- `nexus42` CLI（产品名 Nexus；见 [`cli-spec.md`](./cli-spec.md) §0.1）
- daemon runtime mode (single-binary `nexus42`)
- Nexus Daemon API / IPC
- ACP sessions
- Skills compatibility layer

It preserves the ACP client-only topology from nexus-platform `v1-spec/architecture.md` §6.2.1 and the single-binary daemon runtime boundary (see nexus-platform `v1-spec/adr/adr-026-single-binary-daemon-runtime-and-hybrid-agent-host.md`).

ACP Registry 默认上游索引与仓库入口见 [`registry-integration.md`](./registry-integration.md) §0.1。

Logical `nexus.*` capabilities are shared with platform-hosted creators; this document only defines the **local** runtime boundary, not a separate capability model.

---

## 1. Frozen topology recap

> **Historical topology.** The daemon process and CLI↔daemon link below are retired. The ACP client-only invariant survives; current hosts are identified in §0.

| Component | ACP role | Notes |
| --- | --- | --- |
| User-owned agent | **ACP Agent** | Hosts tools/resources; executes model calls |
| Nexus CLI / runtime | **ACP Client** | Spawns/connects agent; negotiates capabilities |
| daemon runtime mode | None | Local supervisor; must not be advertised as ACP Agent |
| Nexus Daemon API | None | Loopback IPC for CLI↔daemon and automation |

---

## 2. Process model

> **Historical process composition (§2.1–§2.3).** The single-binary daemon mode and its managed-host placement below describe the deleted host, not the current CLI or Electron lifecycle. The retained CLI calls Rust core/cloud/Connect directly; the TS/native service composes the Rust execution and provider owners ([rust-core-service-boundary.md](rust-core-service-boundary.md) §§4–5).

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

## 3. Nexus Daemon API vs ACP

### 3.1 Why Daemon API exists

> **Historical motivation.** The CLI↔daemon use case below is no longer a supported CLI dependency; browser/Electron retain the HTTP wire families through the TS service.

ACP is for agent integration. Nexus still needs a stable internal interface for:

- CLI talking to daemon without spawning agents
- Local automation or IDE plugins that should not pretend to be ACP Agents

### 3.2 Daemon API characteristics

Here “Daemon API” names the retained wire namespace, not the retired Rust process. Exact current route identities and auth tiers come from `apps/nexus-service/src/routes.ts` and its family modules; the inventory below preserves its historical release annotations.

- Loopback-only by default
- Minimal surface: workspace status, daemon health, orchestration/agent-host, local KB/memory — **no** sync or platform registration proxy (see [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md))
- Auth: OS user boundary, with optional token / IPC artifacts under **`$HOME/.nexus42/run/`** (or workspace-scoped subpaths still rooted at `$HOME/.nexus42/`, never under `<workspace>/`)
- Versioned schema: all stable endpoints live under `/v1/daemon/*` so TS / Rust codegen can share one contract

### 3.2.1 Daemon API endpoint families

The retained Daemon API is the **codegen-ready** HTTP contract for browser/Electron clients of `apps/nexus-service`; it is not a CLI dependency or evidence that the deleted Rust daemon still runs.

**Current ownership:** `apps/nexus-service/src/routes.ts` composes the endpoint families and owns runtime health/status HTTP projections. `actors.ts`, `memory.ts`, `context.ts`, `presets.ts`, `works.ts`, and the World/knowledge family modules translate requests to the native facade over `nexus-core`; Rust owns stored-principal authorization and domain/storage effects. `execution.ts` and `workflow-observation.ts` expose the core execution owner, while `routes.ts`/`provider.ts` adapt the retained Agent-Host/provider surface. Current topology and qualification status: [rust-core-service-boundary.md](rust-core-service-boundary.md) §§4, 7.5.

> **Historical endpoint inventory.** The exact inventory below is retained from the Rust-host record. “Active” means active in that record, not that every legacy identity is mounted today. Retained identities are served only when registered in the TS route composer; old `api/mod.rs`/`orchestration_routes()` references and daemon restart wording below are historical. Do not infer current support from these annotations or restore retired `/v1/local/*` routes.

| Endpoint / family | Recorded status (historical Rust host) | Notes |
| --- | --- | --- |
| `GET /v1/daemon/runtime/health` | Active | Unguarded liveness route. |
| `GET /v1/daemon/runtime/status` | Active | Unguarded diagnostic route. |
| `GET /v1/daemon/daemon/status` | Active | Unguarded daemon lifecycle snapshot. |
| `GET /v1/daemon/workspace`, `POST /v1/daemon/workspace/init` | Active | Legacy single-workspace info/init routes. |
| `POST /v1/daemon/workspace/open`, `POST /v1/daemon/workspace/commit` | **Active (V1.56 P0)** | Workspace session open/commit with file-level OCC (SHA-256 content hash). Sessions persisted in `SQLite` `workspace_sessions` table; survive daemon restart; expire per TTL (default 5 min). `open` returns file hashes for all tracked files. `commit` validates `changes[]` manifest against session snapshot; rejects on hash mismatch (409 HASH_CONFLICT). See `concurrency.md` §OCC. |
| `GET|POST /v1/daemon/workspaces`, `GET|PUT /v1/daemon/workspaces/active` | Active | Workspace list/create and active workspace selection. |
| `GET /v1/daemon/creators`, `GET /v1/daemon/creators/{creator_id}`, `GET|PUT /v1/daemon/creators/active`, `POST /v1/daemon/creators/{creator_id}:logout` | Active | Local creator status/selection/logout only; registration remains CLI/cloud-line. |
| `GET /v1/daemon/references` | Active | Local reference list via `nexus-local-db`; not `nexus-knowledge` persistence. |
| `GET|POST /v1/daemon/kb/entries`, `GET|DELETE /v1/daemon/kb/entries/{entry_id}` | Active (`scope=work` only) | CLI local work KB file index; not World KB. See audit KCA-003 C2. |
| `GET /v1/daemon/memory/pending-review`, `GET /v1/daemon/memory/pending-review/count`, `DELETE /v1/daemon/memory/pending-review/{id}` | Active (consume-only) | Creator-memory pending review routes; the obsolete session-capture POST producer was removed in V1.186. |
| `GET|POST /v1/daemon/presets`, `POST /v1/daemon/presets:validate`, `POST /v1/daemon/presets/{id}:reload` | Active | Local preset management. |
| `/v1/daemon/orchestration/*` | Active | Sessions, capabilities, presets, schedules, core-context, history, and signal routes registered in `orchestration_routes()`. |
| `/v1/daemon/agent-host/*` | Active | Health, providers, sessions, operations, cancel, events SSE, and internal tool-executions routes. |
| `GET /v1/daemon/monitoring/pool` | Active | Protected monitoring route. |
| `POST /v1/local/context/assemble` | **Retired (KCA-002 B2; historical route spelling)** | Not registered in `api/mod.rs`; context assembly stays CLI in-process through `nexus-moment-context-assembly`, not Daemon API. |
| `GET /v1/local/research/sources` | **NotImplemented / Retired (historical route spelling)** | Not registered in `api/mod.rs`; do not list as active until a handler exists. |
| `POST /v1/local/research/scan` | **NotImplemented / Retired (historical route spelling)** | Not registered in `api/mod.rs`; do not list as active until a handler exists. |
| `POST /v1/local/agent-sessions/restart` | **Retired (historical route spelling)** | Not registered; shipped agent session control lives under `/v1/daemon/agent-host/*`. |
| `POST /v1/local/sync/push`, `POST /v1/local/sync/pull`, `POST /v1/local/sync/retry` | **Retired (historical route spelling)** | **Cloud line:** `nexus42 sync …` → `nexus-cloud-sync`; daemon sync routes removed in V1.21. |

The historical pre-V1.90 sketch expected each write-style endpoint to accept a small request envelope:

```json
{
  "request_id": "req_xxx",
  "workspace_id": "wrk_xxx",
  "actor": "cli"
}
```

The same historical sketch expected:

```json
{
  "request_id": "req_xxx",
  "success": true,
  "error_code": null,
  "details": {}
}
```

Current handlers and generated schemas are authoritative instead. In
particular, Daemon API failures use
`{ success: false, error: { code, message, details?, request_id? } }`, with
`request_id` nested inside `error`; success payloads remain route-specific.

Rules:

- `request_id` is caller-generated and traceable in logs
- `workspace_id` is mandatory for workspace-scoped actions
- `error_code` should align with sync / conflict schemas where applicable
- Research-specific routes may use the `/v1/daemon/*` namespace only after they are registered in the TS-service route composer.
- **V1.24 KCA-002 B2:** `POST /v1/local/context/assemble` is retired from the Daemon API. CLI/platform context assembly should call `nexus-moment-context-assembly` in-process rather than proxying through the daemon.
- **V1.2**：若请求体支持可选 **`as_of`**，Local 与 Platform HTTP **须**共享 **同一**字段语义与校验；不得仅在一侧出现私有历史参数。

### 3.3 Forbidden patterns

- Exposing Nexus Daemon API as a public ACP endpoint
- Implementing Nexus tools by re-entering ACP as Agent from daemon
- Shipping ad-hoc CLI-only request/response shapes that bypass the versioned Daemon API contract

### 3.4 Relationship diagram

> **Historical relationship diagram (retired host).** The daemon IPC/`DaemonClient` path below was deleted in v1.193 P2. Current CLI/core, browser/TS-service, and Connect boundaries are in [rust-core-service-boundary.md](rust-core-service-boundary.md).

```text
CLI --Daemon API--> daemon runtime mode --ACP Client--> ACP Agent
  |
  +-- sync / register / platform HTTP --> nexus-cloud-sync --> Platform HTTPS
  |
  +-- Registry fetch (CLI or daemon-local cache refresh)
  |
  +-- reference refresh (V1.58 P3):
       nexus42 creator reference refresh [ref_id|all]
         └─ DaemonClient::post
              └─ POST /v1/daemon/agent-host/internal/tool-executions
                   └─ HostToolExecutor::execute()
                        └─ admission_pipeline()
                             └─ CapabilityRegistry::dispatch("nexus.reference.refresh", ...)
                                  └─ ReferenceRefresh::run()
                                       └─ fetch URL → hash → update DB → atomic body.md write
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
- Behavior alignment: skills call Daemon API or CLI subprocess; they do not redefine semantics

### 5.3 Non-goals

- Skills are not a replacement for Registry + ACP handshake for ACP-capable agents
- Skills must not claim ACP Agent status for Nexus daemon

### 5.4 Export artifacts (retired V1.53)

V1.53 cancelled the skills-export CLI/spec line (DF-50). Nexus keeps the static committed skills model and runtime sync/link behavior, but this Master no longer defines an export/verify command contract.

---

## 6. Security considerations

### 6.1 Threat model highlights

- Local malicious agent may attempt filesystem exfiltration
- Local malicious process may talk to Daemon API if socket permissions are weak
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

> **Historical path table.** Daemon health through the CLI below is retired; current HTTP health belongs to `apps/nexus-service`, desktop lifecycle belongs to Electron, and the one-shot CLI does not launch or status that service.

| Action | Preferred path |
| --- | --- |
| Agent reasons & writes manuscript via tools | ACP session |
| User/script checks daemon health | Daemon API / CLI |
| Sync structured deltas to platform | `nexus42 sync …` → **`nexus-cloud-sync`** (CLI/cloud line; not Daemon API) |
| Discover agents | Registry integration |
| Non-ACP tool ecosystem | Skills -> CLI/Daemon API |

---

## 8. Open items

> **Historical open questions.** These are the original daemon-model questions, not an instruction to recreate that host. Current service transport/lifecycle authority is [rust-core-service-boundary.md §8.1](rust-core-service-boundary.md#81-independent-service-launch-and-attach).

- Whether loopback TCP is allowed on shared machines
- Multi-workspace daemon strategy vs one-daemon-multi-workspace
- Whether the frozen `/v1/daemon/*` envelope should be JSON-over-HTTP only or also mirrored on unix socket RPC

---

## V1.57 P1 Draft overlay: 3-caller adapter topology

> **Historical draft overlay (host retired in v1.193 P2).** The original V1.57 draft status and diagram below are preserved as history. `host-call`, daemon IPC, and the daemon-runtime registry placement are not current entrances; retained tool/execution authority is composed through the Rust core/native and TS-service owners.

**Status**: Draft (V1.57 P1)  

### Updated topology diagram

```text
┌─────────────────────────────────────────────────────────┐
│                   Caller entry points                    │
│  ┌──────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │CLI       │  │Worker        │  │HTTP               │  │
│  │host-call │  │agent_tool    │  │ToolExecuteRequest │  │
│  │<tool_id> │  │_request IPC  │  │POST /v1/daemon/... │  │
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
