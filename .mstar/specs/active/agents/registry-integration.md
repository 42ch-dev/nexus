# Nexus ACP Registry Integration

**Status**: Normative for the shipped behavior identified below
**Document class**: Master  

## 0. Document position

This document specifies **ACP agent discovery and selection**, registry caching and agent transport. It does **not** govern `nexus.*` host-tool dispatch or the `nexus.registry.refresh` tool: those belong to [capability-registry.md](capability-registry.md) and the retained core execution surface.

Related docs: nexus-platform `v1-spec/architecture.md` §6.4–6.5, [`cli-spec.md`](../cli/cli-spec.md) §6.8、§11。

**Current owners:** [`nexus-acp-host`](../../../../crates/nexus-acp-host/src/registry.rs) owns the Rust registry client/cache and ACP client/transport; [`nexus-agent-host::HostManager`](../../../../crates/nexus-agent-host/src/core/manager.rs) owns provider/session admission; [`packages/nexus-provider-acp`](../../../../packages/nexus-provider-acp/src/acp.ts) owns the TS service's ACP callback implementation and its agent subprocesses. [`apps/nexus-service/src/index.ts`](../../../../apps/nexus-service/src/index.ts) supplies those callbacks to the native core except in domain-only mode. Electron owns the local service lifecycle, not the ACP Registry.

### 0.1 Upstream registry (canonical source)

The **Agent Client Protocol registry** is maintained upstream as open data and tooling: agent listings, distribution metadata, and JSON schemas live in the community project **`agentclientprotocol/registry`**, with a published **HTTPS index** for clients to fetch. Nexus does not host that authority.

- **Shipped canonical endpoint:** `REGISTRY_URL` in [`registry.rs`](../../../../crates/nexus-acp-host/src/registry.rs) is `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`. The client parses the response as `RegistryManifest`; it does not currently negotiate upstream schema revisions.
- **Not implemented:** workspace overrides / enterprise mirrors, pinned index versions or ETags, and explicit upstream `FORMAT.md` / schema-revision compatibility are not current fetch guarantees.

Authentication expectations for agents are negotiated on the ACP connection; registry metadata is not proof of provider readiness or permission to execute.

---

## 1. Goals

- Treat Registry as the ecosystem layer for compatible agents.
- Make agent resolution deterministic, auditable, and offline-tolerant where possible.
- Keep **stdio** as the default for local agents; position remote transports as optional/future.

---

## 2. Roles and non-roles

### 2.1 Current runtime responsibilities

- `nexus-acp-host::RegistryClient` fetches the canonical index and reads/writes its local cache (§§3–4).
- The CLI resolves registry references and persists a workspace default agent; selection is the prefix/substring match described in §5.
- `HostManager` owns admitted provider recipes, session routing and readiness policy. Its normal boot catalog is built from explicit configuration and PATH discovery; [`core/readiness.rs`](../../../../crates/nexus-agent-host/src/core/readiness.rs) passes an empty registry-source list to `ProviderCatalog::build_from_sources`. The registry-to-catalog mapper is not evidence of an automatic remote fetch at service boot.
- Rust `AcpProvider` and the TS ACP provider own their agent connections/subprocesses; the core/native composition chooses the provider port. See [acp-client-tech-spec.md](acp-client-tech-spec.md) for that current composition.


### 2.2 Explicit non-responsibilities

- Publishing Nexus hosting infrastructure as an agent discoverable by third-party ACP clients.
- Acting as Registry hosting authority.

---

## 3. Manifest sources and fetch rules

### 3.1 Shipped source behavior

`RegistryClient::get_registry()` prefers a usable local cache. With no usable cache it fetches the fixed canonical HTTPS endpoint; it does not implement workspace override or mirror precedence.

### 3.2 Shipped fetch behavior

`fetch_from_cdn` / `fetch_and_save` perform a GET, require a successful HTTP status and parse JSON. The client has a 30-second request timeout; the background helper additionally bounds the send with 60 seconds. Cache-write failure does not fail a successful foreground fetch. Conditional ETag/If-Modified-Since requests and manifest signature/checksum verification are **not implemented** by this client.

### 3.3 Shipped offline behavior

A stale cache is returned immediately even when background refresh fails; 24 hours is a freshness threshold, not an offline-use expiry. No usable cache plus a failed fetch returns an error. A usable cache without readable metadata is returned without scheduling a refresh. This is the `get_registry` implementation, not the original TTL-constrained fallback below.

## 4. Cache policy

**Shipped cache contract:** [`registry.rs`](../../../../crates/nexus-acp-host/src/registry.rs) stores `cache.json` (full registry response) and `cache_meta.json` (`fetched_at`, `registry_version`) under **`$HOME/.nexus42/registry/`**. Fresh cache (<24h) skips the network; stale cache (≥24h with metadata) is served immediately and triggers background refresh. `RegistryClient::refresh()` bypasses freshness; CLI registry probing calls it. There is no separate icon/package/version-list cache or config/doctor invalidation mechanism in this client.

## 5. Selection and filtering rules

**Shipped selection:** `RegistryClient::find_agent` returns the first case-insensitive ID/name **prefix** match in registry order, falling back to the first substring match. It does not rank exact IDs ahead of earlier prefix matches and does not implement hard-filter/soft-scoring ranking. [`apps/nexus42/src/commands/acp/mod.rs`](../../../../apps/nexus42/src/commands/acp/mod.rs) implements `agent use` as a validated reference persisted to `$HOME/.nexus42/creators/<creator_id>/workspaces/<slug>/acp-default-agent.toml`; it does not fetch or compatibility-check the agent at pin time. `registry inspect` displays the matched entry, not filter pass/fail reasoning.

## 6. Transports: stdio vs remote

**Current local transport/ownership:** the CLI resolves NPX first, otherwise a binary recipe for the current platform, and errors if neither is usable (`resolve_launch_command`). [`nexus-acp-host/src/transport.rs`](../../../../crates/nexus-acp-host/src/transport.rs) spawns the stdio agent and owns its process teardown; [`nexus-agent-host/src/providers/acp.rs`](../../../../crates/nexus-agent-host/src/providers/acp.rs) binds an owned process to each Rust Host session. For the TS service path, [`process-owner.ts`](../../../../packages/nexus-provider-acp/src/process-owner.ts) spawns the admitted recipe and connects `ClientSideConnection` over the child's pipes.

### 6.1 Default: JSON-RPC over stdio

- Nexus client spawns agent subprocess and speaks JSON-RPC over stdin/stdout.
- Logs should go to stderr with a structured policy, not stdout framing.

### 6.2 HTTP / WebSocket (not shipped)

- Supported only when explicitly enabled by user policy.
- Require TLS and policy gating.

### 6.3 Transport selection algorithm (not shipped)

1. If pinned agent specifies transport, honor it if allowed.
2. Else prefer `stdio` local launch when manifest provides launch spec.
3. Else use remote endpoint if permitted.
4. Else fail with explicit configuration error.

---

## 7. CLI commands

The current CLI `RegistryCommand` has `list` and `inspect`; `nexus42 acp probe --registry` invokes `RegistryClient::refresh()`.

`nexus42 acp probe` should be reusable by creator registration flows to capture declared capabilities and transport metadata before the platform issues creator credentials.

---

## 8. Security considerations

- **Supply chain**: prefer signed manifests when available.
- **Typosquatting**: show publisher identity prominently.
- **Remote agents**: higher risk; default-off or strict allowlist.
- **Privacy**: Registry fetch leaks coarse usage timing; provide optional offline mode.

---

## 9. Open items

> **Durable roadmap:** the open items below (pin upstream schema compat, enterprise mirror, binary distribution) are DR-55.

- Pin field-level compatibility to upstream **`registry.schema.json` / `agent.schema.json`** revisions as they ship in the **`agentclientprotocol/registry`** project (repo URL and default index in §0.1 below).
- Enterprise mirror documentation and trust roots.
- Binary agent distribution vs path-only local agents.
