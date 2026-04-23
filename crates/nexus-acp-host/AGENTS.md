# nexus-acp-host — ACP Client Adapter

This crate provides the ACP (Agent Client Protocol) client adapter used by `nexus42` and `nexus42d`.

## Protocol Rules

- **ACP-first:** all agent communication goes through the ACP protocol.
- **Client only:** this crate implements ACP client behavior. Do NOT implement ACP agent/server functionality here.
- **Use official ACP Rust SDK:** do not build custom protocol implementations or HTTP layers.

## ACP Registry

The ACP Registry is public — no API key required:
`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`

## Integration

- Both `nexus42` (CLI) and `nexus42d` (daemon) depend on this crate for ACP communication.
- Contract types come from `crates/nexus-contracts` (generated from `schemas/`).
