# Nexus AGENTS.md

This file provides development guidance for agents working in the **nexus** open-source repository.

## Repository Identity

This is the **public open-source monorepo** containing:

- `nexus42` CLI executable (Rust)
- `nexus42d` daemon/supervisor (Rust)
- JSON Schema wire contracts (truth source for TypeScript/Rust code generation)
- Published packages: `@42ch/nexus-contracts` (npm) and `nexus-contracts` (crates.io)

**Not in this repo:** `nexus-platform` (private TypeScript monorepo for web/API/services) — do not reference its tech stack here.

## Tech Stack & Protocol Decisions

- **CLI/daemon: Rust-first** (not Go/Python) — aligns with ACP official SDK availability
- **Protocol: ACP-first, skills-second** — CLI is an ACP client, not an ACP agent/server
- **Wire format: JSON Schema as truth source** — generates both TypeScript and Rust types
- **Platform direction: TypeScript/Next.js/Vercel AI SDK** — but this repo only publishes wire contracts, not platform code

## Key Naming (Frozen)

- Product: **Nexus**
- CLI executable: **`nexus42`**
- Daemon: **`nexus42d`**
- npm scope: **`@42ch`**
- Contracts package: **`@42ch/nexus-contracts`**

## Monorepo Structure (Target)

```
schemas/                # JSON Schema truth source (codegen input)
crates/
  nexus-contracts/      # Generated Rust types
  nexus42/              # CLI binary
  nexus42d/             # Daemon
  nexus-sync/           # Bundle/outbox state machine (library)
  nexus-acp-*/          # ACP client adapters (optional subcrates)
packages/
  nexus-contracts/      # Generated TypeScript wire types (npm package)
tooling/
  codegen/              # Schema → TS + Rust pipeline
docs/                   # User docs (installation, sync, troubleshooting)
.github/workflows/      # CI: schema validation, Rust fmt/clippy/test, npm publish
```

## Development Workflow

**Schema/codegen flow:**

- JSON Schema (`schemas/`) → single codegen pass → Rust (`crates/nexus-contracts`) + TypeScript (`packages/nexus-contracts`)
- Both packages must be published and version-locked with `schema_version`
- CI validates schemas before generating code

**Rust development:**

- Use official ACP Rust SDK (not custom protocol implementations)
- CLI and daemon share generated contract types from `crates/nexus-contracts`
- Daemon is `nexus42d`, started via `nexus42 daemon start`

**TypeScript contract package:**

- `nexus-platform` (private repo) consumes `@42ch/nexus-contracts` via npm semver lock
- **No handwritten second DTO source** in platform — all wire types come from this repo's schemas

## Dev/Test Infrastructure

**Required containers:**

- Postgres + pgvector (`pgvector/pgvector:pg16`)
- Neo4j (`neo4j:5`)
- Redis (`redis/redis-stack-server:latest`)

**API keys (external, not in this repo's code but needed for integration):**

- LLM inference API (platform-side; CLI uses user's local agent)
- OAuth/IdP credentials (for web login + CLI device flow)

**CLI-only note:** ACP Registry is public (`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`); CLI pulls from it, no API key required.

## Versioning & Compatibility

- Schema contracts use `schema_version` field aligned with bundle envelope
- CLI SemVer must reflect breaking wire changes
- `@42ch/nexus-contracts` major bump → coordinated update across CLI + platform API + npm package
- Compatibility matrix maintained in internal runbook (not in this OSS repo)

## Constraints & Pitfalls

- **Do not treat `nexus42d` as an ACP Agent/Server** — it's a local supervisor, client-only
- **Do not sync full manuscript text by default** — only structured deltas/bundles
- **World history is immutable** — changes go through Fork, not in-place mutation
- **Wire contracts must match schemas** — no drift between `schemas/` and generated types
- **Single truth source for DTOs** — avoid parallel handwritten types in Rust or TypeScript