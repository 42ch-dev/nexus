---
module: apps/nexus42, nexus-daemon-runtime, rmcp, nexus-acp-host
date: 2026-08-25
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-08-24-v1.174-p0-peer-tools-transport-registry
tags: 
  - mcp
  - rmcp
  - stdio
  - bridge-child
  - stateless-proxy
  - serverhandler
  - tools-only
applies_when:
  - "Exposing the daemon tool registry to an external MCP client (ACP agent, native CLI, third-party)"
  - "Choosing the process model for an MCP server (client-spawned child vs in-daemon service)"
  - "Implementing a rmcp ServerHandler with a dynamic, registry-backed tool catalog"
  - "Mapping non-MCP error vocabularies onto MCP protocol vs tool-result errors"
---

# Stateless MCP bridge child (Model A) — client-spawned stdio proxy, lockstep by construction

## Context

V1.174 exposes nexus's tool registry to external MCP clients through a
**tools-only MCP server**. The process-model question came first: who spawns
the MCP server? Evidence from the consumption stories settled it — ACP
`McpServerStdio::new(name, command)` passes a command for the **agent binary**
to spawn as its own stdio child (`nexus-acp-host/src/client.rs` L203-218), and
CLI `--mcp-config` semantics are the same. An in-daemon embedded MCP service
has no stdio channel to those external children. The locked model is
therefore **Model A: the MCP client spawns `nexus42 mcp serve` as its own
stdio child**, and the child is a thin, stateless proxy.

## Guidance

### 1. The child holds nothing; every request is a live daemon round trip

- `nexus42 mcp serve` (behind `connect-client`): an rmcp `ServerHandler`
  with **no registry, no allowlist, no policy, no cache**.
  `tools/list` → live `GET /v1/daemon/tools`; `tools/call` → live
  `POST /v1/daemon/agent-host/internal/tool-executions` (the spine's existing
  HTTP face — no new dispatch route).
- Daemon transport resolution **reuses the CLI daemon-client rules** (Unix
  `socket_path` → `NEXUS_DAEMON_PORT` → `127.0.0.1:8420`) — no new discovery
  mechanism.
- **Lockstep by construction:** the child never caches, so `tools/list`
  cannot drift from the registry.
- **stdout is the JSON-RPC channel** — route all tracing to stderr
  (`init_logging` takes `stderr_only` for the child).
- **Security note:** the child grants nothing new — a local process that
  could spawn `nexus42 mcp serve` could already call the daemon's loopback
  HTTP directly; all policy stays daemon-side.

### 2. rmcp 1.8.0 realities (source-verified, three corrections shipped)

- **Runtime-dynamic handler.** `ServerHandler`'s `list_tools`/`call_tool`
  are plain async trait methods — the handler builds the tool list at call
  time from any source. The `#[tool_handler]` macro router is optional and
  static-only; our registry-dynamic catalog maps natively. **schemars stays
  off our surface** — `Tool.input_schema` is an `Arc<JsonObject>` constructed
  directly.
- **Default lists are empty, not errors.** rmcp 1.8.0's default
  `prompts/list` and `resources/list` return `Ok(default())` — **empty
  result lists** — and the service layer does not gate by advertised
  capabilities. The tools-only boundary is enforced by **absence**: only
  `get_info`/`list_tools`/`call_tool` overrides exist; unroutable
  `prompts/get` / `resources/read` remain `METHOD_NOT_FOUND`. Machine-check
  with grep + a protocol probe
  (`prompts_and_resources_are_empty_lists_not_errors`).
- **Two-class error mapping** (SDK-documented semantics):
  - *Unroutable* ⇒ JSON-RPC protocol error `Err(McpError)` —
    `METHOD_NOT_FOUND` for never-admitted/evicted/non-exposable ids (name
    the refusal class); `INTERNAL_ERROR` for daemon unreachable / auth
    rejected, **bounded** by the daemon client's connect/request timeouts —
    never a hang.
  - *Executed-but-failed* ⇒ `Ok(CallToolResult::error(...))` with content
    naming the spine code: structural argument failure (`invalid_input`),
    peer deny (`op_unsupported`/`capability_missing` with the lowercase
    `wire_code` preserved), invoke timeout, transport closed mid-invoke,
    user-cap `run()` error. Success ⇒ text content + `structured_content` =
    the spine result value when it is a JSON object (else text-only).
- **Pin** `rmcp = { version = "=1.8.0", default-features = false, features =
  ["server", "transport-io"] }` behind `connect-client`. The pin's job is
  **single-version lockstep** with the rmcp already in the graph via
  `nexus-acp-host → agent-client-protocol =0.11.1` — not graph introduction
  (see `conventions/graph-pin-honesty-discipline.md`). No streamable-HTTP,
  no child-process, no client features.
- Advertisement: **tools only, without `listChanged`** (no daemon→child push
  channel; clients re-list; push = DF-90).

### 3. Schema mapping (descriptors → rmcp `Tool`)

| Source | `input_schema` | `output_schema` |
|--------|----------------|-----------------|
| Peer tools (manifest) | carried **verbatim** after catalog root-object filter (`mcp_catalog: input_schema not root-object` = named refusal from the MCP catalog; registration lane unaffected) | included iff present **and** root-object; else omitted — never invented, never wrapped |
| User capabilities | parsed draft-2020-12; unparseable ⇒ named catalog refusal | same parse rule |
| Builtin `nexus.*` | uniform permissive `{"type":"object"}` placeholder (the `AcpWire` schema refs are pseudo-schemas, not valid 2020-12); description carries the summary + parameter pointer | none |

No schema is ever synthesized from a non-schema source beyond the declared,
uniform, documented builtin placeholder (real per-tool schemas = DF-89).

### 4. Alternatives evaluated (roadmap, not leftovers)

- **Model B — in-daemon embedded rmcp service** over an in-memory
  (sink/stream) transport: technically feasible (verified) but serves only
  in-daemon MCP clients, which do not exist → DF-88.
- **Model C — daemon spawns and owns the child:** structurally incoherent
  for stdio (the stdio parent must be the MCP client) → rejected without a
  roadmap row.
- **Streamable HTTP + remote consumers:** a different trust class (network
  exposure) → DF-87, never "leftover stdio work".

### 5. Gotchas discovered in the first implementation

- **Message-match discriminators:** the daemon's `BadRequest` Display wraps
  the spine error as `"Bad request: unsupported tool: {id}"` — match with
  `contains`, not `starts_with`, or unroutable ids surface as
  `Ok(CallToolResult::error)` instead of `METHOD_NOT_FOUND` (caught by the
  three-process E2E).
- **Args must survive the ACP mapping:** the `NexusMcpServer::Stdio` → SDK
  mapping must carry `args` (`["mcp", "serve"]`); a hand-built stdio entry
  that drops args spawns the CLI without the serve subcommand. Pin with a
  unit test on the propagation.
- **Timeout ordering:** the child's request timeout must strictly exceed
  the user-cap sandbox wall (30 s) so a slow-but-legitimate user-cap call is
  not raced by the child's own timeout (side-effects-after-timeout must be
  documented).

## Why This Matters

The child is the entire MCP surface for external consumers. If it cached or
held policy, it would be a second authorization domain and a drift surface —
both prohibited by the honesty lockstep. The stateless design moves every
decision daemon-side (where the allowlist, admission, and dispatch live) and
makes `tools/list` ⇄ catalog lockstep **true by construction** instead of by
test. The rmcp 1.8.0 corrections (dynamic handler, empty-list defaults)
matter because they were documented wrongly once and cost a spec-correction
round; the empty-lists reality is the honest boundary for a tools-only
server.

## When to Apply

- Building any MCP server whose catalog is a live registry (never a cached
  snapshot).
- Choosing between child-process and embedded MCP — ask first who owns the
  stdio channel; if the client spawns, the child must be stateless.
- Mapping a domain error vocabulary onto MCP: unroutable ⇒ protocol error,
  executed-but-failed ⇒ `is_error` result; keep the split typed, not
  text-matched.
- Extending the exposure lane (HTTP transport, `listChanged`, embedded
  Model B) — reuse this child's daemon-round-trip + resolution rules.

## Examples

- `apps/nexus42/src/commands/mcp/mod.rs` — the `ServerHandler` impl,
  stderr-only logging, `contains`-based unroutable discriminator.
- `apps/nexus42/src/api/daemon_client.rs` — `post_execution_raw` preserving
  the spine's structured error body (unroutable vs executed-failure vs
  auth-refused distinguishable).
- Proof suites: `apps/nexus42/tests/mcp_serve_e2e.rs` (7, incl.
  `call_tool_daemon_down_is_bounded_internal_error` and the empty-lists
  probe), `mcp_acp_probe.rs` (C-1 ACP stdio injection), `mcp_session_wiring.rs`,
  `apps/nexus42/tests/e2e_peer_mcp.rs` (three-process journeys).
- Consumer docs: `docs/mcp-server.md` (integrator/operator duties, native
  `--mcp-config` story).
- Companion: `architecture-patterns/peer-tool-registration-exposure-lanes.md`
  (the spine + admission the child proxies).
