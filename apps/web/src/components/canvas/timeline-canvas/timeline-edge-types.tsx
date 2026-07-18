/**
 * Timeline canvas edge types — V1.122 P1 T2.
 *
 * The Timeline surface introduces NO new edge DTOs. Edges reuse the V1.74
 * `WorldKbEdgeData` verbatim (see `timeline-canvas-adapter.tsx::TimelineEdgeData`).
 * The architect lock (§11 Non-goals) forbids `ForeshadowEdge` /
 * `RealizesEdge` / `ForkFromEdge` — those are Work-outline projection labels,
 * not World-scoped relationship DTOs.
 *
 * T2 ships NO bespoke edge components. The default React Flow renderer
 * already surfaces the label + stroke styling emitted by
 * `deriveTimelineEdges` (the adapter mirrors the World KB stroke-color +
 * suggested-suffix recipe). `adapter.edgeTypes` is therefore `undefined`,
 * matching the World KB adapter precedent — RF's default rendering applies.
 *
 * This module exists as the forward-compatible hook: when a richer
 * Timeline relationship renderer becomes post-MVP justified (alongside the
 * deferred `world_kb.patch_relationship` write path), it lands here without
 * touching the adapter file. The forbidden-kinds constant below is the
 * single place the §11 lock is named; the unit test asserts against it so
 * a future contributor cannot silently register a Work-outline edge kind.
 */
import type { EdgeTypes } from '@xyflow/react';

/**
 * Reserved slot for a Timeline relationship edge component. Intentionally
 * empty for V1.122 P1 T2 — see module doc.
 */
export const timelineEdgeTypes: EdgeTypes = {};

/**
 * Forbidden edge kinds — the architect lock (architecture spec §11) pins
 * these as Work-outline-only projection labels. The unit test asserts none
 * of them appear in any future extension of `timelineEdgeTypes`.
 */
export const FORBIDDEN_TIMELINE_EDGE_KINDS = [
  'foreshadow',
  'realizes',
  'fork-from',
  'forkFrom',
] as const;
