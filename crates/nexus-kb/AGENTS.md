# nexus-kb — Key Blocks + Source Anchors

Narrative key blocks and source anchors. Provides structured knowledge
extraction primitives used by the narrative crate and context assembly.

## Key Rules

- **Contracts-first**: all public types come from `nexus-contracts`. Do not
  hand-write duplicate KeyBlock or SourceAnchor DTOs.

## Dependencies

- `nexus-contracts` (generated types)
