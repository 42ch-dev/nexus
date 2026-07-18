/**
 * Timeline canvas adapter — V1.122 P1 T2.
 *
 * Projects a World's `WorldKbGraphResponse` onto a left-to-right when-axis
 * (the World-building hero surface, `CanvasSurfaceKind = "timeline"`).
 *
 * Architect-locked contract — see
 * `iterations/v1.122/specs/timeline-canvas-architecture.md` §2-§7:
 *   - Single graph source: `WorldKbGraphResponse` (V1.73 shipped). No wrapper,
 *     no join with other DTOs (`TimelineGraph = WorldKbGraphResponse`).
 *   - `block_type=event` entities → `TimelineEventNode` on the when-axis,
 *     positioned by `body.attributes.occurred_at` (free-form) when present.
 *     Events without a temporal signal cluster in a temporal-unknown group
 *     with honest copy. The adapter MUST NOT fabricate chronology from
 *     `updated_at`, `canonical_name`, `version`, or `sequence_no`.
 *   - Other entity kinds → `TimelineKeyBlockNode` (Context cluster) off-axis.
 *   - `relationships[]` → `Edge<TimelineEdgeData>` reusing `WorldKbEdgeData`
 *     verbatim (V1.74). No `ForeshadowEdge` / `RealizesEdge` / `ForkFromEdge`.
 *   - No Fork marker nodes (Fork data renders as optional header chrome in T3).
 *
 * Write boundary: T2 ships the READ projection only.
 * `worldKb.patchEntity` (T4) is the sole write path; `timeline.patch_event`
 * (Work-scoped) and `world_kb.patch_relationship` MUST NOT be invoked from
 * this surface in V1.122.
 *
 * `wire_contracts_changed: false` — the adapter reuses 12 shipped DTOs/routes
 * verbatim; only the frontend enum + this module are new.
 */
import type { MutableRefObject } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import type { WorldKbEdgeData } from '../world-kb/types';
import { timelineNodeTypes } from './timeline-node-types';

// ─── Public types (architect-locked §3.1) ───────────────────────────────────

/** Single graph source — no wrapper, no join. */
export type TimelineGraph = WorldKbGraphResponse;

/**
 * Node data payload for the Timeline surface.
 *
 * `WorldKbEntityProjection` carries `key_block_id`, `block_type`,
 * `canonical_name`, `status`, `version`, `body`, `source_anchor_count`, etc.
 * The adapter adds a `layoutHint` ('event' for `block_type=event`, 'context'
 * otherwise) and an optional `occurredAtHint` extracted from
 * `body.attributes.occurred_at`.
 *
 * The `[key: string]: unknown` index signature satisfies React Flow's
 * `Node<TNodeData extends Record<string, unknown>>` constraint.
 */
export interface TimelineNodeData extends WorldKbEntityProjection {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  /**
   * 'event' when `block_type === 'event'` (entity-scope-model §5.1.1).
   * 'context' for all other BlockType values.
   */
  layoutHint: 'event' | 'context';
  /**
   * Free-form temporal signal extracted from `body.attributes.occurred_at`
   * when it is a non-empty string. Undefined when not declared by the
   * KeyBlock body — the entity then clusters in the temporal-unknown group.
   */
  occurredAtHint?: string;
}

/** Verbatim reuse of the V1.74 World KB relationship edge payload. */
export type TimelineEdgeData = WorldKbEdgeData;

// ─── Adapter context ────────────────────────────────────────────────────────

/**
 * Mutable context supplied by the orchestrator so the adapter can render
 * inspectors / alt-view / wire write operations (T4) without closing over
 * stale values. Read at render time from the ref; the adapter object itself
 * stays stable across renders (V1.114 §3.3.1 "stable factory that reads from
 * a mutable `React.RefObject` context").
 *
 * T2 ships the minimal shape — `worldId` for projection + an optional
 * `client` slot the T2 write-boundary isolation test uses for negative
 * assertions. T4 will extend this with `patchEntity` callbacks + conflict
 * handlers when it wires the legitimate `worldKb.patchEntity` write path.
 */
export interface TimelineCanvasAdapterContext {
  worldId: string;
  /**
   * Optional client reference. T2 does NOT invoke any client method from
   * `projectGraph` / `summarizeGraph` / `adaptConflict` (the projection is a
   * pure function of the graph). The slot exists so T4 can extend the
   * adapter with `patchEntity` routing without changing the factory
   * signature, AND so the T2 isolation test can assert non-invocation.
   */
  client?: unknown;
}

export type TimelineCanvasAdapter = CanvasSurfaceAdapter<
  TimelineGraph,
  TimelineNodeData,
  TimelineEdgeData
>;

// ─── Projection constants ───────────────────────────────────────────────────

/**
 * Initial-position metrics. `useAutoLayout` runs dagre LR on first open
 * (the adapter does not set `hasSuppliedPositions`), so these are starting
 * hints dagre refines. The temporal sort + axis separation is what survives
 * dagre — events with `occurred_at` feed the LR rank, others fall to a
 * temporal-unknown cluster, and Context entities cluster off-axis.
 *
 * `simplify:` these are deterministic lane metrics mirroring the World KB
 * adapter's `LANE_X` / `ROW_Y` constants; replace with a temporal-aware
 * layout plugin if dagre LR ever stops surfacing chronology acceptably.
 */
const WHEN_AXIS_Y = 0;
const CONTEXT_CLUSTER_Y = -220;
const TEMPORAL_UNKNOWN_Y = 220;
const ORIGIN_X = 40;
const EVENT_STEP_X = 280;
const CONTEXT_STEP_X = 220;

const NODE_ID_PREFIX = 'entity:';

function nodeIdOf(keyBlockId: string): string {
  return `${NODE_ID_PREFIX}${keyBlockId}`;
}

/**
 * Extract the temporal signal from a KeyBlock body. Per the architect lock,
 * ONLY `body.attributes.occurred_at` (free-form string) is honored — never
 * `updated_at`, `canonical_name`, `version`, or `sequence_no`. Non-string
 * values and empty strings are treated as absent.
 *
 * Type narrowing: `WorldKbEntityProjection.body` is `Record<string, unknown>`;
 * `body.attributes` is therefore `unknown`. We narrow via `typeof object`
 * before reading `occurred_at` so the access is type-safe.
 */
function occurredAtOf(entity: WorldKbEntityProjection): string | undefined {
  const attrs = entity.body?.attributes;
  if (attrs === null || typeof attrs !== 'object') return undefined;
  const raw = (attrs as Record<string, unknown>).occurred_at;
  return typeof raw === 'string' && raw.length > 0 ? raw : undefined;
}

/**
 * Project the entities + relationships of a `WorldKbGraphResponse` onto
 * Timeline nodes + edges.
 *
 * Positioning rules:
 *   - Dated events (`body.attributes.occurred_at` present) are placed along
 *     the when-axis (Y = 0) sorted lexicographically by timestamp (ISO 8601
 *     sorts chronologically when the format is consistent).
 *   - Undated events cluster below the when-axis (temporal-unknown group).
 *   - Non-event entities (Context) cluster above the when-axis.
 *   - Node ids are stable across refetches (`entity:${key_block_id}`).
 *
 * Relationship edges reuse the V1.74 World KB edge rendering verbatim
 * (`WorldKbEdgeData`); both the stored + symmetric_reverse projections are
 * rendered when a relationship is symmetric, mirroring the World KB adapter.
 */
export function projectTimelineGraph(graph: TimelineGraph): {
  nodes: Node<TimelineNodeData>[];
  edges: Edge<TimelineEdgeData>[];
} {
  const entities = graph.entities ?? [];
  const relationships = graph.relationships ?? [];

  const eventEntities: WorldKbEntityProjection[] = [];
  const contextEntities: WorldKbEntityProjection[] = [];
  for (const e of entities) {
    if (e.block_type === 'event') {
      eventEntities.push(e);
    } else {
      contextEntities.push(e);
    }
  }

  // Split events by temporal signal; sort dated events chronologically.
  const datedEvents: Array<{ entity: WorldKbEntityProjection; occurredAt: string }> = [];
  const undatedEvents: WorldKbEntityProjection[] = [];
  for (const e of eventEntities) {
    const occurredAt = occurredAtOf(e);
    if (occurredAt === undefined) {
      undatedEvents.push(e);
    } else {
      datedEvents.push({ entity: e, occurredAt });
    }
  }
  datedEvents.sort((a, b) => {
    if (a.occurredAt < b.occurredAt) return -1;
    if (a.occurredAt > b.occurredAt) return 1;
    // Stable tiebreaker on key_block_id so identical timestamps are
    // deterministic across refetches.
    return a.entity.key_block_id.localeCompare(b.entity.key_block_id);
  });

  const nodes: Node<TimelineNodeData>[] = [];

  datedEvents.forEach(({ entity, occurredAt }, i) => {
    nodes.push({
      id: nodeIdOf(entity.key_block_id),
      type: 'timeline-event',
      position: { x: ORIGIN_X + i * EVENT_STEP_X, y: WHEN_AXIS_Y },
      data: entityToTimelineNodeData(entity, 'event', occurredAt),
    });
  });

  // Undated events cluster below the when-axis (temporal-unknown group).
  // Their X spread continues from the rightmost dated event so the cluster
  // reads as "and these happened, we don't know when."
  const undatedOriginX = ORIGIN_X + datedEvents.length * EVENT_STEP_X;
  undatedEvents.forEach((entity, i) => {
    nodes.push({
      id: nodeIdOf(entity.key_block_id),
      type: 'timeline-event',
      position: { x: undatedOriginX + i * EVENT_STEP_X, y: TEMPORAL_UNKNOWN_Y },
      data: entityToTimelineNodeData(entity, 'event', undefined),
    });
  });

  // Context entities cluster above the when-axis (off-axis).
  contextEntities.forEach((entity, i) => {
    nodes.push({
      id: nodeIdOf(entity.key_block_id),
      type: 'timeline-key-block',
      position: { x: ORIGIN_X + i * CONTEXT_STEP_X, y: CONTEXT_CLUSTER_Y },
      data: entityToTimelineNodeData(entity, 'context', undefined),
    });
  });

  const edges = deriveTimelineEdges(relationships);

  return { nodes, edges };
}

function entityToTimelineNodeData(
  entity: WorldKbEntityProjection,
  layoutHint: 'event' | 'context',
  occurredAt: string | undefined,
): TimelineNodeData {
  return {
    ...entity,
    layoutHint,
    ...(occurredAt !== undefined ? { occurredAtHint: occurredAt } : {}),
  };
}

/**
 * Derive Timeline edges from the graph's relationship projections.
 *
 * Reuses the V1.74 `WorldKbEdgeData` shape verbatim (`relationType:
 * 'relationship'`, `sourceAnchorIds`, `confidence`, `needsReview`,
 * `source`). The backend's `project_relationships_for_world` already swaps
 * source/target when emitting the `symmetric_reverse` projection, so we
 * consume `source_entity_id` / `target_entity_id` verbatim — same contract
 * as `world-kb/relationship-projection.ts::deriveRelationshipEdges`.
 */
export function deriveTimelineEdges(
  relationships: WorldKbRelationshipProjection[],
): Edge<TimelineEdgeData>[] {
  return relationships.map((rel) => {
    const data: TimelineEdgeData = {
      relationType: 'relationship',
      sourceAnchorIds: rel.source_anchor_ids ?? [],
      confidence: rel.confidence,
      needsReview: rel.needs_review ?? false,
      source: rel.source,
    };
    const label = relationshipLabel(rel);
    const strokeColor =
      rel.relation_type === 'custom'
        ? 'var(--color-canvas-worldkb-relationship-edge-custom)'
        : rel.symmetric
          ? 'var(--color-canvas-worldkb-relationship-edge-symmetric)'
          : 'var(--color-canvas-worldkb-relationship-edge-default)';
    const labelText = data.needsReview ? `${label} · suggested` : label;
    return {
      id: `relationship:${rel.relationship_id}:${rel.projection_direction}`,
      source: nodeIdOf(rel.source_entity_id),
      target: nodeIdOf(rel.target_entity_id),
      type: 'default',
      label: labelText,
      data,
      selectable: true,
      focusable: true,
      style: { stroke: strokeColor },
    } satisfies Edge<TimelineEdgeData>;
  });
}

const RELATIONSHIP_KIND_LABELS: Record<string, string> = {
  allied_with: 'Allied With',
  opposes: 'Opposes',
  parent_of: 'Parent Of',
  child_of: 'Child Of',
  member_of: 'Member Of',
  located_in: 'Located In',
  rules_over: 'Rules Over',
  references: 'References',
  serves: 'Serves',
  rival_of: 'Rival Of',
  mentor_of: 'Mentor Of',
  custom: 'Custom',
};

/**
 * Edge label — core kind Title Cased, or the custom label verbatim. Mirrors
 * the World KB helper; inlined here so the Timeline adapter stays
 * dependency-free of the World KB relationship module (T2 ships no new
 * public dependency on a sibling surface — projection reuses DTOs only).
 */
function relationshipLabel(rel: WorldKbRelationshipProjection): string {
  if (rel.relation_type === 'custom' && rel.custom_label) return rel.custom_label;
  return RELATIONSHIP_KIND_LABELS[rel.relation_type] ?? rel.relation_type;
}

// ─── Honest summary (architect §7) ──────────────────────────────────────────

const ORDERING_DISCLAIMER =
  'Ordering inferred from available event data; not a canonical chronology.';

/**
 * Build the screen-reader live-region summary for the Timeline canvas.
 *
 * The disclaimer MUST appear whenever temporal signals are partial OR absent
 * — i.e. when ANY event entity lacks `body.attributes.occurred_at`, OR when
 * there are zero events at all (sparse World empty-state). The disclaimer
 * is omitted only when EVERY event carries `occurred_at`.
 *
 * `simplify:` plain English (no i18n) — same convention as the World KB
 * `graphSummary`. The canvas a11y summary is an SR-only live region, not a
 * visible label; the World KB precedent follows this rule. If a future
 * iteration localises the canvas a11y summary, mirror the change here and
 * in `world-kb/graph-projection.ts::graphSummary`.
 */
export function summarizeTimelineGraph(graph: TimelineGraph): string {
  const entities = graph.entities ?? [];
  const relationships = graph.relationships ?? [];
  const anchors = graph.source_anchors ?? [];

  const events = entities.filter((e) => e.block_type === 'event');
  const contextCount = entities.length - events.length;
  const datedEvents = events.filter((e) => occurredAtOf(e) !== undefined);
  const undatedEventCount = events.length - datedEvents.length;

  const parts: string[] = [];
  parts.push(
    `${events.length} ${events.length === 1 ? 'event' : 'events'}`,
  );
  parts.push(
    `${contextCount} ${contextCount === 1 ? 'context entity' : 'context entities'}`,
  );
  parts.push(
    `${relationships.length} ${relationships.length === 1 ? 'relationship' : 'relationships'}`,
  );
  if (anchors.length > 0) {
    parts.push(
      `${anchors.length} ${anchors.length === 1 ? 'source anchor' : 'source anchors'}`,
    );
  }

  // Time span — present only when at least one event carries occurred_at.
  // The span is the raw min/max of the hints; we do NOT claim a canonical
  // chronology (see the disclaimer).
  if (datedEvents.length > 0) {
    const hints = datedEvents
      .map((e) => occurredAtOf(e)!)
      .sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
    const earliest = hints[0];
    const latest = hints[hints.length - 1];
    if (earliest === latest) {
      parts.push(`temporal signal at ${earliest}`);
    } else {
      parts.push(`temporal span ${earliest} → ${latest}`);
    }
  }

  let summary = `Timeline: ${parts.join(', ')}.`;

  // Disclaimer — required when temporal signals are partial OR absent.
  if (undatedEventCount > 0 || events.length === 0) {
    summary = `${summary} ${ORDERING_DISCLAIMER}`;
  }

  return summary;
}

// ─── Stable factory ─────────────────────────────────────────────────────────

/**
 * Build a stable Timeline canvas adapter that reads mutable values from the
 * supplied context ref (V1.114 §3.3.1 "stable factory that reads from a
 * mutable `React.RefObject` context").
 *
 * The returned object MUST stay referentially stable across renders —
 * `useCanvasSurface` memoises on `adapter` and would otherwise re-project on
 * every orchestrator state change. The factory is therefore called once per
 * orchestrator mount (e.g. via `useMemo([], ...)` or a `useRef`).
 *
 * T2 ships the READ projection only — `projectGraph` + `summarizeGraph` are
 * pure functions of the supplied graph and do not consult the context. The
 * `ctxRef` parameter is a forward-compatible slot: T4 will route
 * `patchEntity` callbacks + `adaptConflict` resolution through it (mirroring
 * the World KB adapter), at which point the projection methods will read
 * `ctxRef.current` for write callbacks without changing this factory's
 * signature.
 */
export function createTimelineCanvasAdapter(
  _ctxRef: MutableRefObject<TimelineCanvasAdapterContext>,
): TimelineCanvasAdapter {
  return {
    surfaceKind: 'timeline',
    nodeTypes: timelineNodeTypes,
    // `edgeTypes` is intentionally undefined: V1.122 P1 T2 ships no bespoke
    // edge components. The default React Flow renderer surfaces the label +
    // stroke styling emitted by `deriveTimelineEdges`. The
    // `timeline-edge-types.tsx` module exists as a forward-compatible hook
    // for post-MVP edge components; the architect lock forbids
    // ForeshadowEdge / RealizesEdge / ForkFromEdge (Work-outline kinds).
    edgeTypes: undefined,
    layoutOptions: { direction: 'LR' },

    projectGraph(graph) {
      return projectTimelineGraph(graph);
    },

    adaptConflict(_error) {
      // T2 stub — conflict UX (WorldKbConflictError 409 +
      // WorldKbValidationError 422) is T4 scope. Returning null keeps the
      // orchestrator's conflict modal closed until T4 lands.
      return null;
    },

    // renderInspector / renderAltView are T3 / T5 scope. T2 ships the READ
    // projection only; the adapter interface leaves both optional.

    summarizeGraph(graph) {
      return summarizeTimelineGraph(graph);
    },
  };
}
