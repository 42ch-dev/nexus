# nexus-moment-context-assembly — Per-moment Context Assembly

Per-moment context assembly for ACP sessions. Two strategies:

- **Stage-0** (default, local-only): assembles context from SOUL sections, long-term memories,
  fragment keywords, and the user prompt.
- **Stage-1** (`cloud-stage` feature): two-stage assembly that merges platform API response
  with local data.

## Key Rules

- **No cloud-sync dependency by default** — daemon builds use Stage-0 only.
- `cloud-stage` feature gate adds `nexus-cloud-sync` dependency and `TwoStageAssembly`.
- Token budget / truncation uses chars/4 heuristic (spec §9.3).
- Personality section is NEVER truncated.

## Dependencies

- `nexus-creator-memory` (for `LongTermMemory`)
- `nexus-contracts` (for `RuntimeMode`, local types)
- `nexus-cloud-sync` (optional, behind `cloud-stage` feature)
