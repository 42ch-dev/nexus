# Nexus Local Runtime Boundary

**Status**: Normative — retained `/v1/daemon/*` wire families and ACP boundaries; topology authority: [rust-core-service-boundary.md](rust-core-service-boundary.md)  
**Document class**: Master  

## 0. Document position

> **Host model.** The `nexus-daemon-runtime` crate and integrated `nexus42 daemon` process mode are deleted. Retained `/v1/daemon/*` wire families are served by the standalone TypeScript service, `apps/nexus-service`, over `nexus-core`; Electron owns the app-managed service lifecycle and is the desktop host since v1.192. An independently owned service may be attached without transferring ownership to the app. The Connect host is `nexus-runtime`, an independent bin of `apps/nexus42`, not a daemon mode. Current topology authority: [rust-core-service-boundary.md](rust-core-service-boundary.md); desktop lifecycle: [desktop-shell.md](../surfaces/desktop-shell.md).

This document defines boundaries between:

- `nexus42` CLI（产品名 Nexus；见 [`cli-spec.md`](../cli/cli-spec.md) §0.1）
- Nexus Daemon API / IPC
- ACP sessions
- Skills compatibility layer

It preserves the ACP client-only topology from nexus-platform `v1-spec/architecture.md` §6.2.1.

ACP Registry 默认上游索引与仓库入口见 [`registry-integration.md`](../agents/registry-integration.md) §0.1。

Logical `nexus.*` capabilities are shared with platform-hosted creators; this document only defines the **local** runtime boundary, not a separate capability model.

---

## 1. Frozen topology recap

| Component | ACP role | Notes |
| --- | --- | --- |
| User-owned agent | **ACP Agent** | Hosts tools/resources; executes model calls |
| Nexus CLI / runtime | **ACP Client** | Spawns/connects agent; negotiates capabilities |
| Nexus Daemon API | None | Retained loopback HTTP wire namespace for browser/Electron clients and automation (§3.2.1) |

---

## 2. Process model

### 2.1 One-shot CLI

Examples: `auth`, `doctor`, `sync pull`, `config`

- May start a short-lived internal client for a single operation
- Should not require daemon for basic commands unless long-lived state is needed

## 3. Nexus Daemon API vs ACP

### 3.1 Why Daemon API exists

ACP is for agent integration. The Daemon API is the stable internal interface for:

- Browser and desktop-shell clients of `apps/nexus-service` talking to the local runtime without spawning agents
- Local automation or IDE plugins that should not pretend to be ACP Agents

### 3.2 Daemon API characteristics

Here “Daemon API” names the retained wire namespace, not the retired Rust process. Exact current route identities and auth tiers come from `apps/nexus-service/src/routes.ts` and its family modules.

- Loopback-only by default
- Minimal surface: workspace status, daemon health, orchestration/agent-host, local KB/memory — **no** sync or platform registration proxy (cloud line stays CLI-only, [cli-spec.md](../cli/cli-spec.md) §6.5; historical record: [local-cloud-crate-architecture.md](../../archived/architecture/2026-09-21-local-cloud-crate-architecture.md))
- Auth: OS user boundary, with optional token / IPC artifacts under **`$HOME/.nexus42/run/`** (or workspace-scoped subpaths still rooted at `$HOME/.nexus42/`, never under `<workspace>/`)
- Versioned schema: all stable endpoints live under `/v1/daemon/*` so TS / Rust codegen can share one contract

### 3.2.1 Daemon API endpoint families

The retained Daemon API is the **codegen-ready** HTTP contract for browser/Electron clients of `apps/nexus-service`; it is not a CLI dependency or evidence that the deleted Rust daemon still runs.

**Current ownership:** `apps/nexus-service/src/routes.ts` composes the endpoint families and owns runtime health/status HTTP projections. `actors.ts`, `memory.ts`, `context.ts`, `presets.ts`, `works.ts`, and the World/knowledge family modules translate requests to the native facade over `nexus-core`; Rust owns stored-principal authorization and domain/storage effects. `execution.ts` and `workflow-observation.ts` expose the core execution owner, while `routes.ts`/`provider.ts` adapt the retained Agent-Host/provider surface. Current topology and qualification status: [rust-core-service-boundary.md](rust-core-service-boundary.md) §§4, 7.5.

Current handlers and generated schemas are authoritative instead. In
particular, Daemon API failures use
`{ success: false, error: { code, message, details?, request_id? } }`, with
`request_id` nested inside `error`; success payloads remain route-specific.

Rules:

- `request_id` is caller-generated and traceable in logs
- `workspace_id` is mandatory for workspace-scoped actions
- `error_code` should align with sync / conflict schemas where applicable
- Research-specific routes may use the `/v1/daemon/*` namespace only after they are registered in the TS-service route composer.
- CLI/platform context assembly calls `nexus-moment-context-assembly` in-process rather than proxying through the Daemon API.
- **V1.2**：若请求体支持可选 **`as_of`**，Local 与 Platform HTTP **须**共享 **同一**字段语义与校验；不得仅在一侧出现私有历史参数。

### 3.3 Forbidden patterns

- Exposing Nexus Daemon API as a public ACP endpoint
- Implementing Nexus tools by re-entering ACP as Agent from daemon
- Shipping ad-hoc CLI-only request/response shapes that bypass the versioned Daemon API contract

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

### 5.4 Export artifacts

The skills-export CLI/spec line is cancelled (DF-50). Nexus keeps the static committed skills model and runtime sync/link behavior; this Master defines no export/verify command contract.

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

| Action | Preferred path |
| --- | --- |
| Agent reasons & writes manuscript via tools | ACP session |
| User/script checks service health | `apps/nexus-service` HTTP health routes |
| Sync structured deltas to platform | `nexus42 sync …` → **`nexus-cloud-sync`** (CLI/cloud line; not Daemon API) |
| Discover agents | Registry integration |
| Non-ACP tool ecosystem | Skills → CLI / Daemon API wire |

---
