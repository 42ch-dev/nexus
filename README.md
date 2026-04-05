# Nexus

**Open-source monorepo** for the Nexus CLI, daemon, and wire contracts.

## Repository Structure

```
schemas/                # JSON Schema truth source (codegen input)
crates/
  nexus-contracts/      # Generated Rust types
  nexus42/              # CLI binary (future)
  nexus42d/             # Daemon (future)
  nexus-sync/           # Bundle/outbox state machine (future)
packages/
  nexus-contracts/      # Generated TypeScript wire types (npm package)
tooling/
  codegen/              # Schema → TS + Rust pipeline (future)
docs/                   # User docs (installation, sync, troubleshooting)
.github/workflows/      # CI: schema validation, Rust fmt/clippy/test, npm publish
```

## Development

### Prerequisites

- Rust 1.75+ (stable)
- Node.js 20+
- pnpm 8+

### Setup

```bash
# Install Rust dependencies
cargo build

# Install Node dependencies
pnpm install
```

### Build

```bash
# Build all Rust crates
cargo build --all

# Build all TypeScript packages
pnpm run build
```

## Wire Contracts

**JSON Schema as truth source** - All wire types are generated from `schemas/`:

- TypeScript: `packages/nexus-contracts/src/generated/` (published to npm as `@42ch/nexus-contracts`)
- Rust: `crates/nexus-contracts/src/generated/` (published to crates.io as `nexus-contracts`)

**Do not modify generated types manually.** Update schemas, then run codegen.

## License

MIT
