# nexus-connect — Peer Connect Protocol

The peer-facing Connect protocol stack, extracted from `nexus-core`'s `connect`
module (v1.244 packaging ladder). It owns:

- `peer_tools` — the process-global, protocol-neutral peer tool registry (the
  reverse-invoke spine: admission chain, collision policy, eviction).
- `ws_transport` — the bounded message-oriented WS `Transport` both the peer
  stack and the remote memory client dial through.
- `accept` / `config` / `events` / `identity` / `session` / `table` /
  `visibility` / `watch` / `peer_control` — the Connect-host cohort: accept
  loop, config snapshot + reload watcher, session manager, MCP catalog
  visibility, peer-control lane.
- `mcp_bridge` / `mcp_embedded` — the rmcp reverse-invoke/operator bridge and
  the embedded Model B server shell.

## Dependency direction

`nexus-core → nexus-connect`, never back. Nothing in this crate may name a
`nexus_core` item:

- Errors are local (`ConnectError::Internal { category }`); core's `CoreError`
  carries the local-db/orchestration taxonomy this protocol never touches.
- The reserved builtin tool ids are **injected** by the composing side through
  `start_peer_tools_lane` / `PeerResponderOptions::builtin_tool_ids`; this
  crate never reads a host-tool registry.
- `ExecutionHandle`'s peer-control entry points stay in `nexus-core`
  (`execution/handle_ops.rs`) — Rust cannot write an inherent impl for a
  foreign type. [`PeerControlLane::new`], `set_enabled`, `allow` and `state`
  are public for exactly that boundary.

`cargo tree -p nexus-connect --no-default-features` must stay free of
`nexus-core`, `nexus-orchestration`, `nexus-local-db` and `nexus-agent-host`;
`tooling/check-graph-pins.sh` pins the first two for exactly that graph. Feature
cohorts are a different matter: `peer-tools` pulls `nexus-spoke-adapter`, which
reaches `nexus-local-db` (see Features).

## Features

`nexus-connect` has no default features; six select what compiles.

| Feature | Adds | Selected by |
| --- | --- | --- |
| `peer-tools` | the registry alone (`peer_tools.rs`): admission chain, collision policy, eviction | `nexus-core`'s `execution` cohort |
| `transport` | `ws_transport.rs` plus `spoke-connect` / `tokio-tungstenite` / `futures-util` | `memory-remote` |
| `connect-host` | accept / config / events / session / table / watch / peer-control cohort with its contracts, home-layout and orchestration inputs; implies `peer-tools` and `transport` | `nexus-core`'s `connect-host` |
| `connect-client` | the rmcp bridge (`mcp_bridge.rs`) via `rmcp` | core `connect-client` |
| `embedded-mcp` | the Model B shell (`mcp_embedded.rs`) plus `futures-channel` | core `embedded-mcp` |
| `memory-remote` | `transport` alone — the WS lane the remote memory client dials | core `memory-remote` |

`peer-tools` — and so every cohort built on it (`connect-host`,
`connect-client`, `embedded-mcp`) — reaches `nexus-local-db` transitively through
`nexus-spoke-adapter`: the registry reuses that boundary's descriptor/argument
validators and its wire refusal codes instead of writing a second copy. The
protocol itself reads none of it, so an unfeatured build stays free of the
local-db and orchestration edges.

The shipped Connect host never enables rmcp.

## Tests

The retained peer contract assertions live beside this crate in
`crates/nexus-connect/tests/retained_peer_contracts.rs` and run with
`--features connect-client,embedded-mcp`. Their one core-only case (core's
host-tool roster) stayed in core's `tests/capability_compute.rs`: this crate has
no core edge.
