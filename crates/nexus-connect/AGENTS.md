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

`cargo tree -p nexus-connect` must stay free of `nexus-core`,
`nexus-orchestration`, `nexus-local-db` and `nexus-agent-host`;
`tooling/check-graph-pins.sh` pins the first two for the default graph.

## Features

`nexus-connect` has no default features. `peer-tools` is the registry alone
(all `nexus-core`'s `execution` cohort needs); `transport` is the WS transport
alone (what `memory-remote` dials through); `connect-host` adds the Connect-host
cohort; `connect-client` adds rmcp; `embedded-mcp` adds the Model B shell. The
shipped Connect host never enables rmcp.

## Tests

The retained peer contract assertions live beside this crate in
`crates/nexus-connect/tests/retained_peer_contracts.rs` and run with
`--features connect-client,embedded-mcp`. Their one core-only case (core's
host-tool roster) stayed in core's `tests/capability_compute.rs`: this crate has
no core edge.
