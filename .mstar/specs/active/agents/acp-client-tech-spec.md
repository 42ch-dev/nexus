# ACP Client Integration — Technical Specification

**Status:** Shipped. The Rust adapter uses official `agent-client-protocol = "=3.2.0"` (stable-v1 API via `schema::v1::*`; no unstable v2 features, no `agent-client-protocol-rmcp`) behind Nexus-owned DTOs. Current service composition uses the core-owned `HostManager` and `packages/nexus-provider-acp` callbacks, not a route-facing daemon manager plus per-creator `acp-worker` children.
**Document class**: Master  

**Source Plan**: `2025-04-05-acp-client`
**Date**: 2026-04-06
**Last reconciled**: 2026-10-13 — `agent-client-protocol = "=3.2.0"` pin, the v3 opt-in feature contract, and the v3.2.0 capability disposition (§1.2).


The shipped boundary is:

- `crates/nexus-acp-host/Cargo.toml` pins
  `agent-client-protocol = "=3.2.0"`; stable-v1 messages are imported from
  `agent_client_protocol::schema::v1` (the flat `schema` re-export was
  removed upstream) and the adapter pins `ProtocolVersion::V1` explicitly.
- Official SDK types are confined to `nexus-acp-host`; the public
  `NexusAcpClient` trait uses Nexus-owned DTOs and includes both one-shot and
  streaming prompt operations without treating a method count as a contract.
- `nexus-agent-host::providers::acp` consumes `NexusAcpClient` /
  `AcpSdkAdapter`, but those provider-specific types do not cross the
  `HostFacade` boundary.
- [`HostManager`](../../../../crates/nexus-agent-host/src/core/manager.rs) owns provider/session policy. `register_provider` accepts an adapter plus its launch recipe; ordinary `start` instead materializes the admitted catalog when no adapters were pre-registered. [`core/readiness.rs`](../../../../crates/nexus-agent-host/src/core/readiness.rs) builds that catalog from configured providers and PATH discovery, not a fixed native-only registration list.
- [`nexus-core-node` boot](../../../../crates/nexus-core-node/src/lifecycle.rs) starts one manager, wraps a supplied JS provider port in `AdmittingProviderPort` (otherwise uses `host.build_provider_port()`), and adopts that same manager into the core host authority. [`apps/nexus-service/src/index.ts`](../../../../apps/nexus-service/src/index.ts) supplies `createAcpProvider()` except in domain-only mode; [`lifecycle.ts`](../../../../apps/nexus-service/src/lifecycle.ts) passes it to `openCore`.
- The Rust `AcpProvider` stores an enabled ACP launch recipe and lazily owns a separate child for each Host session. The TS callback implementation in [`packages/nexus-provider-acp`](../../../../packages/nexus-provider-acp/src/acp.ts) owns its ACP sessions and subprocesses through [`process-owner.ts`](../../../../packages/nexus-provider-acp/src/process-owner.ts). Neither path requires the deleted daemon host.

---

## Table of Contents

1. [SDK Selection Decision](#1-sdk-selection-decision)
2. [Transport, Boundary, and Capabilities](#2-transport-boundary-and-capabilities)

---

## 1. SDK Selection Decision

### 1.1 Current dependency and boundary

The shipped Rust ACP SDK is **`agent-client-protocol` 3.2.0**, exact-pinned
(`=3.2.0`) in `crates/nexus-acp-host/Cargo.toml`. ACP crate major 3 does not
put the wire on protocol v2 — the adapter stays on stable v1 and does not pull
`rmcp` (the ordinary-CLI ACP graph is rmcp-free; see `tooling/check-graph-pins.sh`).
v3 makes the SDK's capabilities opt-in: the SDK declares `default = []`, so
its native transports (`process`, `stdio`) and JSON-Schema generation
(`schemars`) ship only when explicitly enabled, and Nexus intentionally
enables none of them — it owns child-process lifecycle itself (Tokio `Command`
+ `ByteStreams`) and owns its wire schemas. The SDK row therefore resolves to
exactly one version whose resolved feature set is the empty default (printed
`feats=[default]`), with `rmcp`, `async-process` and `blocking` absent,
enforced by `assert_exactly_one nexus42 "" agent-client-protocol`,
`assert_features nexus42 "" agent-client-protocol "default"`,
`assert_empty nexus42 "" async-process` and `assert_empty nexus42 "" blocking`
in `tooling/check-graph-pins.sh`. `crates/nexus-acp-host/src/client.rs`
confines `agent_client_protocol::schema` types to `AcpSdkAdapter` conversion
and implementation code; consumers use nexus contract DTOs through
`NexusAcpClient`.

`crates/nexus-agent-host/src/providers/acp.rs` adapts that client boundary to
the normalized `ProviderAdapter` lifecycle (initialize, session creation,
prompt/stream, cancel, shutdown). The integrated daemon is retired; current
service/provider ownership is described above. No fixed client-method count is normative: the trait definition and
its adapter implementation are the source authority as protocol support
evolves.

### 1.2 v3.2.0 capability disposition

Recorded 2026-10-13 for the `=3.2.0` adoption (plan
`2026-10-12-v1.215-acp-sdk-3.2.0-adoption`). Every row was re-verified against
the fetched crate sources — cited as `<crate>-<version>/<path>:<line>` under
`$CARGO_HOME/registry/src/index.crates.io-*/` — or against the repo file:line
cited; a plan claim that failed verification is rewritten to what the sources
show, not softened.

#### Adopted

| Capability | Why | Verified against |
|---|---|---|
| v3 opt-in feature contract: the SDK declares `default = []`; its native transports (`process`, `stdio`) and JSON-Schema generation (`schemars`) are opt-in Cargo features, and the pin enables none of them | Nexus owns child-process lifecycle (Tokio `Command` + `ByteStreams`) and its own wire schemas, so the featureless pin is the desired end state; moving the adapters behind features removes `async-process`, `blocking`, `async-channel`, `async-lock`, `async-signal`, `async-task` and `piper` from the workspace lock outright | `agent-client-protocol-3.2.0/Cargo.toml.orig:42` (`default = []`), `:45` (`schemars`), `:48-49` (`process` / `stdio` dep lists); the 2.2.0 baseline carried the same crates unconditionally (`agent-client-protocol-2.2.0/Cargo.toml.orig:65,73-82`); repo `Cargo.lock` delta `f99b21459`→`e5887127a` (all seven 1→0). Enforced by `tooling/check-graph-pins.sh:207` `assert_exactly_one nexus42 "" agent-client-protocol`, `:211` `assert_features nexus42 "" agent-client-protocol "default"`, `:214` `assert_empty nexus42 "" async-process`, `:215` `assert_empty nexus42 "" blocking` |

#### Inherited without callsite change

| Capability | Why | Verified against |
|---|---|---|
| Graceful drain / connection lifetimes (#385); protocol-neutral raw JSON-RPC error preservation (#384) | Both are breaking SDK-internal changes that pass underneath Nexus: the repo holds no raw JSON-RPC relay and no `ConnectTo::into_channel_and_future` callsite (grep of `crates/` + `apps/`: 0 hits), and its error path stays typed | `agent-client-protocol-3.2.0/CHANGELOG.md:47-48` (3.0.0 "Fixed"); typed error conversion at `crates/nexus-acp-host/src/error.rs:174-175` (`pub fn sdk(err: &agent_client_protocol::Error)`) |
| Ordered prompt publication (`send_ordered_request_to` + `on_receiving_result`); the 3.1.0 `prepare_request` ordering API leaves eager sends unchanged | Already consumed, no change: `ActiveSession::send_prompt` published ordered in 2.2.0 and still does in 3.2.0, and Nexus sends prompts only through it | `agent-client-protocol-3.2.0/src/session.rs:1164-1179` (`send_prompt` → `send_ordered_request_to` + `on_receiving_result`); same shape in `agent-client-protocol-2.2.0/src/session.rs:1035-1050`; Nexus callsites `crates/nexus-acp-host/src/client.rs:955,1029`; `agent-client-protocol-3.2.0/CHANGELOG.md` 3.1.0 (#407) |
| Sensitive-log + URL-credential redaction (#401) — **rewritten on failed verification**: not inherited, because it is outside the consumed crate | The fix is real upstream but landed in `agent-client-protocol-http` and `agent-client-protocol-conductor`, neither of which is in Nexus's dependency graph; the consumed core crate ships no redaction code, so there is nothing to adopt, inherit, or regression-test | Whole-crate grep of `agent-client-protocol-3.2.0/` for `redact` / `credential`: 0 hits; upstream PR #401 ("fix: avoid sensitive transport logs and redact trace URL credentials", merged 2026-10-06) touches only `src/agent-client-protocol-http/**`, `src/agent-client-protocol-conductor/**`, `md/**`, `Cargo.lock` |

#### Declined

| Capability | Why | Verified against |
|---|---|---|
| Retained `RequestCancellationHandle` (`cancellation_handle()`, #409) | Unreachable without rewriting the session layer: `ActiveSession::send_prompt` returns `Result<(), Error>` and exposes no request handle. Automatic drop-cancel already existed in 2.2.0 and remains in 3.2.0, so Nexus's `tokio::select!` + drop pattern already issues `$/cancel_request`; ACP `session/cancel` stays the user-facing cancel | `agent-client-protocol-3.2.0/src/session.rs:1164`; the handle exists on prepared/sent requests at `agent-client-protocol-3.2.0/src/jsonrpc.rs:5981,6462`, with drop-cancel retained at `:6320`; 2.2.0 baseline `agent-client-protocol-2.2.0/src/jsonrpc.rs:5535` (`impl Drop for SentRequestCancellation`); `$/cancel_request` wire name `agent-client-protocol-schema-1.10.2/src/v1/protocol_level.rs:73`; Nexus `crates/nexus-acp-host/src/client.rs:1203,1279,1420,1549,1624,1702` (select + drop), `:1462` (`cancel()`) |
| `AcpAgent` / `AcpAgentConfig` (`process` feature), native `Stdio` (`stdio` feature) | Nexus spawns and reaps its own children with `tokio::process` and owns termination semantics; adopting either adds a second lifecycle owner | Feature-gated exports: `agent-client-protocol-3.2.0/src/lib.rs:202-209` (`AcpAgent`, `AcpAgentConfig`), `:222-226` (`Stdio`); Nexus-owned lifecycle `crates/nexus-acp-host/src/transport.rs:35,141-158` |
| `schemars` JSON-Schema generation, typed MCP tool builders | Nexus owns its wire contracts as JSON Schema in `schemas/` (generated into TS + Rust); SDK schema generation would be a second, unowned truth source | Opt-in since v3: `agent-client-protocol-3.2.0/Cargo.toml.orig:45,83` (unconditional in the 2.2.0 baseline at `:65`); `agent-client-protocol-3.2.0/CHANGELOG.md` 3.0.0 (#373); repo `schemas/` |
| Native MCP services, request-scoped MCP binding, `unstable_mcp_over_acp` | Nexus is an ACP **client** that forwards generic `newSession.mcp_servers` descriptors; native MCP services belong to agents, and `rmcp` stays out of the shipped graph | `agent-client-protocol-3.2.0/Cargo.toml.orig:65`; `agent-client-protocol-3.2.0/CHANGELOG.md` 3.0.0 (#388 request-scoped binding, native MCP services); descriptor forwarding `crates/nexus-acp-host/src/client.rs:222-232`; `rmcp` absence enforced by `tooling/check-graph-pins.sh:203` `assert_empty nexus42 "" rmcp` |
| `unstable_protocol_v2`, `unstable_subagents`, session compaction / fork / notices, `unstable_llm_providers`, `unstable_end_turn_token_usage` | The adapter pins `ProtocolVersion::V1` and the posture is stable-v1-only; schema 1.10.2 adds **no new stable v1 message/update surface** — its only v1 method-name delta is the removal of the `unstable_mcp_over_acp`-gated `mcp/connect` / `mcp/disconnect` pair, and its remaining v1 delta is the removal of the internal `SkipListener` deserialization seam, which Nexus never implemented | Pin: `crates/nexus-acp-host/src/client.rs:205-218`. Unstable feature forwards: `agent-client-protocol-3.2.0/Cargo.toml.orig:53-71`. Schema 1.9.1→1.10.2: v1 method-name sets identical except `mcp/connect` / `mcp/disconnect` removed from the `#[cfg(feature = "unstable_mcp_over_acp")]`-gated module (`agent-client-protocol-schema-1.10.2/src/v1/mod.rs:9-10,25-26`); `SkipListener` present in `agent-client-protocol-schema-1.9.1/src/v1/client.rs:16,341,499,803,1469`, 0 hits in 1.10.2, 0 hits in Nexus `crates/` + `apps/` |

---

## 2. Transport, Boundary, and Capabilities

nexus42 hosts ACP agents directly over stdio: the adapter spawns the agent
subprocess and speaks JSON-RPC on its stdin/stdout. No service mediates the
wire.

Agents do not receive raw local-service access merely by participating in ACP,
and no ACP-specific local-service API endpoints exist. Agent → client requests
(`request_permission`, `fs/*`, `terminal/*`) are mediated by the permission
policy in `crates/nexus-acp-host/src/policy.rs`; operators inspect and adjust
it through `nexus42 acp permission`.

The client capabilities declared at `initialize` are the frozen set defined in
[`crates/nexus-acp-host/src/skills.rs`](../../../../crates/nexus-acp-host/src/skills.rs):
`file_system.read`, `file_system.write`, `terminal.create`, `terminal.output`,
`terminal.release`.

Agent registry/catalog behavior is owned by
[registry-integration.md](registry-integration.md); the unified Host/Provider
model and creator-facing lifecycle are owned by [agent-host.md](agent-host.md).

---

## Appendix A: ACP Protocol Lifecycle Reference

For implementer reference, the ACP protocol lifecycle:

1. **initialize**: Client sends `initialize` with capabilities. Agent responds with its capabilities.
2. **authenticate** (optional): If agent requires authentication.
3. **session/new** or **session/load**: Create or resume a session.
4. **session/prompt**: Send user message → agent streams response (markdown).
5. **cancel**: Cancel in-progress prompt.

**Agent → Client requests** (nexus42 must handle as client):
- `request_permission`: Agent asks permission to use a tool. Adjudicated by the permission policy (`crates/nexus-acp-host/src/policy.rs`).
- `fs/write_text_file`, `fs/read_text_file`: Agent reads/writes files. Adjudicated by the permission policy.
- `terminal/create`, `terminal/output`, `terminal/release`, `terminal/wait_for_exit`, `terminal/kill`: Terminal management. The declared capability set is listed in §2.

## Appendix B: Residual Findings

> **Durable roadmap:** DR-20, DR-21, DR-22 (ACP-R3..R11: daemon-mediated tool access + permission policy, session persistence, `terminal.kill`/`terminal.wait_for_exit`, `slash_commands`, `agent_plan`, persistent skills manifest, binary auto-update, `session.modes`).
>
> Verify against current behavior before picking up.
