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

## Wired consumption path: hosted ACP agents

For nexus-hosted ACP agent sessions the MCP server is wired first-class
(AR-75 C-1, P1 T1): `nexus-acp-host::mcp::nexus_mcp_stdio_server()` produces
`McpServer::Stdio { name: "nexus", command: "<nexus42>", args: ["mcp", "serve"] }`
carried on `NewSessionRequest.mcp_servers` when the `connect-client`
feature is enabled. This is the AC-closing journey — a scripted ACP agent
spawns the real `nexus42 mcp serve` child and calls an integrator-registered
peer tool through its own MCP client.
