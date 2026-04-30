# nexus-home-layout — Path Layout Helpers

Defines the canonical `~/.nexus42/` directory structure per ADR-014.

## Key Rules

- All crates touching the filesystem must use these helpers — do not hardcode paths.
- `~/.nexus42/` is the only user-local root. Do not use `~/.config/nexus42/` or XDG dirs.

## Pre-release Note

Since pre-1.0, the path layout may change without migration. A re-init or wipe is acceptable when paths change.
