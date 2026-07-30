# nexus-narrative — Worlds, Forks, Timelines, Manuscripts

Narrative domain: worlds, timeline events, fork branches, story manifests,
manuscript state, and consistency checking.

## Key Rules

- **Contracts-first**: all public types come from `nexus-contracts`. Do not
  hand-write duplicate World, Timeline, Story, or Manuscript DTOs.
- World history is **immutable** — changes go through Fork, not in-place
  mutation.
- Depends on `nexus-knowledge` for knowledge entries and source anchors.

## Dependencies

- `nexus-contracts` (generated types)
- `nexus-knowledge` (knowledge entries, source anchors)
- `spoke-schemas` — the spoke standard `TimelineEvent` wire type (V1.143 P0 T1). The two `From` impls in `timeline_event.rs` are the sole L5 temporal-axis conversion seam (spec `spoke-adapter-architecture.md` §7.1).
- `spoke-operations` — standard spoke-library usage (leaf dep, no cycle): `InMemoryNarrativeGateway::get_timeline_ordered` calls the spoke `order_timeline_events_by_ids` beat-assist ordering helper. This is the same kind of dep as depending on `serde`, not a spoke-adapter aggregation concern. (V1.145 P1b: came directly from `spoke-operations` after the `nexus-spoke-adapter → nexus-local-db` dep reversal; previously it arrived via `nexus-spoke-adapter`.) Routing that ordering back through the spoke-adapter boundary is a V1.146 refactor. See spec §7.4 "Read-path ScopeQuery adoption".
