# Nexus Local Runtime Boundary

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

| Endpoint | Status on daemon | Notes |
| --- | --- | --- |
| `GET /v1/local/runtime/health` | Active | |
| `GET /v1/local/workspaces/{workspace_id}/status` | Active | |
| `POST /v1/local/context/assemble` | Active | Wire-aligned with platform context assembly (§3.2.1 rules below) |
| `GET /v1/local/research/sources` | Active | |
| `POST /v1/local/research/scan` | Active | |
| `POST /v1/local/agent-sessions/restart` | Active | Prefer agent-host namespace where shipped |
| `POST /v1/local/sync/push` | **Retired (target)** | **Cloud line:** `nexus42 sync push` → `nexus-cloud-sync`. Legacy daemon route until V1.21 removal. |
| `POST /v1/local/sync/pull` | **Retired (target)** | **Cloud line:** `nexus42 sync pull` |
| `POST /v1/local/sync/retry` | **Retired (target)** | **Cloud line:** `nexus42 sync retry` |

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
- Context assembly and research-specific routes stay under the same `/v1/local/*` namespace
- **C4 Closed（V1.0）**：`POST /v1/local/context/assemble` 的 request/response **必须与** `ContextAssembleRequestV1` / `ContextAssembleResponseV1` 字段级同构（不允许 local 子集或私有别名字段），见 `shared/schema/context-assembly-wire-v1.md` §3.1。
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

### 5.4 Export artifacts

`nexus42 skills export` should emit:

- manifest
- tool definitions
- docs for required user confirmations

`nexus42 skills verify` checks schema + version consistency.

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
