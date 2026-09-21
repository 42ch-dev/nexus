# Nexus MCP Server (retired in v1.193 P2)

> **Historical record — nothing below is a current setup instruction.** The
> Model A `nexus42 mcp serve` stdio bridge, its
> `nexus-acp-host::mcp::nexus_mcp_stdio_server()` ACP factory (v1.193 P1-T6)
> and the whole `nexus-daemon-runtime` host composition (v1.193 P2-T13) were
> deleted. No `nexus42` command spawns an MCP server any more. The retained
> MCP/peer surface is the transport-neutral core library
> (`nexus_core::connect` behind the `connect-client` / `embedded-mcp`
> features: peer tool registry, shared rmcp bridge core, embedded Model B
> server, `VisibilityPolicy`, peer-control lane), composed by the native/TS
> consumers. See
> [`.mstar/specs/rust-core-service-boundary.md`](../.mstar/specs/rust-core-service-boundary.md)
> §3–§4 and [`ARCHITECTURE.md`](ARCHITECTURE.md).

`nexus42 mcp serve` **was** a **tools-only** Model Context Protocol (MCP)
server over stdio (V1.174 P0-5 / P1, AR-70/71/72). An MCP client — Claude
Code, Codex, or a hosted ACP agent — spawned it as its **own stateless
child** (AR-71 Model A): the child kept no registry, no allowlist, no
policy, no cache. Every `tools/list` was a live
`GET /v1/daemon/tools` and every `tools/call` a live
`POST /v1/daemon/agent-host/internal/tool-executions` over the daemon's
loopback HTTP (config `daemon_url`, default `http://127.0.0.1:8420`).

The catalog mirrored the daemon's live tool registry as **one catalog**:
builtin `nexus.*` host tools, admitted user capabilities, and (with the
deleted app `connect-client` selector) admitted peer tools.

> **Historical prerequisites (no longer applicable).** The bridge required
> the `nexus42 daemon` host running and reachable at `daemon_url` (the
> stateless child failed bounded with `INTERNAL_ERROR` when it was down),
> and a `nexus42` binary built with the app `connect-client` feature that
> gated the `mcp` subcommand. Both that command group and that app selector
> are deleted.

## Claude Code (command-line config — retired entry)

Claude Code accepts a user-side `--mcp-config` JSON flag. The Nexus side of
this example is historical (the command no longer exists):

```sh
claude --mcp-config '{"mcpServers":{"nexus":{"command":"nexus42","args":["mcp","serve"]}}}'
```

## Codex (settings config — retired entry)

Codex has **no** `--mcp-config` CLI flag. Add the same
`mcpServers` entry to its settings (historical Nexus side, as above):

```json
{
  "mcpServers": {
    "nexus": {
      "command": "nexus42",
      "args": ["mcp", "serve"]
    }
  }
}
```

## Why this is documented-only (PL-10) — not an AC-V174-1 miss

Nexus does **not** own native CLI spawn configuration:

- `ClaudeCliProvider::launch()` (`crates/nexus-agent-host/src/providers/native_cli/claude.rs`)
  registers session state only and never passes a `--mcp-config` flag;
  the native-CLI descriptor (`native_cli_limited()`) honestly reports
  `mcp_http/mcp_sse/mcp_stdio = false`, so MCP servers are not part of a
  native-hosted session.
- `CodexNativeProvider` spawns `codex app-server` with no MCP flags.

The T0 consumption probe (AR-75 C-2) therefore recorded the native adapter
face as **documented-only**, and the verdict is **not** an
AC-V174-1 miss: AC-V174-1 closes on the **wired ACP path** below, not on
native CLI configuration (PL-10). No `structured_tool_calls` descriptor
flips were made on this surface.

## Wired consumption path for hosted ACP agents (retired)

**Retired in v1.193 P1-T6:** the T1 helper
`nexus-acp-host::mcp::nexus_mcp_stdio_server()` and the app `connect-client`
selector it needed were deleted with the Model A bridge. Historically it
produced
`McpServer::Stdio { name: "nexus", command: "<nexus42>", args: ["mcp", "serve"] }`
carried on `NewSessionRequest.mcp_servers` when the `connect-client`
feature was enabled, and the AC-closing journey was a scripted ACP agent
spawning the real `nexus42 mcp serve` child and calling an
integrator-registered peer tool through its own MCP client. Generic ACP
`mcp_servers` descriptors remain supported; the deleted factory is not the
way to build one.

## Integrator & operator duties (V1.174 P1, AC-V174-4)

> **Retained contract.** The peer-admission rules in this section are still
> enforced by the transport-neutral core library
> (`nexus_core::connect::config` / `watch` / `visibility`); only their
> historical Model A host (the daemon composition and the `nexus42 mcp serve`
> child) is gone. Read "daemon" below as "the process hosting the Connect
> runtime".

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

`~/.nexus42/connect/daemon.json` is read **once** at daemon boot (V1.174
P0, AR-67/AR-69). Edits to `tool_allowlist`, `peer_ids`, `peer_keys.json`,
or `max_sessions` take effect on the **next daemon restart** — never
mid-session, and never on a live MCP `tools/list` (the child re-lists every
time, but the daemon's allowlist snapshot is fixed for the process
lifetime). Runtime reload is a tracked roadmap item (DF-92), not current
behavior.

### One catalog: builtin `nexus.*` rows are always present (PL-5)

The MCP catalog is the daemon's **full dispatchable registry as one
catalog** — builtin `nexus.*` host tools are always listed, plus admitted
user capabilities and (with `connect-client`) admitted peer tools. An
operator configuring peer tools should expect the builtin rows to be
present **even when no peer is connected**; this is by design (one catalog,
PL-5), not leakage.

### Builtin schemas are real draft-2020-12 (V1.175 / DF-89)

Each catalog row's `input_schema` is a **real draft-2020-12 JSON Schema
string** (root `"type":"object"`) authored on the registry
`CatalogDescriptor`. The V1.174 permissive builtin placeholder
`{"type":"object"}` is gone. A row that cannot yet carry a real input
schema (none remain after V1.175 P0) would emit the **named** placeholder
`{"type":"object","$comment":"nexus42:schema-pending"}` and be listed on
`SCHEMA_REMAINDER_LEDGER` — never a silent generic object. `output_schema`
is present when the success shape is a stable object; omitted otherwise.
Peer schemas stay verbatim; user-cap schema behavior is unchanged.
Schemas are **descriptive**: the spine does not start rejecting
`tools/call` arguments based on these strings.

### Long-lived sessions see catalog changes (V1.175 / DF-90)

`nexus42 mcp serve` advertises `tools.listChanged`. A background watcher
polls `GET /v1/daemon/tools` every **2 s** (`MCP_CATALOG_WATCH_INTERVAL`,
not configurable this iteration), digests the response body, and sends
`notifications/tools/list_changed` when the digest changes between
successful polls. The first successful poll is a **baseline** (no
notification at session start). Poll errors keep the last digest, log to
stderr (once per error-state transition, never every 2 s during an
outage), and never notify. A failed `notify_tool_list_changed` also keeps
the previous digest, so the next successful poll **retries** the
notification — `listChanged` is idempotent (the client re-lists), so
duplicates are safe, loss is not.

This is a **child-side watch** (AR-79): the child holds a digest +
interval only — no registry, allowlist, policy, or read cache. There is
**no daemon→child push channel** and no new daemon route. Session-visible
change sources include peer admission/eviction and user-cap changes: the
daemon hot-reloads `~/.nexus42/capabilities/` into the live registry
(V1.176, RN-2), and the next successful poll observes the swap. The
end-to-end budget for a user-cap change to reach a live session is
**~1 s daemon watch (incl. the hot-reload rebuild, bounded by the caps
count) + ~2 s child watch + one HTTP request ≈ ≤ 4 s worst case** (both
legs named, AR-93); deleting `<name>/` drops the row on the same chain.
Notification latency ≤ 2 s + one HTTP request timeout (never unbounded). A
subsequent `tools/list` is still a live daemon round trip.

### Tools-only vocabulary boundary (PL-7/PL-9)

The retired `nexus42 mcp serve` child was a **tools-only** MCP surface: it
implemented only the tools family (`tools/list`, `tools/call`) plus server
info; `prompts/list` / `resources/list` returned empty lists and the
unroutable `prompts/get` / `resources/read` returned `METHOD_NOT_FOUND`.
This is not a general-purpose MCP product and there is no marketplace
reopening. The origin vocabulary stays honest: builtin / user / peer rows
are labeled with their provenance on the catalog (PL-9); tools are never
re-labeled or re-scoped to fit the MCP lane.

### Native CLI config JSON (per T0 verdict)

The native CLI `--mcp-config` story is **documented-only** (PL-10) — see
[Claude Code (command-line config — retired entry)](#claude-code-command-line-config--retired-entry)
and [Codex (settings config — retired entry)](#codex-settings-config--retired-entry)
above for the exact user-side JSON; nexus does not own native CLI spawn
configuration. A document-only native face is **not** an AC-V174-1 miss: the
acceptance journey closed on the (now retired) wired ACP path above.

## Implementation ownership (v1.190 P4-T3; host composition retired in v1.193 P2)

The MCP/peer serving implementation moved behind transport-neutral ownership
in `nexus-core`; the former daemon host kept only compositions and was
deleted in v1.193 P2. Current facts:

- **Peer tool registry.** The process-level registry of admitted peer tools
  is `nexus_core::execution::peer_tools::peer_tool_registry()` (P3). The
  connect stack admits into it and never keeps a second registry; collision
  policy, reserved tool ids and evictions are registry semantics.
- **Peer/MCP sessions, transport, watchers.** The WS transport
  (`WsTransport`, tokio-tungstenite, bounded envelope), the accept loop +
  `PeerSessionManager`, the config snapshot/watcher chain, peer identity
  loading, and the shared rmcp bridge core live in
  `nexus_core::connect` behind the `connect-client` feature. The old
  `crates/nexus-daemon-runtime/src/connect/` re-export shims went away with
  the crate.
- **Embedded MCP server (Model B).** The generic shell (process-global
  session budget `EMBEDDED_MCP_MAX_SESSIONS`, server-side budget-slot
  lifetime, watch-based shutdown gate) is
  `nexus_core::connect::mcp_embedded` behind the nested `embedded-mcp`
  feature. The fail-closed invalid-`mcp_visibility` construction refusal
  is unchanged.
- **Visibility policy.** `VisibilityPolicy` (V1.180 P1, RN-OGA-2) is
  evaluated at the shared MCP serving seam before `tools/list` filtering and
  before `tools/call` dispatch. Visibility is never an authorization grant:
  an absent policy stays byte-identical to the pre-seam behavior, and a
  hidden-tool call is refused at the seam before any peer invocation.
- **Peer control.** The execution owner admits one peer-control lane
  (`ExecutionHandle::start_peer_control` / `peer_control`): explicit
  enablement plus a named operation allowlist; operations off the allowlist
  are refused before any effect. The Model A child stdio CLI composition
  that used to expose this lane (v1.190 P6-T2) was deleted in v1.193 P1-T6;
  the lane is composed by the native/TS consumers that host the runtime.
