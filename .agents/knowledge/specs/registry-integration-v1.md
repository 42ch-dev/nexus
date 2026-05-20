# Nexus ACP Registry Integration v1

## 0. Document position

This document specifies how **Nexus CLI runtime (ACP Client)** integrates with **ACP Registry** for agent discovery, selection, caching, and transport choice.

Related docs: [`architecture.md`](../architecture.md) §6.4–6.5, [`cli-spec-v1.md`](./cli-spec-v1.md) §6.8、§11。

### 0.1 Upstream registry (canonical source)

The **Agent Client Protocol registry** is maintained upstream as open data and tooling: agent listings, distribution metadata, and JSON schemas live in the community project **`agentclientprotocol/registry`**, with a published **HTTPS index** for clients to fetch. Nexus does not host that authority.

- **Concrete URLs, GitHub repo, and schema links** (only allowed external HTTP index for `v1-spec`): [`references-learnings.md`](../../references-learnings.md) §0.1 — rows **ACP Registry** / **分发用索引**.
- **Default remote canonical source**: implementations SHOULD use the upstream **registry index JSON** as the default for §3.1 item 1 unless a workspace override or enterprise mirror is configured. Entry shape and fields MUST follow upstream **`FORMAT.md`** and **`registry.schema.json` / `agent.schema.json`** as versioned by that repo; Nexus MAY pin a specific index version or ETag once the fetch contract is stable.

Authentication expectations for listed agents (e.g. `authMethods` in handshake) are defined upstream; see that repo’s **`AUTHENTICATION.md`** via the same references index.

---

## 1. Goals

- Treat Registry as the ecosystem layer for compatible agents.
- Make agent resolution deterministic, auditable, and offline-tolerant where possible.
- Keep **stdio** as V1.0 default for local agents; position remote transports as optional/future.

---

## 2. Roles and non-roles

### 2.1 Nexus runtime responsibilities

- Fetch or read Registry manifests and local mirrors.
- Validate agent entries against ACP protocol version, transport support, and Nexus capability requirements.
- Persist a local selection.
- Spawn/connect the selected ACP Agent as the ACP server side of the session from Nexus client perspective.
- Support capability probing for **Creator registration** and pairing flows when a local agent is being promoted into a platform-known Creator.

### 2.2 Explicit non-responsibilities

- Publishing Nexus daemon as an agent discoverable by third-party ACP clients.
- Acting as Registry hosting authority.

---

## 3. Manifest sources and fetch rules

### 3.1 Source priority

1. Remote canonical Registry (HTTPS)
2. Workspace override
3. User cache
4. Local static mirror

### 3.2 Fetch behavior

- Use ETag / If-Modified-Since when available.
- Failures must not crash daemon; enter degraded mode.
- Verify manifest signatures/checksums when Registry provides them.

### 3.3 Offline behavior

- If remote fetch fails, runtime uses last good cached manifest if within TTL.
- If no cache exists, runtime requires local override or manual agent configuration.

---

## 4. Cache policy

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

### 6.1 V1.0 default: JSON-RPC over stdio

- Nexus client spawns agent subprocess and speaks JSON-RPC over stdin/stdout.
- Process supervision and restart/backoff are owned by daemon.
- Logs should go to stderr with a structured policy, not stdout framing.

### 6.2 Future-compatible: HTTP / WebSocket

- Supported only when explicitly enabled by user policy.
- Require TLS and policy gating.

### 6.3 Transport selection algorithm

1. If pinned agent specifies transport, honor it if allowed.
2. Else prefer `stdio` local launch when manifest provides launch spec.
3. Else use remote endpoint if permitted.
4. Else fail with explicit configuration error.

---

## 7. CLI commands

Minimum recommended:

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

- Pin field-level compatibility to upstream **`registry.schema.json` / `agent.schema.json`** revisions as they ship in the **`agentclientprotocol/registry`** project (repo URL and default index in [`references-learnings.md`](../../references-learnings.md) §0.1).
- Enterprise mirror documentation and trust roots.
- Binary agent distribution vs path-only local agents.
