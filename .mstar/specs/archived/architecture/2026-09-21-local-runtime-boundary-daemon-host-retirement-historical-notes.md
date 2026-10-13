# Local Runtime Boundary — Daemon Host Retirement Historical Notes

**Provenance**: frozen from [`specs/active/architecture/local-runtime-boundary.md`](../../active/architecture/local-runtime-boundary.md) on 2026-10-13 during the **v1.216** de-historicization pass. 
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 (2026-09-21) deleted the `nexus-daemon-runtime` crate and integrated `nexus42 daemon` process mode; the frozen text is the daemon host-process sections (§2.2–§2.3), the historical endpoint inventory, the retired-topology diagrams, and the V1.57 draft overlay. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (172 lines). Each block is anchored to the line number the text occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Normative for retained wire/ACP boundaries; daemon host-process sections are historical (retired in v1.193 P2)  
```

### at pre-freeze line ~8

```markdown
> **Retired daemon host model (v1.193 P2).** The `nexus-daemon-runtime` crate and integrated `nexus42 daemon` process mode are deleted. Retained `/v1/daemon/*` wire families are served by the standalone TypeScript service, `apps/nexus-service`, over `nexus-core`; Electron owns the app-managed service lifecycle and is the desktop host since v1.192. An independently owned service may be attached without transferring ownership to the app. The Connect host is `nexus-runtime`, an independent bin of `apps/nexus42`, not a daemon mode. Current topology authority: [rust-core-service-boundary.md](../../active/architecture/rust-core-service-boundary.md); desktop lifecycle: [desktop-shell.md](../../active/surfaces/desktop-shell.md). The original scope outline below and explicitly marked process sections remain historical, not setup instructions.
```

### at pre-freeze line ~13

```markdown
- daemon runtime mode (single-binary `nexus42`)
```

### at pre-freeze line ~18

```markdown
It preserves the ACP client-only topology from nexus-platform `v1-spec/architecture.md` §6.2.1 and the single-binary daemon runtime boundary (see nexus-platform `v1-spec/adr/adr-026-single-binary-daemon-runtime-and-hybrid-agent-host.md`).
```

### at pre-freeze line ~28

```markdown
> **Historical topology.** The daemon process and CLI↔daemon link below are retired. The ACP client-only invariant survives; current hosts are identified in §0.

```

### at pre-freeze line ~34

```markdown
| daemon runtime mode | None | Local supervisor; must not be advertised as ACP Agent |
| Nexus Daemon API | None | Loopback IPC for CLI↔daemon and automation |
```

### at pre-freeze line ~41

```markdown
> **Historical process composition (§2.1–§2.3).** The single-binary daemon mode and its managed-host placement below describe the deleted host, not the current CLI or Electron lifecycle. The retained CLI calls Rust core/cloud/Connect directly; the TS/native service composes the Rust execution and provider owners ([rust-core-service-boundary.md](../../active/architecture/rust-core-service-boundary.md) §§4–5).

```

### at pre-freeze line ~50

```markdown
### 2.2 Daemon runtime mode (`nexus42` internal process mode)

Owns:

- Workspace-scoped SQLite open handles
- Long-lived agent session supervision
- Local IPC listener

Does **not** own platform sync or registration — that stays on the cloud line (CLI → `nexus-cloud-sync`, [cli-spec.md](../../active/cli/cli-spec.md) §6.5); historical split record: [local-cloud-crate-architecture.md](2026-09-21-local-cloud-crate-architecture.md).

Does not own:

- Human confirmation UX for destructive actions

### 2.3 Managed `nexus-agent-host` (Hybrid)

Daemon runtime hosts agent sessions through a managed host subsystem with these constraints:

1. **Managed-only**: no unmanaged attach mode in v1 scope.
2. **Hybrid providers**: ACP-backed and native CLI-backed adapters are both allowed.
3. **Normalized capability contract**: runtime dispatches via host capability contract, not provider-specific protocol shape.
4. **ACP role invariant**: host integration does not make daemon runtime an ACP Agent/Server.

---

```

### at pre-freeze line ~79

```markdown
> **Historical motivation.** The CLI↔daemon use case below is no longer a supported CLI dependency; browser/Electron retain the HTTP wire families through the TS service.

ACP is for agent integration. Nexus still needs a stable internal interface for:
```

### at pre-freeze line ~83

```markdown
- CLI talking to daemon without spawning agents
```

### at pre-freeze line ~88

```markdown
Here “Daemon API” names the retained wire namespace, not the retired Rust process. Exact current route identities and auth tiers come from `apps/nexus-service/src/routes.ts` and its family modules; the inventory below preserves its historical release annotations.
```

### at pre-freeze line ~101

````markdown
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

````

### at pre-freeze line ~157

```markdown
- **V1.24 KCA-002 B2:** `POST /v1/local/context/assemble` is retired from the Daemon API. CLI/platform context assembly should call `nexus-moment-context-assembly` in-process rather than proxying through the daemon.
```

### at pre-freeze line ~166

````markdown
### 3.4 Relationship diagram

> **Historical relationship diagram (retired host).** The daemon IPC/`DaemonClient` path below was deleted in v1.193 P2. Current CLI/core, browser/TS-service, and Connect boundaries are in [rust-core-service-boundary.md](../../active/architecture/rust-core-service-boundary.md).

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

````

### at pre-freeze line ~227

```markdown
### 5.4 Export artifacts (retired V1.53)
```

### at pre-freeze line ~229

```markdown
V1.53 cancelled the skills-export CLI/spec line (DF-50). Nexus keeps the static committed skills model and runtime sync/link behavior, but this Master no longer defines an export/verify command contract.
```

### at pre-freeze line ~258

```markdown
> **Historical path table.** Daemon health through the CLI below is retired; current HTTP health belongs to `apps/nexus-service`, desktop lifecycle belongs to Electron, and the one-shot CLI does not launch or status that service.

```

### at pre-freeze line ~263

```markdown
| User/script checks daemon health | Daemon API / CLI |
```

### at pre-freeze line ~266

```markdown
| Non-ACP tool ecosystem | Skills -> CLI/Daemon API |
```

### at pre-freeze line ~269

````markdown

## 8. Open items

> **Historical open questions.** These are the original daemon-model questions, not an instruction to recreate that host. Current service transport/lifecycle authority is [rust-core-service-boundary.md §8.1](../../active/architecture/rust-core-service-boundary.md#81-independent-service-launch-and-attach).

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
````
