# nexus-creator-memory — Memory Pipeline, SOUL I/O

Creator memory pipeline: long-term memory, experience aggregation,
personality sync, review quality tracking, and SOUL file I/O.

## Key Rules

- **Contracts-first**: all public types come from `nexus-contracts`. Do not
  hand-write duplicate memory or personality DTOs.
- Depends on `nexus-creator` for the creator aggregate.
- SOUL I/O reads/writes the SOUL file under the creator's home layout.

## Dependencies

- `nexus-creator` (aggregate root)
- `nexus-contracts` (generated types)
- `nexus-home-layout` (`~/.nexus42/` paths)
