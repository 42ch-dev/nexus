/**
 * Timeline canvas adapter — V1.122 P1 T2 (projection) + T4 (write boundary).
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
 * Write boundary (T4 — architect-locked §4): the Timeline surface edits
 * World-scoped KeyBlock entities through `NexusClient.worldKbPatchEntity`
 * (the V1.73 `POST .../kb/patch-entity` route) ONLY. The orchestrator owns
 * the React Query mutation hook; the adapter receives the write callback via
 * `TimelineCanvasAdapterContext` (mirrors the V1.114 World KB ctxRef pattern).
 * Forbidden in V1.122: `timeline.patch_event` (Work-scoped),
 * `world_kb.patch_relationship` (relationships read-only on Timeline),
 * `kb.promote_candidate` (World KB surface), raw-file writes.
 *
 * Conflict policy (T4 — architect-locked §5): reuses `WorldKbConflictError`
 * (HTTP 409) + `WorldKbValidationError` (HTTP 422); no Timeline-specific
 * conflict DTO. `extractConflict` parses the canonical `ErrorResponse`
 * envelope into a `TimelineConflictInfo`; `adaptConflict` (the inherited
 * interface method) stays `null` — the modal is orchestrator-owned, mirroring
 * the World KB adapter (the Strategy-specific `ConflictModalProps` return type
 * does not fit world-kb-flavored conflicts).
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
import { TimelineInspector } from './timeline-inspector';
import { TimelineAltView } from './timeline-alt-view';
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
 * Patchable fields on a `WorldKbEntityProjection` (V1.73 `WorldKbEntityPatch`).
 * The Timeline surface's write boundary is limited to these four fields via
 * `kb.patch_entity` (architect-locked §4.1).
 */
export type TimelinePatchField = 'title' | 'body' | 'aliases' | 'block_type';

/**
 * The patch payload the Timeline adapter emits — a subset of the V1.73
 * `WorldKbEntityPatch` wire shape. The field set is identical so the
 * orchestrator can forward `patch` straight into `worldKbPatchEntity` without
 * a remap. At least one property is required by the schema; the
 * orchestrator's submit handler skips no-op patches.
 */
export type TimelineEntityPatch = {
  title?: string;
  body?: Record<string, unknown>;
  aliases?: string[];
  block_type?: WorldKbEntityProjection['block_type'];
};

/**
 * Structured conflict info extracted from a daemon `ErrorResponse` that
 * matches the V1.73 `WorldKbConflictError` (409) or `WorldKbValidationError`
 * (422) detail shape. The orchestrator renders the existing
 * `WorldKbEntityConflictModal` (world-kb-flavored copy) from this info — no
 * Timeline-specific conflict DTO is introduced (architect-locked §5).
 */
export type TimelineConflictInfo =
  | {
      kind: 'conflict';
      /** Canonical version the daemon now holds (OCC). */
      currentVersion: number;
      /** Entity id the patch targeted. */
      entityId: string;
      /** Field path the daemon reports as conflicting (free-form). */
      conflictingPath: string;
      /** The user's pending patch (kept so "Reapply" can re-submit). */
      draftPatch: TimelineEntityPatch;
      /** Fields the user touched (drives overlap detection in the modal). */
      dirtyFields: TimelinePatchField[];
    }
  | {
      kind: 'validation';
      /** Field-level validation messages from `validation_summary.errors[]`. */
      errors: string[];
    };

/**
 * Mutable context supplied by the orchestrator so the adapter can render
 * inspectors / wire write operations without closing over stale values. Read
 * at render time from the ref; the adapter object itself stays stable across
 * renders (V1.114 §3.3.1 "stable factory that reads from a mutable
 * `React.RefObject` context").
 *
 * T2 shipped the minimal shape — `worldId` + an optional `client` slot for the
 * T2 isolation test. T4 extends this with the write callbacks the inspector
 * routes through:
 *   - `onPatchEntity` — the ONLY legitimate write path. The orchestrator
 *     wires it to `usePatchWorldKbEntity(worldId).mutate(...)`, which calls
 *     `client.worldKbPatchEntity(worldId, request)` (V1.73). It MUST NOT be
 *     wired to `client.patchTimelineEvent` (Work-scoped) or any other surface.
 *   - `onConflict` — fired when the daemon returns 409 / 422 so the
 *     orchestrator can open the world-kb-flavored conflict modal and refetch
 *     the canonical graph.
 *
 * The `client` slot stays for forward-compatibility and for the write-
 * boundary isolation tests (T2/T4) to assert negative invocation.
 */
export interface TimelineCanvasAdapterContext {
  worldId: string;
  /**
   * Optional client reference. T2 does NOT invoke any client method from
   * `projectGraph` / `summarizeGraph` (the projection is a pure function of
   * the graph). T4 routes writes through `onPatchEntity` (which the
   * orchestrator wires to `usePatchWorldKbEntity`) — the client object is
   * not read directly from this slot at write time. The slot exists so the
   * T2/T4 isolation tests can assert negative invocation against every
   * forbidden method on a single mocked client.
   */
  client?: unknown;
  /**
   * Write callback — routes a Timeline patch through
   * `NexusClient.worldKbPatchEntity` (V1.73) only. The orchestrator owns the
   * React Query mutation; the adapter calls this from the inspector's submit
   * handler. Undefined in T2 (projection-only) and in tests that don't wire
   * writes.
   */
  onPatchEntity?: (
    node: Node<TimelineNodeData>,
    patch: TimelineEntityPatch,
    dirtyFields: TimelinePatchField[],
  ) => void;
  /**
   * Conflict hand-off — fired by the orchestrator's mutation `onError` when
   * the daemon returns 409 / 422. The orchestrator renders the
   * world-kb-flavored conflict modal from the structured info and refetches
   * the canonical graph. Undefined when no write hook is wired.
   */
  onConflict?: (info: TimelineConflictInfo) => void;
  /**
   * Projected Timeline nodes (T5 — alt-view companion). The orchestrator
   * supplies the post-projection, post-layout node array so the alt-view
   * table reads the same rows the canvas renders. Mirrors the V1.114 World
   * KB ctxRef pattern (`ctx.nodes`). Optional in T2/T4 tests that don't
   * render the alt-view; the wrapper treats `undefined` as an empty list.
   */
  nodes?: Node<TimelineNodeData>[];
  /**
   * Currently-selected node id (T5 — alt-view row highlight). The
   * orchestrator owns selection state (it surfaces from `useCanvasSurface`)
   * and passes the id through so the alt-view highlights the matching row.
   */
  selectedNodeId?: string | null;
  /**
   * Selection hand-off (T5 — alt-view → inspector). Fires when the user
   * clicks / keyboard-activates an alt-view row. The orchestrator selects
   * the matching React Flow node so the inspector that owns the
   * `kb.patch_entity` write path opens. The alt-view performs NO writes
   * itself (architect-locked §4.2 — selection-only, inspector-owned writes).
   */
  onSelectNode?: (nodeId: string) => void;
}

export type TimelineCanvasAdapter = CanvasSurfaceAdapter<
  TimelineGraph,
  TimelineNodeData,
  TimelineEdgeData
>;

// ─── Projection constants ───────────────────────────────────────────────────

/**
 * Initial-position metrics. The adapter sets `layoutOptions.hasSuppliedPositions`
 * (T4 — Batch 1 reviewer note), so `useAutoLayout` honors these positions on
 * first open and does NOT collapse the chronology onto dagre's generic graph
 * layout. The author can still trigger an explicit `relayout()` to re-run
 * dagre LR (e.g. when the graph grows too dense to read on the supplied
 * lanes).
 *
 * The metrics encode a deliberate three-lane layout:
 *   - WHEN_AXIS_Y (0)        — dated events, sorted left→right by `occurred_at`.
 *   - CONTEXT_CLUSTER_Y (-)  — non-event entities (characters / scenes / ...).
 *   - TEMPORAL_UNKNOWN_Y (+) — events whose body lacks `occurred_at`.
 *
 * `simplify:` deterministic lane metrics mirroring the World KB adapter's
 * `LANE_X` / `ROW_Y` constants. Replace with a temporal-aware layout plugin
 * if the three-lane scheme stops scaling (e.g. > 50 dated events need a
 * scroll-snap calendar rail rather than a wider X axis).
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
 * T4 extensions over T2:
 *   - `layoutOptions.hasSuppliedPositions = true` — preserves the temporal-
 *     sorted node positions on first open so dagre LR does NOT collapse the
 *     chronology onto a generic graph layout (Batch 1 reviewer note). An
 *     explicit `relayout()` remains available via `useCanvasSurface`.
 *   - `renderInspector(node)` — renders the inline title/body editor via
 *     `TimelineInspector`, which routes the patch through
 *     `ctxRef.current.onPatchEntity` (the orchestrator-owned write callback).
 *     The inspector receives the full ctxRef so it stays decoupled from
 *     stale closures.
 *   - `adaptConflict` is intentionally `null` — the inherited return type
 *     (`ConflictModalProps`) is Strategy-specific and does not fit world-kb-
 *     flavored conflicts. Conflict UX is orchestrator-owned (mirrors World
 *     KB). Use `extractConflict(error)` for the typed parse.
 *
 * T5 extensions over T4:
 *   - `renderAltView()` — renders the non-spatial sortable Timeline entity
 *     table via `TimelineAltView` (mirrors the V1.114 World KB alt-view
 *     pattern). Reads projected nodes + selection from the ctxRef. The
 *     alt-view is selection-only: row click / Enter fires
 *     `ctxRef.current.onSelectNode(nodeId)`, which the orchestrator routes
 *     to a React Flow selection. The inspector that opens as a result owns
 *     the `kb.patch_entity` write — the alt-view itself performs NO writes
 *     (architect-locked §4.2 — `timeline.patch_event` is forbidden).
 *   - `summarizeGraph(graph)` — verified/strengthened for the a11y live
 *     region: non-empty for every graph state (empty + dense + partial
 *     temporal signal); emits the ordering disclaimer whenever temporal
 *     signals are partial OR absent (Batch 1 shipped this; T5 re-tests it).
 */
export function createTimelineCanvasAdapter(
  ctxRef: MutableRefObject<TimelineCanvasAdapterContext>,
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
    layoutOptions: { direction: 'LR', hasSuppliedPositions: true },

    projectGraph(graph) {
      return projectTimelineGraph(graph);
    },

    adaptConflict(_error) {
      // Orchestrator-owned — see `extractConflict` + module doc. Returning
      // null is consistent with the V1.74 World KB adapter: the Strategy-
      // specific `ConflictModalProps` shape does not fit world-kb-flavored
      // conflicts, so the orchestrator renders `WorldKbEntityConflictModal`
      // directly from the structured info.
      return null;
    },

    renderInspector(node) {
      return <TimelineInspector node={node} ctxRef={ctxRef} />;
    },

    renderAltView() {
      return <TimelineAltViewWrapper ctxRef={ctxRef} />;
    },

    summarizeGraph(graph) {
      return summarizeTimelineGraph(graph);
     },
  };
}

// ─── Alt-view wrapper (T5 — non-spatial sortable table) ─────────────────────

/**
 * Adapter-driven alt view; reads the projected nodes + selection state from
 * the orchestrator-owned ctxRef at render time. Mirrors the V1.114 World KB
 * `WorldKbAltViewWrapper` recipe.
 *
 * Selection hand-off: row click / Enter invokes `ctxRef.current.onSelectNode`,
 * which the orchestrator wires to a React Flow node selection. The inspector
 * that opens as a result owns the `kb.patch_entity` write path — the alt-view
 * performs NO writes (architect-locked §4.2 — `timeline.patch_event` is
 * forbidden from this surface).
 */
function TimelineAltViewWrapper({
  ctxRef,
}: {
  ctxRef: MutableRefObject<TimelineCanvasAdapterContext>;
}) {
  const ctx = ctxRef.current;
  return (
    <TimelineAltView
      nodes={ctx.nodes ?? []}
      selectedNodeId={ctx.selectedNodeId ?? null}
      onSelectNode={(nodeId) => ctx.onSelectNode?.(nodeId)}
    />
  );
}

// ─── Conflict extraction (T4 — world-kb-flavored, reused DTOs) ──────────────

/**
 * Parsed shape of the canonical `ErrorResponse.details` payload the daemon
 * returns under `world_kb_conflict` (HTTP 409). Mirrors the V1.73 contract
 * reused verbatim (no Timeline-specific DTO).
 */
interface WorldKbConflictDetailsShape {
  current_version?: number;
  entity_id?: string;
  conflicting_path?: string;
  recovery_hint?: string;
}

/**
 * Parsed shape of the canonical `ErrorResponse.details` payload the daemon
 * returns under `world_kb_validation_failed` (HTTP 422).
 */
interface WorldKbValidationDetailsShape {
  validation_summary?: { errors?: string[] };
}

/**
 * The two daemon error codes the Timeline surface recognises. Both are V1.73
 * reused verbatim — no new code is introduced.
 */
type WorldKbErrorCode =
  | 'world_kb_conflict'
  | 'world_kb_validation_failed';

interface NexusClientErrorLike {
  status?: number;
  code?: string;
  details?: unknown;
}

/**
 * Narrow an unknown error into a recognisable NexusClient error shape. Kept
 * loose (no `instanceof` coupling) so the extractor is pure and trivially
 * testable against plain object fixtures.
 */
function asNexusClientError(error: unknown): NexusClientErrorLike | null {
  if (typeof error !== 'object' || error === null) return null;
  const e = error as Record<string, unknown>;
  if (typeof e.status !== 'number' && typeof e.code !== 'string') return null;
  return {
    status: typeof e.status === 'number' ? e.status : undefined,
    code: typeof e.code === 'string' ? (e.code as WorldKbErrorCode) : undefined,
    details: e.details,
  };
}

/**
 * Project a daemon error into a `TimelineConflictInfo`, reusing the V1.73
 * `WorldKbConflictError` (409) + `WorldKbValidationError` (422) DTOs
 * verbatim (architect-locked §5 — no Timeline-specific conflict DTO).
 *
 * Returns `null` for every other error shape (500, dropped network, etc.) —
 * those are surfaced as toasts by the orchestrator's mutation `onError`,
 * matching the V1.73 World KB path.
 *
 * Pure: no React, no side effects. The orchestrator renders the modal from
 * the structured info; this function only narrows the error.
 */
export function extractTimelineConflict(
  error: unknown,
  context?: {
    /** The patch the user was attempting (kept so "Reapply" can re-submit). */
    draftPatch?: TimelineEntityPatch;
    /** Fields the user touched (drives overlap detection in the modal). */
    dirtyFields?: TimelinePatchField[];
  },
): TimelineConflictInfo | null {
  const parsed = asNexusClientError(error);
  if (!parsed) return null;

  if (parsed.status === 409 && parsed.code === 'world_kb_conflict') {
    const details = (parsed.details ?? {}) as WorldKbConflictDetailsShape;
    return {
      kind: 'conflict',
      currentVersion:
        typeof details.current_version === 'number'
          ? details.current_version
          : 0,
      entityId: details.entity_id ?? '',
      conflictingPath: details.conflicting_path ?? '',
      draftPatch: context?.draftPatch ?? {},
      dirtyFields: context?.dirtyFields ?? [],
    };
  }

  if (parsed.status === 422 && parsed.code === 'world_kb_validation_failed') {
    const details = (parsed.details ?? {}) as WorldKbValidationDetailsShape;
    const errors = details.validation_summary?.errors ?? [];
    return {
      kind: 'validation',
      errors: errors.length > 0 ? errors : [],
    };
  }

  return null;
}
