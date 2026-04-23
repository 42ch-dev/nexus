# nexus-home-layout — Path Layout Helpers

Shared ADR-014 path layout helpers defining the `~/.nexus42/` directory structure (`creators/...`, `workspaces/...`, etc.).

## Key Rules

- This crate defines the canonical path structure for Nexus local state. All crates that touch the filesystem should use these helpers rather than hardcoding paths.
- Path layout follows ADR-014 — see `.agents/knowledge/` for the architecture decision record.
- `~/.nexus42/` is the user-local root. Do not use other locations (e.g. `~/.config/nexus42/` or XDG dirs) for Nexus data.

## Pre-release Note

Since the project is pre-1.0, the path layout may change without migration. When paths change, a simple re-init or wipe is acceptable.
