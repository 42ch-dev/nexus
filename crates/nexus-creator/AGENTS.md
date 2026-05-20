# nexus-creator — Creator Aggregate + Local Identity

Creator aggregate root and local identity management. Provides the local
creator profile, challenge-response flows, and identity verification logic
used by the CLI.

## Key Rules

- **Contracts-first**: all public types come from `nexus-contracts`. Do not
  hand-write duplicate Creator or Identity DTOs.
- Local-only crate — no cloud/HTTP transport. Cloud registration lives in
  `nexus-cloud-sync::platform_client`.

## Dependencies

- `nexus-contracts` (generated types)
- `nexus-home-layout` (`~/.nexus42/` paths)
