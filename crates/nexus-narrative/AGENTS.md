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
- (V1.146 P1) `spoke-operations` is **no longer** a dependency: the former `InMemoryNarrativeGateway::get_timeline_ordered` (which called the spoke `order_timeline_events_by_ids` ordering helper) was removed — it never had a production call site, and the ordered-timeline facet now lives on the spoke-adapter boundary as `NexusBaselineAdapter::list_timeline_events_ordered` in `nexus-spoke-adapter` (spec §7.4).
