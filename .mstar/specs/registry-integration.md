# Nexus ACP Registry Integration

**Status**: Normative for the shipped behavior identified below; original unshipped design is explicitly labeled
**Document class**: Master  

## 0. Document position

This document specifies **ACP agent discovery and selection**, registry caching and agent transport. It does **not** govern `nexus.*` host-tool dispatch or the `nexus.registry.refresh` tool: those belong to [capability-registry.md](capability-registry.md) and the retained core execution surface.

Related docs: nexus-platform `v1-spec/architecture.md` §6.4–6.5, [`cli-spec.md`](./cli-spec.md) §6.8、§11。

**Current owners (post-v1.193 P2; reconciled 2026-09-29):** [`nexus-acp-host`](../../crates/nexus-acp-host/src/registry.rs) owns the Rust registry client/cache and ACP client/transport; [`nexus-agent-host::HostManager`](../../crates/nexus-agent-host/src/core/manager.rs) owns provider/session admission; [`packages/nexus-provider-acp`](../../packages/nexus-provider-acp/src/acp.ts) owns the TS service's ACP callback implementation and its agent subprocesses. [`apps/nexus-service/src/index.ts`](../../apps/nexus-service/src/index.ts) supplies those callbacks to the native core except in domain-only mode. Electron owns the local service lifecycle, not the ACP Registry. The old daemon-owned supervision/restart/backoff model is **historical**: `nexus-daemon-runtime` and the integrated daemon were deleted in v1.193 P2.

### 0.1 Upstream registry (canonical source)

The **Agent Client Protocol registry** is maintained upstream as open data and tooling: agent listings, distribution metadata, and JSON schemas live in the community project **`agentclientprotocol/registry`**, with a published **HTTPS index** for clients to fetch. Nexus does not host that authority.

- **Shipped canonical endpoint:** `REGISTRY_URL` in [`registry.rs`](../../crates/nexus-acp-host/src/registry.rs) is `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`. The client parses the response as `RegistryManifest`; it does not currently negotiate upstream schema revisions.
- **Original design (not shipped as a source-selection policy):** workspace overrides / enterprise mirrors, pinned index versions or ETags, and explicit upstream `FORMAT.md` / schema-revision compatibility remain design considerations, not current fetch guarantees.

Authentication expectations for agents are negotiated on the ACP connection; registry metadata is not proof of provider readiness or permission to execute.

---

## 1. Goals

- Treat Registry as the ecosystem layer for compatible agents.
- Make agent resolution deterministic, auditable, and offline-tolerant where possible.
- Keep **stdio** as V1.0 default for local agents; position remote transports as optional/future.

---

## 2. Roles and non-roles

### 2.1 Current runtime responsibilities

- `nexus-acp-host::RegistryClient` fetches the canonical index and reads/writes its local cache (§§3–4).
- The CLI resolves registry references and persists a workspace default agent; selection is not the original compatibility/scoring algorithm (§5).
- `HostManager` owns admitted provider recipes, session routing and readiness policy. Its normal boot catalog is built from explicit configuration and PATH discovery; [`core/readiness.rs`](../../crates/nexus-agent-host/src/core/readiness.rs) passes an empty registry-source list to `ProviderCatalog::build_from_sources`. The registry-to-catalog mapper is not evidence of an automatic remote fetch at service boot.
- Rust `AcpProvider` and the TS ACP provider own their agent connections/subprocesses; the core/native composition chooses the provider port. See [acp-client-tech-spec.md](acp-client-tech-spec.md) for that current composition.

**Historical design scope:** generalized ACP-version/capability/platform filtering, creator-registration probing, and daemon-owned process supervision/restart/backoff were original integration goals. They must not be inferred from registry membership or treated as current daemon responsibilities.

### 2.2 Explicit non-responsibilities

- Publishing Nexus hosting infrastructure as an agent discoverable by third-party ACP clients (including the historical daemon).
- Acting as Registry hosting authority.

---

## 3. Manifest sources and fetch rules

### 3.1 Shipped source behavior

`RegistryClient::get_registry()` prefers a usable local cache. With no usable cache it fetches the fixed canonical HTTPS endpoint; it does not implement workspace override or mirror precedence.

**Original source-priority design (not shipped):**

1. Remote canonical Registry (HTTPS)
2. Workspace override
3. User cache
4. Local static mirror

### 3.2 Shipped fetch behavior

`fetch_from_cdn` / `fetch_and_save` perform a GET, require a successful HTTP status and parse JSON. The client has a 30-second request timeout; the background helper additionally bounds the send with 60 seconds. Cache-write failure does not fail a successful foreground fetch. Conditional ETag/If-Modified-Since requests and manifest signature/checksum verification are **not implemented** by this client.

**Original fetch design (not shipped guarantees; daemon wording historical):**

- Use ETag / If-Modified-Since when available.
- Failures must not crash daemon; enter degraded mode.
- Verify manifest signatures/checksums when Registry provides them.

### 3.3 Shipped offline behavior

A stale cache is returned immediately even when background refresh fails; 24 hours is a freshness threshold, not an offline-use expiry. No usable cache plus a failed fetch returns an error. A usable cache without readable metadata is returned without scheduling a refresh. This is the `get_registry` implementation, not the original TTL-constrained fallback below.

**Original offline design (superseded, not current behavior):**

- If remote fetch fails, runtime uses last good cached manifest if within TTL.
- If no cache exists, runtime requires local override or manual agent configuration.

---

## 4. Cache policy

**Shipped cache contract:** [`registry.rs`](../../crates/nexus-acp-host/src/registry.rs) stores `cache.json` (full registry response) and `cache_meta.json` (`fetched_at`, `registry_version`) under **`$HOME/.nexus42/registry/`**. Fresh cache (<24h) skips the network; stale cache (≥24h with metadata) is served immediately and triggers background refresh. `RegistryClient::refresh()` bypasses freshness; CLI registry probing calls it. There is no separate icon/package/version-list cache or config/doctor invalidation mechanism in this client.

**Historical design boundary (§§4.1–4.4):** the artifact matrix, invalidation triggers, proposed `registry refresh` CLI spelling and `cache/registry/` location below are the original design, **not the shipped cache contract**.

### 4.1 What is cached

- Registry manifest files
- Agent package metadata
- Small artifacts such as icons/descriptions

### 4.2 TTL and freshness

| Artifact | Default TTL | Notes |
| --- | --- | --- |
| Manifest index | 24h | refresh in background |
| Agent version list | 24h | pin overrides TTL |
| Downloaded agent package | explicit | only if Registry supports binary distribution |

Runtime may serve stale cache immediately while async refresh proceeds.

### 4.3 Cache invalidation triggers

- User runs `nexus42 acp registry refresh`
- TTL expiry + next online event
- Workspace config changes
- Doctor detects checksum mismatch vs pinned agent

### 4.4 Storage location

- Under `$HOME/.nexus42/cache/registry/` (or a workspace-keyed subtree still under `$HOME/.nexus42/cache/`)

---

## 5. Selection and filtering rules

**Shipped selection:** `RegistryClient::find_agent` returns the first case-insensitive ID/name **prefix** match in registry order, falling back to the first substring match. It does not rank exact IDs ahead of earlier prefix matches or implement the hard-filter/soft-scoring design below. [`apps/nexus42/src/commands/acp/mod.rs`](../../apps/nexus42/src/commands/acp/mod.rs) implements `agent use` as a validated reference persisted to `$HOME/.nexus42/creators/<creator_id>/workspaces/<slug>/acp-default-agent.toml`; it does not fetch or compatibility-check the agent at pin time. `registry inspect` displays the matched entry, not filter pass/fail reasoning.

**Historical design boundary (§§5.1–5.4):** the compatibility filters, scoring order, explanatory inspect output and fallback policy below are original design targets, **not shipped selection guarantees**. Explicit provider configuration and Host readiness admission are separate from this registry lookup.

### 5.1 Hard filters

- ACP protocol version outside supported window
- Transport unsupported for current OS / runtime mode
- Missing required Nexus capabilities for selected profile
- Platform policy flags

### 5.2 Soft scoring

Prefer, in order:

1. User pinned agent
2. Workspace default
3. Highest compatible version within same major line
4. Local stdio agents over remote agents
5. Recent successful agent

### 5.3 Explicit user binding

- `nexus42 acp agent use` writes pin record under `$HOME/.nexus42/` or workspace-linked config (not ad-hoc under `<workspace>/` without user intent)
- `nexus42 acp registry inspect` shows why an agent passed/failed filters

### 5.4 Fallback path

If Registry resolution fails:

- Allow manual agent command in config
- Doctor prints actionable fix steps

---

## 6. Transports: stdio vs remote

**Current local transport/ownership:** the CLI resolves NPX first, otherwise a binary recipe for the current platform, and errors if neither is usable (`resolve_launch_command`). [`nexus-acp-host/src/transport.rs`](../../crates/nexus-acp-host/src/transport.rs) spawns the stdio agent and owns its process teardown; [`nexus-agent-host/src/providers/acp.rs`](../../crates/nexus-agent-host/src/providers/acp.rs) binds an owned process to each Rust Host session. For the TS service path, [`process-owner.ts`](../../packages/nexus-provider-acp/src/process-owner.ts) spawns the admitted recipe and connects `ClientSideConnection` over the child's pipes. Neither composition uses the retired daemon supervisor or establishes a daemon restart/backoff contract.

### 6.1 V1.0 default: JSON-RPC over stdio

- Nexus client spawns agent subprocess and speaks JSON-RPC over stdin/stdout.
- **Historical ownership claim (superseded v1.193 P2):** “Process supervision and restart/backoff are owned by daemon.” Current owners are listed above; no such daemon exists.
- Logs should go to stderr with a structured policy, not stdout framing.

### 6.2 HTTP / WebSocket (unshipped design)

- Supported only when explicitly enabled by user policy.
- Require TLS and policy gating.

### 6.3 Transport selection algorithm (historical design; not shipped)

1. If pinned agent specifies transport, honor it if allowed.
2. Else prefer `stdio` local launch when manifest provides launch spec.
3. Else use remote endpoint if permitted.
4. Else fail with explicit configuration error.

---

## 7. CLI commands

Minimum recommended:

**Historical recommendation, not an executable command inventory:** in the current CLI `RegistryCommand` has `list` and `inspect`, not `refresh`; `nexus42 acp probe --registry` invokes `RegistryClient::refresh()`. The list below retains the original recommendation.

- `nexus42 acp registry list`
- `nexus42 acp registry inspect <agent>`
- `nexus42 acp registry refresh`
- `nexus42 acp agent use <agent>`
- `nexus42 acp probe`

`nexus42 acp probe` should be reusable by creator registration flows to capture declared capabilities and transport metadata before the platform issues creator credentials.

---

## 8. Security considerations

- **Supply chain**: prefer signed manifests when available.
- **Typosquatting**: show publisher identity prominently.
- **Remote agents**: higher risk; default-off or strict allowlist in v1.
- **Privacy**: Registry fetch leaks coarse usage timing; provide optional offline mode.

---

## 9. Open items

> **Durable roadmap:** the open items below (pin upstream schema compat, enterprise mirror, binary distribution) are DR-55.

- Pin field-level compatibility to upstream **`registry.schema.json` / `agent.schema.json`** revisions as they ship in the **`agentclientprotocol/registry`** project (repo URL and default index in §0.1 below).
- Enterprise mirror documentation and trust roots.
- Binary agent distribution vs path-only local agents.
