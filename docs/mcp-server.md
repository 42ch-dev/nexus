# MCP & peer-tool surface

Nexus exposes MCP as a **tools-only** surface over the single capability-registry / dispatch spine. The transport-neutral implementation lives in the core library — `nexus_core::connect` behind the `connect-client` / `embedded-mcp` features (peer tool registry, shared rmcp bridge core, embedded Model B server, `VisibilityPolicy`, peer-control lane) — and is composed by the native/TS consumers. No `nexus42` command spawns an MCP server. (The removed Model A `nexus42 mcp serve` stdio bridge is recorded in [`../.mstar/specs/archived/architecture/2026-10-13-mcp-server-historical-notes.md`](../.mstar/specs/archived/architecture/2026-10-13-mcp-server-historical-notes.md).) See [`.mstar/specs/active/architecture/rust-core-service-boundary.md`](../.mstar/specs/active/architecture/rust-core-service-boundary.md) §3–§4 and [`ARCHITECTURE.md`](ARCHITECTURE.md).

## Integrator & operator duties (retained contract)

The peer-admission rules below are enforced by the transport-neutral core library (`nexus_core::connect::config` / `watch` / `visibility`). Read "daemon" below as "the process hosting the Connect runtime".

### Both sides name the same exact tool id

A peer tool is admitted only when **both** ends name the **same exact
`tools.<ns>.<id>` string** — there is no fuzzy matching, no namespace-level
grants, and no "any tool from this peer" umbrella:

- **Integrator (spoke `RemoteAdapter` dial side):** the hello manifest must
  list each tool id it wants to serve in `capabilities[]` **and** its owning
  namespace in `namespaces[]`. `spoke-operations::validate_manifest_tools`
  enforces this: `op == capability_id`, id ∈ `capabilities[]`, derived ns ∈
  `namespaces[]`, unique across the manifest — a violation fails the whole
  manifest with `INVALID_INPUT` (zero ingestion, session stays).
- **Operator (daemon side):** the allowlist in
  `~/.nexus42/connect/daemon.json` (`tool_allowlist`, plus the dialer
  handshake allowlist `peer_ids` and `peer_keys.json`) names the **same
  exact id**. Entry validation at config load rejects umbrellas
  (`tools`, `tools.*`, `tools.<ns>`), the reserved `tools.nexus.*` namespace,
  and malformed ids with a named `InvalidAllowlist` error — the whole config
  load fails rather than silently dropping an entry. A missing/empty
  `tool_allowlist` is default-deny: zero peer tools admitted.

Admission then intersects the two sides: negotiated = integrator
`capabilities[]` ∩ daemon hello capabilities (which derive **only** from the
operator allowlist, AR-69) ∩ operator allowlist. An id missing on either
side is never admitted. The MCP catalog mirrors what the daemon spine can
actually dispatch — a never-admitted id is absent from `tools/list` and
refused on `tools/call` (`METHOD_NOT_FOUND`).

### Allowlist edits apply on (host) restart

`~/.nexus42/connect/daemon.json` is read **once** at daemon boot (AR-67/AR-69). Edits to `tool_allowlist`, `peer_ids`, `peer_keys.json`,
or `max_sessions` take effect on the **next daemon restart** — never
mid-session, and never on a live MCP `tools/list` (the child re-lists every
time, but the daemon's allowlist snapshot is fixed for the process
lifetime). Runtime reload is a tracked roadmap item (DF-92), not current
behavior.

### One catalog: builtin `nexus.*` rows are always present (PL-5)

The MCP catalog is the daemon's **full dispatchable registry as one
catalog** — builtin `nexus.*` host tools are always listed, plus admitted
user capabilities and admitted peer tools. An
operator configuring peer tools should expect the builtin rows to be
present **even when no peer is connected**; this is by design (one catalog,
PL-5), not leakage.

### Builtin schemas are real draft-2020-12 (DF-89)

Each catalog row's `input_schema` is a **real draft-2020-12 JSON Schema
string** (root `"type":"object"`) authored on the registry
`CatalogDescriptor`. A row that cannot yet carry a real input
schema would emit the **named** placeholder
`{"type":"object","$comment":"nexus42:schema-pending"}` and be listed on
`SCHEMA_REMAINDER_LEDGER` — never a silent generic object. `output_schema`
is present when the success shape is a stable object; omitted otherwise.
Peer schemas stay verbatim; user-cap schema behavior is unchanged.
Schemas are **descriptive**: the spine does not start rejecting
`tools/call` arguments based on these strings.

### Tools-only vocabulary boundary (PL-7/PL-9)

The MCP surface is **tools-only**: it implements only the tools family (`tools/list`, `tools/call`) plus server
info; `prompts/list` / `resources/list` return empty lists and the
unroutable `prompts/get` / `resources/read` return `METHOD_NOT_FOUND`.
This is not a general-purpose MCP product and there is no marketplace
reopening. The origin vocabulary stays honest: builtin / user / peer rows
are labeled with their provenance on the catalog (PL-9); tools are never
re-labeled or re-scoped to fit the MCP lane.

### Native CLI config JSON (PL-10)

The native CLI `--mcp-config` story is **documented-only** (PL-10): nexus does not own native CLI spawn
configuration — the native-CLI providers (`ClaudeCliProvider`, `CodexNativeProvider`) register session state with no MCP flags, and the native-CLI descriptor honestly reports
`mcp_http` / `mcp_sse` / `mcp_stdio = false`, so MCP servers are not part of a
native-hosted session. Nexus is an ACP **client** that forwards generic
`newSession.mcp_servers` descriptors to the agent (owned by
[acp-client-tech-spec.md](../.mstar/specs/active/agents/acp-client-tech-spec.md) and
[agent-host.md](../.mstar/specs/active/agents/agent-host.md)); native MCP
services belong to the agent, and `rmcp` stays out of the shipped graph.

## Implementation ownership

The MCP/peer serving implementation is owned by `nexus-core` behind transport-neutral boundaries. Current facts:

- **Peer tool registry.** The process-level registry of admitted peer tools
  is `nexus_core::execution::peer_tools::peer_tool_registry()`. The
  connect stack admits into it and never keeps a second registry; collision
  policy, reserved tool ids and evictions are registry semantics.
- **Peer/MCP sessions, transport, watchers.** The WS transport
  (`WsTransport`, tokio-tungstenite, bounded envelope), the accept loop +
  `PeerSessionManager`, the config snapshot/watcher chain, peer identity
  loading, and the shared rmcp bridge core live in
  `nexus_core::connect` behind the `connect-client` feature.
- **Embedded MCP server (Model B).** The generic shell (process-global
  session budget `EMBEDDED_MCP_MAX_SESSIONS`, server-side budget-slot
  lifetime, watch-based shutdown gate) is
  `nexus_core::connect::mcp_embedded` behind the nested `embedded-mcp`
  feature. The fail-closed invalid-`mcp_visibility` construction refusal
  is unchanged.
- **Visibility policy.** `VisibilityPolicy` (RN-OGA-2) is
  evaluated at the shared MCP serving seam before `tools/list` filtering and
  before `tools/call` dispatch. Visibility is never an authorization grant:
  an absent policy stays byte-identical to the pre-seam behavior, and a
  hidden-tool call is refused at the seam before any peer invocation.
- **Peer control.** The execution owner admits one peer-control lane
  (`ExecutionHandle::start_peer_control` / `peer_control`): explicit
  enablement plus a named operation allowlist; operations off the allowlist
  are refused before any effect. The lane is composed by the native/TS consumers that host the runtime.
