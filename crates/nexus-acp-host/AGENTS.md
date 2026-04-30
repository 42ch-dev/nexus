# nexus-acp-host — ACP Client Adapter

Provides the ACP client adapter used by `nexus42` and `nexus42d`.

## Protocol Rules

- **ACP-first:** all agent communication goes through the ACP protocol.
- **Client only:** do NOT implement ACP agent/server functionality here.
- **Use official ACP Rust SDK:** do not build custom protocol implementations or HTTP layers.

## ACP Registry

Public — no API key required: `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`

## Integration

Both `nexus42` and `nexus42d` depend on this crate. Contract types come from `crates/nexus-contracts` (generated from `schemas/`).
