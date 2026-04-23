# Nexus

Nexus is a local-first creative writing tool.

## Quick Start

```bash
# Build
cargo build --release

# Show help
./target/release/nexus42 --help

# Initialize a workspace
./target/release/nexus42 init workspace "My Novel"

# Start daemon
./target/release/nexus42 daemon start
```

## Components

| Component | Description |
|-----------|-------------|
| `nexus42` | CLI client |
| `nexus42d` | Local daemon |

## License

GPL-3.0-only
