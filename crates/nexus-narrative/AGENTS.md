# nexus-narrative — Worlds, Forks, Timelines, Manuscripts

Narrative domain: worlds, timeline events, fork branches, story manifests,
manuscript state, and consistency checking.

## Key Rules

- **Contracts-first**: all public types come from `nexus-contracts`. Do not
  hand-write duplicate World, Timeline, Story, or Manuscript DTOs.
- World history is **immutable** — changes go through Fork, not in-place
  mutation.
- Depends on `nexus-kb` for key blocks and source anchors.

## Dependencies

- `nexus-contracts` (generated types)
- `nexus-kb` (key blocks, source anchors)
