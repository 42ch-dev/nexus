# nexus42 — CLI Executable

`nexus42` is the CLI for Nexus. It is an **ACP client**, not an ACP agent/server.

## Key Rules

- **Protocol:** ACP-first, skills-second. Talks to ACP servers as a client.
- **Contract types:** shares generated types from `crates/nexus-contracts`. Do NOT hand-write duplicate DTOs.
- **ACP SDK:** use the official ACP Rust SDK (via `crates/nexus-acp-host`). Do not implement custom protocol plumbing.

## ACP Registry

Public — no API key required: `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`

## Naming (Frozen)

- CLI executable: **nexus42**
- Product: **Nexus**
