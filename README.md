# Nexus42

**CLI-native creative writing platform** with built-in AI agent integration via the [Agent Client Protocol (ACP)](https://agentclientprotocol.com/).

Nexus42 helps writers manage manuscripts, build worlds, and collaborate with AI agents — all from the terminal. It provides structured lifecycle management (draft → review → published), world-building with 15 domain aggregates, and seamless integration with 16+ ACP-compatible AI agents.

## Quick Start

```bash
# Build from source
cargo build --release

# Initialize a workspace
./target/release/nexus42 init workspace "My Novel"

# Start the daemon
./target/release/nexus42 daemon start

# Create a manuscript
./target/release/nexus42 manuscript create "Chapter 1" --phase draft

# Run an AI agent
./target/release/nexus42 agent run claude-acp
```

### Local data layout

Operational SQLite (`state.db`) and workspace metadata sit under `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/`. The default creative tree is under your **Documents** folder (`Documents/nexus/<creator_id>/<workspace_slug>/`; override with `nexus42 init workspace --creative-root <path>`). If you still have a legacy flat `$HOME/.nexus42/state.db`, move it into the new layout with `nexus42 migrate local-fs --help`.

## Status

**V1.0 GA** — Foundation release. Core infrastructure complete, Phase 2 in progress.

- ✅ 15 domain aggregates with rigorous consistency rules
- ✅ Sync mechanism (Command, DeltaBundle, Outbox, ConflictResolution)
- ✅ ACP SDK integration with registry cache (16 agents)
- ✅ Context assembly for AI agent context
- ✅ 445+ tests passing

> **Note**: V1.0 is a foundation release. Many CLI commands are skeleton implementations being filled in Phase 2. See the [project roadmap](#roadmap) below.

## Key Features

### Manuscript Lifecycle

Manage manuscripts through structured phases:

```bash
nexus42 manuscript create "Chapter 1"
nexus42 manuscript phase "Chapter 1" review
nexus42 manuscript promote "Chapter 1"
nexus42 manuscript verify "Chapter 1"
```

### World Building

Structured world-building with characters, timelines, and story manifests:

```bash
nexus42 creator register "My Pen Name"
nexus42 research scan
```

### AI Agent Integration

First-class ACP support — run AI agents that can read and write your workspace:

```bash
nexus42 agent list              # 16+ agents available
nexus42 agent run claude-acp     # Interactive agent session
nexus42 agent probe --agent claude-acp
```

### Sync

Delta-based synchronization with conflict resolution:

```bash
nexus42 sync status
nexus42 sync push
```

## Repository Structure

```
schemas/                # JSON Schema truth source (codegen input)
crates/
  nexus-contracts/      # Generated Rust types
  nexus-domain/         # 15 domain aggregates with consistency rules
  nexus-sync/           # Command/DeltaBundle/Outbox state machine
  nexus42/              # CLI binary (nexus42)
  nexus42d/             # Daemon (nexus42d)
packages/
  nexus-contracts/      # Generated TypeScript wire types (npm: @42ch/nexus-contracts)
tooling/
  codegen/              # Schema → TS + Rust pipeline
docs/                   # User & contributor documentation
.agents/plans/          # Development plans, knowledge base, review reports
.github/workflows/      # CI: schema validation, codegen diff, Rust fmt/clippy/test
```

## Architecture

```
nexus-contracts  ← (no internal deps — generated from JSON Schema)
     ↑
nexus-domain     ← nexus-contracts (15 aggregates, 133 tests)
     ↑
nexus-sync       ← nexus-contracts (sync library, 226 tests)
nexus42          ← nexus-contracts, nexus-domain, ACP SDK
nexus42d         ← nexus-contracts, nexus-domain, axum, SQLite
```

**Key design decisions:**

- **JSON Schema as truth source** — All wire types generated from `schemas/`. Do not hand-edit generated files.
- **CLI is an ACP client, not an ACP agent/server** — Nexus42 drives AI agents, doesn't host them.
- **Daemon is a local supervisor** — HTTP API on port 8420, not an ACP agent.
- **Schema-first contracts** — TypeScript (`@42ch/nexus-contracts`) and Rust (`nexus-contracts`) stay in sync via codegen.

## Development

### Prerequisites

- Rust 1.75+ (stable) + nightly toolchain (for rustfmt)
- Node.js 20+ / pnpm 8+

### Setup

```bash
# Install Rust nightly (for formatting)
rustup toolchain install nightly --component rustfmt

# Build all Rust crates
cargo build --all

# Install Node dependencies
pnpm install
```

### Codegen

**Any commit that touches `schemas/` MUST also include regenerated output:**

```bash
pnpm run codegen
git add packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
```

### CI Checks (run locally before pushing)

```bash
# 1. Schema validation
node tooling/validation/schema-validator.js

# 2. Codegen matches committed output
pnpm run codegen
git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/

# 3. Rust formatting
cargo +nightly fmt --all -- --check

# 4. Rust lints
cargo clippy --all -- -D warnings

# 5. TypeScript contract package
pnpm run typecheck

# 6. Tests
cargo test --all
```

## Roadmap

| Version | Scope | Status |
|---------|-------|--------|
| **V1.0** | Foundation: domain models, CLI/daemon, sync contract, ACP client, context assembly | ✅ Complete |
| **V1.1** | Foundation hardening, ACP SDK bridge, manuscript ops, auth flow, codegen alignment | 🔄 In progress |
| **V1.2** | Sync push/pull with platform, conflict resolution UI, session persistence | Planned |
| **V2.0** | GUI frontend, full platform sync, collaboration, plugin ecosystem | Future |

## Wire Contracts

**JSON Schema as truth source** — all wire types are generated from `schemas/`:

- TypeScript: `packages/nexus-contracts/src/generated/` → npm [`@42ch/nexus-contracts`](https://www.npmjs.com/package/@42ch/nexus-contracts)
- Rust: `crates/nexus-contracts/src/generated/` → crates.io [`nexus-contracts`](https://crates.io/crates/nexus-contracts)

**Do not modify generated types manually.** Update schemas, then run `pnpm run codegen`.

## License

MIT
