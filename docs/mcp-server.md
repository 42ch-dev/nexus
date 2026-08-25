# Nexus MCP Server

`nexus42 mcp serve` is a **tools-only** Model Context Protocol (MCP)
server over stdio (V1.174 P0-5 / P1, AR-70/71/72). An MCP client — Claude
Code, Codex, or a hosted ACP agent — spawns it as its **own stateless
child** (AR-71 Model A): the child keeps no registry, no allowlist, no
policy, no cache. Every `tools/list` is a live
`GET /v1/daemon/tools` and every `tools/call` a live
`POST /v1/daemon/agent-host/internal/tool-executions` over the daemon's
loopback HTTP (config `daemon_url`, default `http://127.0.0.1:8420`).

The catalog mirrors the daemon's live tool registry as **one catalog**:
builtin `nexus.*` host tools, admitted user capabilities, and (with the
`connect-client` feature) admitted peer tools.

## Prerequisites

- The daemon must be running (`nexus42 daemon`) and reachable at
  `daemon_url`. The bridge is a stateless child — it has no data of its
  own and fails bounded (`INTERNAL_ERROR`) when the daemon is down.
- The `nexus42` binary must be built with the `connect-client` feature:
  the `mcp` subcommand is feature-gated
  (`#[cfg(feature = "connect-client")]`), same as the ACP client stack.

## Claude Code (command-line config)

Claude Code accepts a user-side `--mcp-config` JSON flag:

```sh
claude --mcp-config '{"mcpServers":{"nexus":{"command":"nexus42","args":["mcp","serve"]}}}'
```

## Codex (settings config)

Codex has **no** `--mcp-config` CLI flag. Add the same
`mcpServers` entry to its settings:

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

## Wired consumption path for hosted ACP agents

The shipped construction path for the ACP stdio surface is the T1 helper
`nexus-acp-host::mcp::nexus_mcp_stdio_server()`, which produces
`McpServer::Stdio { name: "nexus", command: "<nexus42>", args: ["mcp", "serve"] }`
carried on `NewSessionRequest.mcp_servers` when the `connect-client`
feature is enabled (S-b / QC1 S-2: no hosted-session caller constructs the
option directly yet — the helper is the single safe constructor, and the
Nexus→SDK mapping now carries `args` for hand-built Stdio descriptors as
well). This is the AC-closing journey — a scripted ACP agent spawns the
real `nexus42 mcp serve` child and calls an integrator-registered peer
tool through its own MCP client.

## Integrator & operator duties (V1.174 P1, AC-V174-4)

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

### Allowlist edits apply on daemon restart

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
stderr, and never notify.

This is a **child-side watch** (AR-79): the child holds a digest +
interval only — no registry, allowlist, policy, or read cache. There is
**no daemon→child push channel** and no new daemon route. Session-visible
change sources: peer admission, peer eviction, and any other catalog
content change the next successful poll observes (user-cap set changes
are restart-scoped today; RN-2). Notification latency ≤ 2 s + one HTTP
request timeout (never unbounded). A subsequent `tools/list` is still a
live daemon round trip.


### Tools-only vocabulary boundary (PL-7/PL-9)

`nexus42 mcp serve` is a **tools-only** MCP surface: it implements only
the tools family (`tools/list`, `tools/call`) plus server info;
`prompts/list` / `resources/list` return empty lists and the unroutable
`prompts/get` / `resources/read` return `METHOD_NOT_FOUND`. This is not a
general-purpose MCP product and there is no marketplace reopening. The
origin vocabulary stays honest: builtin / user / peer rows are labeled
with their provenance on the catalog (PL-9); tools are never re-labeled
or re-scoped to fit the MCP lane.

### Native CLI config JSON (per T0 verdict)

The native CLI `--mcp-config` story is **documented-only** (PL-10) — see
[Claude Code (command-line config)](#claude-code-command-line-config) and
[Codex (settings config)](#codex-settings-config) above for the exact
user-side JSON; nexus does not own native CLI spawn configuration. A
document-only native face is **not** an AC-V174-1 miss: the acceptance
journey closes on the wired ACP path above.
