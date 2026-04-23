# nexus42 — CLI Executable

`nexus42` is the command-line interface for Nexus. It is an **ACP client**, not an ACP agent/server.

## Key Rules

- **Protocol:** ACP-first, skills-second. The CLI talks to ACP servers as a client.
- **Contract types:** shares generated types from `crates/nexus-contracts` with `nexus42d` and other workspace crates. Do NOT hand-write duplicate DTOs.
- **ACP SDK:** use the official ACP Rust SDK (via `crates/nexus-acp-host`). Do not implement custom protocol plumbing.
- **Daemon control:** start the daemon via `nexus42 daemon start`.

## ACP Registry

The ACP Registry is public — no API key required:
`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`

## Naming (Frozen)

- CLI executable: `**nexus42`**
- Product: `**Nexus**`
- npm scope: `**@42ch**`
