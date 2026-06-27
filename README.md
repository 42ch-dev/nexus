# Nexus

Nexus is a local-first creative writing tool.

## Quick Start

```bash
# Build
cargo build --release

# Show help
./target/release/nexus42 --help

# Start daemon
./target/release/nexus42 daemon start
```

## Monorepo Layout

| Directory | Contents |
|-----------|----------|
| `apps/` | Product surfaces — `nexus42` (Rust CLI + daemon), `desktop` (Tauri client), `web` (browser SPA) |
| `crates/` | Reusable Rust libraries (daemon runtime, orchestration, local DB, contracts, etc.) |
| `packages/` | npm packages — `@42ch/nexus-contracts` generated from `schemas/` |
| `modules/` | Domain content (embedded presets, WASM modules, reference data) |
| `tooling/` | Codegen pipeline and CI helpers |
| `schemas/` | JSON Schema wire contracts — single truth source for Rust + TypeScript types |

## License

Apache-2.0
