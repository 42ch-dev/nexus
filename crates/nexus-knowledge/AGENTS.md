# nexus-knowledge — Reference Sources

Reference source management for creator knowledge. Handles the lifecycle
of reference materials (add, update, remove, query).

## Key Rules

- **Contracts-first**: all public types come from `nexus-contracts`. Do not
  hand-write duplicate ReferenceSource DTOs.

## Dependencies

- `nexus-contracts` (generated types)
