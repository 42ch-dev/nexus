# Nexus

Nexus is an open-source monorepo for:

- `nexus42` (CLI)
- `nexus42d` (local daemon/supervisor)
- schema-first wire contracts (`@42ch/nexus-contracts`, `nexus-contracts`)

## Current Status

This README is intentionally minimal during V1.2 delivery.

- Focus now: V1.2 CLI MVP and local-first runtime tracks.
- User-facing documentation will be expanded after V1.2 completion.

## Quick Start

```bash
# Build CLI
cargo build --release

# Show help
./target/release/nexus42 --help
```

## Basic Usage

```bash
# Initialize a workspace
./target/release/nexus42 init workspace "My Novel"

# Start daemon
./target/release/nexus42 daemon start

# List available commands
./target/release/nexus42 help
```

## Notes

- `nexus42` is a CLI client.
- `nexus42d` is a local daemon.
- This repository is under active V1.2 development.

## License

GPL-3.0-only (GNU General Public License v3.0)