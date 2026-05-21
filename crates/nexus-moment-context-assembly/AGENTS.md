# nexus-moment-context-assembly — Per-moment Context Assembly

Per-moment context assembly for ACP sessions. Three strategies:

- **Stage-0** (default, local-only): assembles context from SOUL sections, long-term memories,
  fragment keywords, and the user prompt.
- **Moment assembly** (default, local-only): aggregates from all four local domains —
  creator memory, narrative state, World KB, and User knowledge. Primary entry point for V1.23+.
- **Stage-1** (`cloud-stage` feature): two-stage assembly that merges platform API response
  with local data.

## Key Rules

- **No cloud-sync dependency by default** — daemon builds use Stage-0 only.
- `cloud-stage` feature gate adds `nexus-cloud-sync` dependency and `TwoStageAssembly`.
- Token budget / truncation uses chars/4 heuristic (spec §9.3).
- Personality section is NEVER truncated.
- Moment assembly is the V1.23 primary entry point (`assemble_moment`).

## Dependencies

- `nexus-creator-memory` (for `LongTermMemory`)
- `nexus-contracts` (for `RuntimeMode`, local types)
- `nexus-narrative` (for `NarrativeGateway` — world state, timeline, events)
- `nexus-kb` (for `KbStore` — World-scoped key blocks)
- `nexus-knowledge` (for `KnowledgeStore` — User-scoped knowledge entries)
- `nexus-cloud-sync` (optional, behind `cloud-stage` feature)
