---
module: crates/nexus-core, nexus-acp-host, rmcp
date: 2026-08-25
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-08-24-v1.174-p0-peer-tools-transport-registry
tags:
  - mcp
  - rmcp
  - serverhandler
  - tools-only
  - visibility
  - embedded-server
  - transport-neutral
  - catalog
last_updated: 2026-10-12
applies_when:
  - "Exposing a nexus tool registry to an external MCP client over rmcp"
  - "Choosing the process model for an MCP server (client-spawned child vs in-process service)"
  - "Implementing an rmcp ServerHandler with a dynamic, backend-supplied tool catalog"
  - "Mapping non-MCP error vocabularies onto MCP protocol vs tool-result errors"
---

# Transport-neutral MCP bridge core + embedded Model B server (lockstep by construction)

## Context

V1.174 P0 asked who spawns the MCP server that exposes nexus's tool registry
to external MCP clients. The delivered answer was **Model A — an MCP client
spawns `nexus42 mcp serve` as its own stdio child** (AR-71). That entry is now
**historical**: the whole `nexus42 daemon` group, hidden `daemon-run`, and the
Model A `mcp serve` child were deleted in **v1.193 P2** (see
`.mstar/specs/active/cli/cli-spec.md` §V1.193 delivered), and the
`nexus-acp-host::mcp::nexus_mcp_stdio_server()` ACP descriptor factory that
bound it to ACP was deleted earlier in **v1.193 P1-T6**. No `nexus42` command
spawns an MCP server any more, and `apps/nexus42/src/` has no `mcp` command
group.

What survives — and what this note documents — is the **transport-neutral core
library** under `nexus_core::connect`, compiled behind the `connect-client` /
`embedded-mcp` cohort features:

- the **shared rmcp bridge core** (`crates/nexus-core/src/connect/mcp_bridge.rs`)
  — a `ServerHandler` generic over a host-supplied `McpBackend`; and
- the **embedded Model B server shell**
  (`crates/nexus-core/src/connect/mcp_embedded.rs`) — an in-process rmcp
  instance over a sink/stream transport.

The retired daemon supplied the concrete `WorkspaceState`-backed backend;
native/TS consumers compose the shell directly. A core-side regression must
supply its own `McpBackend` — see `crates/nexus-core/tests/retained_peer_contracts.rs`
(the embedded-only half of that target uses a `PeerRegistryBackend`).

## Guidance

### 1. The bridge core is generic over a backend, not a transport

`McpBridgeHandler<B: McpBackend>` owns no catalog, allowlist, policy, or cache
of its own; it delegates both directions to the backend:

- `list_tools()` → the backend's **live** catalog rows (`CatalogRow`);
- `call_tool(name, params)` → the backend executes one tool through the spine
  and maps the outcome into the `ToolCallOutcome` vocabulary.

**Lockstep by construction:** the handler never caches, so `tools/list` cannot
drift from the backend's live source. Model A's backend was loopback HTTP
(`GET /v1/daemon/tools`, `POST …/tool-executions`); Model B's backend is
in-process function calls (DF-88). The handler is identical for both — only the
backend differs.

- **Per-consumer visibility seam** (V1.180 P1, RN-OGA-2): an optional
  `VisibilityPolicy` filters `tools/list` and short-circuits a hidden-tool
  `tools/call` at the seam **before** the backend, minting the distinct
  `tool_not_authorized` refusal (`METHOD_NOT_FOUND`), separate from unroutable
  (the spine cannot resolve the id at all). Absent policy ⇒ byte-identical
  unfiltered behavior.
- **Tools-only boundary by absence:** the handler overrides only
  `get_info` / `list_tools` / `call_tool`; it advertises `enable_tools()` (plus
  `tools.listChanged` only when the backend delivers it — §2). The
  `prompts/*` / `resources/*` families stay at rmcp's defaults (§3).

### 2. Embedded Model B server shell (`embedded-mcp` feature)

`mcp_embedded` was moved out of the (now-deleted) daemon in **v1.190 P4-T3**;
it is transport-neutral and generic over `McpBackend` via the shared bridge. A
host composition supplies the catalog/call backend and a shutdown signal; the
session budget, sink/stream session establishment, and the rmcp lifecycle live
in the shell.

- **Session bound:** a documented exemption from the remote WS accept path's
  `PeerSessionManager::max_sessions` (AR-67). An in-process consumer presents
  no remote dial surface, so it must not consume peer-session counters. The
  shell carries its own compile-time bound `EMBEDDED_MCP_MAX_SESSIONS` (= 4);
  the (N+1)-th concurrent establish is refused with the honest discriminator
  `embedded_mcp_session_limit`. The budget is **process-global** — every server
  handle in the process shares one registry, so a second instance cannot bypass
  it. A slot is held by the session's server-side serve task for the whole
  connection lifetime (not by the client-side `EmbeddedSession` handle).
- **Lifecycle:** each session's serve task selects on the owner's shutdown
  signal, so live sessions end with the run that spawned them and release their
  slots. `establish` refuses with `EmbeddedMcpError::Shutdown` once the signal
  is set.
- **`listChanged`:** the shell runs **no** catalog watcher, so a backend over it
  leaves `advertise_tool_list_changed()` at the default `false` and the handler
  must not advertise a capability it never delivers. Model A's child-side 2 s
  poll watch (DF-90 / AR-79) is **historical** — deleted with the child.

Exports (`nexus_core::connect`): `start_embedded_mcp_server`,
`EmbeddedMcpServer`, `EmbeddedSession`, `EmbeddedShutdown`, `EmbeddedMcpError`,
`EMBEDDED_MCP_MAX_SESSIONS`.

### 3. rmcp realities (source-verified against the pin)

- **Pin:** `rmcp = { version = "=3.5.1", default-features = false, features =
  ["server", "client"] }`, optional, behind `connect-client` (which
  `embedded-mcp` composes). Feature-gated — exactly one rmcp per shipped graph
  (see `.mstar/knowledge/conventions/graph-pin-honesty-discipline.md`). The
  shipped Connect-host product does **not** enable the rmcp-bearing cohort
  (`tooling/check-graph-pins.sh` asserts rmcp absent for `connect-host`).
- **Runtime-dynamic handler.** `ServerHandler`'s `list_tools` / `call_tool` are
  plain async trait methods — the handler builds the tool list at call time
  from the backend. The `#[tool_handler]` macro router is optional and
  static-only; a backend-supplied catalog maps natively. `Tool.input_schema` is
  an `Arc<JsonObject>` constructed directly (no schemars on this surface).
- **Default lists are empty, not errors.** rmcp's default `prompts/list` and
  `resources/list` return `Ok(default())` — empty result lists — and the
  service layer does not gate by advertised capabilities. The tools-only
  boundary is enforced by **absence** of overrides; unroutable `prompts/get` /
  `resources/read` remain `METHOD_NOT_FOUND`.
- **Two-class error mapping** through the rmcp `CallToolResponse` union: the
  handler's `call_tool` returns `Result<CallToolResponse, McpError>` and
  produces **only** the `Complete(CallToolResult)` variant:
  - *Unroutable* ⇒ `Err(McpError)`: `METHOD_NOT_FOUND` for
    never-admitted / evicted / non-exposable ids, plus the `tool_not_authorized`
    discriminator for a policy-hidden id; `INTERNAL_ERROR` (`internal_error`)
    for a backend transport failure.
  - *Executed-but-failed* ⇒ `Ok(Complete(CallToolResult::error(...)))` naming
    the spine code, with the lowercase `wire_code` preserved verbatim when the
    spine supplies one (peer deny). Success ⇒ text content plus
    `structured_content` when the result is a JSON object, else text-only.
  - `is_unroutable(code, wire_code)` encodes the typed split: `not_supported`
    **without** a `wire_code` is unroutable; `not_supported` **with** a wire
    code is a peer deny (executed-but-failed) — matched structurally, never by
    message text.

### 4. Schema mapping (descriptor rows → rmcp `Tool`)

The catalog gates live in `crates/nexus-core/src/connect/table.rs`
(`mcp_catalog_admission`, `mcp_catalog_output_root_object`):

| Source | `input_schema` | `output_schema` |
|--------|----------------|-----------------|
| Peer tools (descriptor) | carried **verbatim** after the catalog root-object filter (`mcp_catalog: input_schema not root-object` is the named refusal) | included iff present **and** root-object; else omitted — never invented, never wrapped |
| Builtin `nexus.*` | authored draft-2020-12 schema on the registry row (`CatalogDescriptor`, DF-89); named `$comment: nexus42:schema-pending` fallback only for a hypothetical schema-less row (`SCHEMA_REMAINDER_LEDGER`, currently empty) | pinned where the success shape is a stable object; omitted otherwise |

No schema is ever synthesized from a non-schema source. See
`.mstar/knowledge/architecture-patterns/descriptive-first-builtin-schema-authoring.md`.

### 5. Process-model alternatives (mixed)

- **Model A — client-spawned stdio child** (the Model A `mcp serve` entry):
  the V1.174 delivered design; **historical, deleted v1.193 P2**. Its stateless
  child held no registry / allowlist / policy / cache and made live daemon HTTP
  round trips, with stdout as the JSON-RPC channel and stderr-only logging;
  none of that surface exists now.
- **Model B — in-process embedded rmcp service** over a sink/stream transport:
  **retained** and relocated to the core library (§2), generic over
  `McpBackend`.
- **Model C — host spawns and owns the child:** structurally incoherent for
  stdio (the stdio parent must be the MCP client) → rejected without a roadmap
  row.
- **Streamable HTTP + remote consumers:** a different trust class (network
  exposure) → DF-87, never "leftover stdio work".

### 6. ACP generic `mcp_servers` descriptors (retained)

ACP sessions still carry arbitrary `mcp_servers` descriptors (`Stdio` / `Http`
/ `Sse`) on `newSession`; `crates/nexus-acp-host/src/client.rs` maps the nexus
DTO to the SDK types verbatim (unit-pinned by
`new_session_request_propagates_mcp_servers`), and
`apps/nexus42/tests/mcp_acp_probe.rs` pins the generic round trip. The nexus
DTO → SDK mapping must carry `args` — a hand-built `NexusMcpServer::Stdio`
entry that drops args spawns a command without its subcommand. What is gone is
the nexus-owned `nexus` descriptor factory, not the generic descriptor path.

## Why This Matters

The bridge core is the entire MCP surface nexus exposes to external consumers.
If it cached a catalog or held policy, it would be a second authorization
domain and a drift surface. Keeping it generic over a host-supplied backend
moves every decision to that backend (where admission and dispatch live) and
makes `tools/list` ⇄ live-source lockstep **true by construction** instead of
by test. The early rmcp corrections (dynamic handler, empty-list defaults)
matter because they were documented wrongly once and cost a spec-correction
round; the empty-lists reality is the honest boundary for a tools-only server.

## When to Apply

- Building any MCP server whose catalog is a live source (never a cached
  snapshot).
- Choosing between a child process and an in-process service — ask first who
  owns the stdio/transport channel.
- Mapping a domain error vocabulary onto MCP: unroutable ⇒ protocol error,
  executed-but-failed ⇒ `is_error` result; keep the split typed, not
  text-matched.
- Extending the exposure lane: reuse this bridge core's backend contract and
  the `VisibilityPolicy` seam rather than forking a second handler.

## Examples

- `crates/nexus-core/src/connect/mcp_bridge.rs` — `McpBackend`,
  `McpBridgeHandler`, `map_outcome`, `is_unroutable`, `CatalogRow`.
- `crates/nexus-core/src/connect/mcp_embedded.rs` — embedded Model B shell
  (`start_embedded_mcp_server`, session bound, shutdown).
- `crates/nexus-core/src/connect/table.rs` — catalog admission / output-root
  gates.
- `crates/nexus-core/src/connect/visibility.rs` — `VisibilityPolicy` seam.
- Proof suites: `crates/nexus-core/tests/retained_peer_contracts.rs`
  (embedded-Model-B half behind `embedded-mcp`),
  `apps/nexus42/tests/mcp_acp_probe.rs` (generic ACP descriptor round trip).
- Consumer docs: `docs/mcp-server.md` (retained-contract pointer); the
  retired Model A bridge record is
  `.mstar/specs/archived/architecture/2026-10-13-mcp-server-historical-notes.md`.
- Companion: `architecture-patterns/peer-tool-registration-exposure-lanes.md`
  (the spine + admission the bridge proxies).
