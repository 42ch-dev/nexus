/**
 * Timeline canvas adapter — V1.122 P1 T2 (projection) + T4 (write boundary)
 *  + V1.123 P1 T2 (Brief layer projection).
 *
 * Projects a World's `WorldKbGraphResponse` onto a left-to-right when-axis
 * (the World-building hero surface, `CanvasSurfaceKind = "timeline"`).
 *
 * Architect-locked contract — see
 * `iterations/v1.122/specs/timeline-canvas-architecture.md` §2-§7 +
 * `iterations/v1.123/specs/three-layer-architecture.md` §2 + §8:
 *   - Single graph source: `WorldKbGraphResponse` (V1.73 shipped). No wrapper,
 *     no join with other DTOs (`TimelineGraph = WorldKbGraphResponse`).
 *   - **Narrative layer (V1.122 preserved)**: `block_type=event` entities →
 *     `TimelineEventNode` on the when-axis, positioned by
 *     `body.attributes.occurred_at` (free-form) when present. Events without
 *     a temporal signal cluster in a temporal-unknown group with honest copy.
 *     The adapter MUST NOT fabricate chronology from `updated_at`,
 *     `canonical_name`, `version`, or `sequence_no`. Non-event, non-era
 *     entities → `TimelineKeyBlockNode` (Context cluster) off-axis.
 *   - **Brief layer (V1.123 P1 T2 — architect §2 + §8)**: `block_type=era`
 *     entities → `TimelineBriefEraNode` on the Brief when-axis, positioned by
 *     `body.attributes.start_hint` when present. Eras without `start_hint`
 *     cluster in a temporal-unknown group. Era markers carry `body.attributes.{
 *     era_id, start_hint, end_hint, world_summary}` for the compact era card
 *     + Brief-era inspector (Task 4). No relationship edges on Brief (layer-
 *     feel-differentiation.md §2.2 — minimal density, era sweep only).
 *   - `relationships[]` → `Edge<TimelineEdgeData>` reusing `WorldKbEdgeData`
 *     verbatim (V1.74) on the Narrative layer only. No `ForeshadowEdge` /
 *     `RealizesEdge` / `ForkFromEdge`.
 *   - No Fork marker nodes (Fork data renders as optional header chrome in T3).
 *
 * V1.123 layer model: `projectGraphForLayer(graph, 'brief' | 'narrative')`
 * selects the active layer. The default `projectGraph(graph)` delegates to
 * the adapter's active layer, which defaults to `'narrative'` for V1.122
 * backward compatibility (Task 3 wires Brief-default-on-World-entry).
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
 * `wire_contracts_changed: true` — attributable to Task 1's single additive
 * `BlockType = "era"` enum value (schema + Rust companion + codegen). V1.123
 * P1 Tasks 2+3 add zero wire diff: the adapter is a pure frontend filter
 * over the V1.73 `WorldKbGraphResponse` (architect §5).
 */
import type { MutableRefObject } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type {
  TimelineEventInfo,
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import type { WorldKbEdgeData } from '../world-kb/types';
import { TimelineInspector } from './timeline-inspector';
import { TimelineComputeInspector } from './timeline-compute-inspector';
import { TimelineBriefEraInspector } from './timeline-brief-era-inspector';
import { TimelineAltView } from './timeline-alt-view';
import type { BriefSpineConfig, DirectedAxisSpineNodeData, NarrativeSpineConfig } from './directed-axis-spine';
import { SPINE_Y_OFFSET } from './directed-axis-spine';
import { timelineNodeTypes } from './timeline-node-types';

// ─── Public types (architect-locked §3.1 + V1.123 §2/§8) ────────────────────

/** Single graph source — no wrapper, no join. */
export type TimelineGraph = WorldKbGraphResponse;

/**
 * V1.123 layer kind on the World Timeline surface.
 *
 * - `'brief'`     — world-shape era sweep (`block_type=era` KeyBlocks on the
 *                   Brief when-axis; minimal density; no relationship edges).
 * - `'narrative'` — V1.122 event timeline (`block_type=event` on the when-axis
 *                   + Context clusters + relationship edges).
 *
 * Architect-locked in `three-layer-architecture.md` §8 + `layer-feel-
 * differentiation.md` §2. The adapter's `projectGraphForLayer(graph, layer)`
 * selects the active projection; `projectGraph(graph)` delegates to the
 * adapter's active layer (default `'narrative'` for V1.122 backward compat).
 */
export type TimelineLayer = 'brief' | 'narrative';

/**
 * V1.147 P2 T3 — compute provenance carried by a merged Narrative compute
 * node (derived from a `TimelineEventInfo` row + the KB graph). App-local
 * payload; the node chrome + compute inspector render from it.
 *
 * The `TimelineEventInfo` wire type stays authoritative: `eventId` +
 * `moduleId`/`moduleVersion`/`runId`/`sourceKind` come from
 * `extensions.compute` (daemon-stamped provenance), `reportDigest` from the
 * event summary, and `affectedEntries` resolve `affected_key_block_ids`
 * against the KB graph at projection time.
 */
export interface ComputeNodePayload {
  /** Timeline event row id (`compute:<id>` node id prefix). */
  eventId: string;
  /** Module id from provenance (`extensions.compute.module_id`). */
  moduleId: string;
  /**
   * Module version at invocation time.
   */
  moduleVersion: string;
  /**
   * Module display name — resolved from the caller-supplied module registry
   * map at projection time (falls back to `moduleId` when the registry has
   * not loaded the module — honest for preset-path modules absent locally).
   */
  moduleName: string;
  /** Run correlation id — direct lane only (absent for preset-path nodes). */
  runId?: string;
  /**
   * Provenance source kind — `direct_invoke` vs `preset` when stamped. The
   * daemon stamps only these two today; unknown kinds resolve to
   * `undefined` (the UI falls back to the direct-lane label — `simplify:`
   * a new kind needs a catalog entry when one ships).
   */
  sourceKind?: 'direct_invoke' | 'preset';
  /** Module event summary — the report digest (e.g. the damage line). */
  reportDigest?: string;
  /** Affected KnowledgeEntries resolved against the KB graph by id. */
  affectedEntries: Array<{ id: string; title: string }>;
}

/**
 * Node data payload for the Timeline surface.
 *
 * `WorldKbEntityProjection` carries `key_block_id`, `block_type`,
 * `canonical_name`, `status`, `version`, `body`, `source_anchor_count`, etc.
 * The adapter adds:
 *   - `layoutHint` — discriminates the four projection kinds:
 *     `'event'`  for Narrative `block_type=event` (V1.122).
 *     `'context'` for Narrative non-event, non-era KeyBlocks (V1.122).
 *     `'brief'`  for Brief `block_type=era` (V1.123 P1 T2).
 *     `'compute'` for V1.147 P2 compute_result log events merged into the
 *     Narrative projection (V1.147 P2 T3). Compute nodes carry synthetic
 *     entity-projection fields (`key_block_id: "log:<event_id>"`,
 *     `block_type: 'event'`, `canonical_name` = event title fallback) purely
 *     to satisfy the shared `WorldKbEntityProjection` base — they are NOT KB
 *     entities and MUST NOT route to the `kb.patch_entity` write path (the
 *     compute inspector owns no patch wiring).
 *   - `occurredAtHint` — free-form temporal signal extracted from
 *     `body.attributes.occurred_at` when present (Narrative layer). Compute
 *     nodes set it to the event's ISO `created_at` (a machine timestamp, not
 *     a fabricated chronology).
 *   - `eraId` / `startHint` / `endHint` / `worldSummary` — V1.123 Brief-era
 *     markers extracted from `body.attributes` when `layoutHint === 'brief'`
 *     (architect §2.3 + §8).
 *   - `compute` — V1.147 P2 compute payload, present only when
 *     `layoutHint === 'compute'`.
 *
 * The `[key: string]: unknown` index signature satisfies React Flow's
 * `Node<TNodeData extends Record<string, unknown>>` constraint.
 */
export interface TimelineNodeData extends WorldKbEntityProjection {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  /**
   * Projection kind. `'event'` / `'context'` are the V1.122 Narrative
   * partitions; `'brief'` is the V1.123 P1 T2 Brief-era partition; `'compute'`
   * is the V1.147 P2 machine-written compute-result partition (Narrative
   * only — the Brief layer never projects compute nodes).
   */
  layoutHint: 'event' | 'context' | 'brief' | 'compute';
  /**
   * Free-form temporal signal extracted from `body.attributes.occurred_at`
   * when it is a non-empty string. Undefined when not declared by the
   * KeyBlock body — the entity then clusters in the temporal-unknown group.
   * Narrative layer only (V1.122). Compute nodes carry the machine
   * `created_at` of the log event instead.
   */
  occurredAtHint?: string;
  /**
   * V1.123 P1 T2 Brief-era markers. Present only when `layoutHint === 'brief'`.
   * Read from `body.attributes` per architect §2.3 + §8 (`era_id`,
   * `start_hint`, `end_hint`, `world_summary` — freeform). The Brief-era
   * node + Brief-era inspector (Task 4) consume these for the compact era
   * card chrome.
   */
  eraId?: string;
  /** Era start hint (`body.attributes.start_hint`). Used for LR positioning. */
  startHint?: string;
  /** Era end hint (`body.attributes.end_hint`). Surfaces in the time-span label. */
  endHint?: string;
  /** Optional world-shape summary line for the era card. */
  worldSummary?: string;
  /**
   * V1.147 P2 T3 — compute payload for merged compute_result nodes. Present
   * only when `layoutHint === 'compute'`. See {@link ComputeNodePayload}.
   */
  compute?: ComputeNodePayload;
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
   *
   * The callback MAY return a `Promise` that settles when the underlying
   * mutation resolves or rejects (the orchestrator wires it to
   * `usePatchWorldKbEntity().mutateAsync`). The inspector awaits the return
   * so it can reset its local `isSubmitting` flag in a `finally` block on
   * every outcome — success AND error (PR #156 fix: without this, a 409/422/
   * network failure left Save permanently disabled until the selection
   * changed). A synchronous `void` return is still accepted (read-only test
   * mounts); the inspector's `await` treats it as an already-settled
   * promise.
   */
  onPatchEntity?: (
    node: Node<TimelineNodeData>,
    patch: TimelineEntityPatch,
    dirtyFields: TimelinePatchField[],
  ) => Promise<void> | void;
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
  /**
   * V1.123 P3 Task 4 — bound Work id (the Work that realizes this World).
   * When present, the World Timeline event inspector surfaces a "View in
   * Work Timeline" affordance that navigates to the realizing Work's
   * Timeline surface. Derived client-side by the orchestrator (uses
   * `useWorks()` + per-Work `getWork()` fan-out — capped at N=20 most-recent
   * Works; `simplify:` ceiling documented in the orchestrator).
   *
   * Undefined when no Work realizes the World (the orchestrator hides the
   * affordance — honest scope cut per plan §"If binding is missing or
   * unreliable, P3 hides the affordance").
   */
  boundWorkId?: string;
  /**
   * V1.123 P3 Task 4 — cross-surface navigation hand-off. Fires when the
   * user clicks "View in Work Timeline" on a World Timeline Narrative event.
   * The orchestrator navigates to `/works/:workId/timeline?layer=narrative`
   * (Moment layer one click away per plan §"Cross-surface navigation URL
   * contract"). Undefined when no realizing Work is bound.
   */
  onViewInWorkTimeline?: () => void;
  /**
   * V1.147 P2 T3 — Open Run hand-off from the compute node inspector.
   * Fires with the full run id + module id when the author activates "Open
   * Run" on a Compute result node; the orchestrator navigates to
   * Settings → Modules run detail (deep link). Undefined when the compute
   * inspector is read-only (tests without wiring).
   */
  onOpenRun?: (runId: string, moduleId: string) => void;
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

/**
 * V1.123 P1 T4 — per-layer dagre layout options.
 *
 * `layer-feel-differentiation.md` §2.2 locks the Brief feel as a "horizontal
 * era sweep": wider inter-rank spacing (`rankSep`) makes the era sweep read
 * as sparse landmarks, and tighter intra-rank spacing (`nodeSep`) keeps the
 * temporal-unknown era cluster compact. Narrative (V1.122) leaves both
 * undefined so `useAutoLayout`'s internal defaults (80/80) apply — V1.122
 * regression preserved verbatim.
 *
 * The `hasSuppliedPositions: true` flag is preserved on both layers so the
 * adapter's temporal-sorted positions survive first open (Batch 1 reviewer
 * note + Batch A T2); these `rankSep` / `nodeSep` values only take effect
 * on an explicit `relayout()`.
 *
 * `simplify:` static metrics — `dagre` is invoked at most a couple of times
 * per surface entry (first open is supplied-positions; explicit relayout on
 * user action). No measurable cost to deriving these inline.
 */
const BRIEF_LAYOUT_OPTIONS = {
  direction: 'LR' as const,
  rankSep: 240,
  nodeSep: 40,
  hasSuppliedPositions: true,
};
const NARRATIVE_LAYOUT_OPTIONS = {
  direction: 'LR' as const,
  hasSuppliedPositions: true,
};

const NODE_ID_PREFIX = 'entity:';
/** V1.147 P2 T3 — compute log-event node id prefix (`compute:<event_id>`). */
const COMPUTE_NODE_ID_PREFIX = 'compute:';
/**
 * V1.147 P2 T3 — synthetic `key_block_id` for compute nodes. Compute nodes
 * are NOT KB entities; the id is namespaced so it can never collide with a
 * real `entity:` node and never reaches the `kb.patch_entity` write path.
 */
const COMPUTE_KEY_BLOCK_PREFIX = 'log:';

function nodeIdOf(keyBlockId: string): string {
  return `${NODE_ID_PREFIX}${keyBlockId}`;
}

function computeNodeIdOf(eventId: string): string {
  return `${COMPUTE_NODE_ID_PREFIX}${eventId}`;
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
 * Extract V1.123 Brief-era markers from `body.attributes` per architect §2.3
 * + §8 (`era_id`, `start_hint`, `end_hint`, `world_summary` — all freeform).
 * Non-string values and empty strings are treated as absent. Used by the
 * Brief projection to populate the era card + Brief-era inspector (Task 4).
 *
 * Type narrowing mirrors `occurredAtOf` — `body.attributes` is `unknown`
 * until narrowed.
 */
function extractEraAttributes(entity: WorldKbEntityProjection): {
  eraId?: string;
  startHint?: string;
  endHint?: string;
  worldSummary?: string;
} {
  const attrs = entity.body?.attributes;
  if (attrs === null || typeof attrs !== 'object') return {};
  const a = attrs as Record<string, unknown>;
  const readString = (key: string): string | undefined => {
    const raw = a[key];
    return typeof raw === 'string' && raw.length > 0 ? raw : undefined;
  };
  return {
    eraId: readString('era_id'),
    startHint: readString('start_hint'),
    endHint: readString('end_hint'),
    worldSummary: readString('world_summary'),
  };
}

/**
 * Project the entities + relationships of a `WorldKbGraphResponse` onto
 * Timeline nodes + edges for the given layer.
 *
 * V1.123 P1 T2 — the layer parameter selects the active projection:
 *   - `'narrative'` (default) — V1.122 event timeline + Context clusters.
 *   - `'brief'`               — V1.123 Brief-era sweep (era markers only).
 *
 * V1.122 callers that omit the layer argument get the V1.122 Narrative
 * projection unchanged — backward-compat is verified by the V1.122 regression
 * test suite (`timeline-canvas-adapter.test.tsx`,
 * `timeline-a11y.test.tsx`, `timeline-write-boundary.test.tsx`).
 *
 * Narrative positioning rules (V1.122 preserved):
 *   - Dated events (`body.attributes.occurred_at` present) are placed along
 *     the when-axis (Y = 0) sorted lexicographically by timestamp (ISO 8601
 *     sorts chronologically when the format is consistent).
 *   - Undated events cluster below the when-axis (temporal-unknown group).
 *   - Non-event, non-era entities (Context) cluster above the when-axis.
 *     V1.123 architect §5.2: era entities are EXCLUDED from Context clusters
 *     on the Narrative layer (they are Brief-layer-only markers).
 *   - Node ids are stable across refetches (`entity:${key_block_id}`).
 *
 * Brief positioning rules (V1.123 P1 T2):
 *   - Eras with `body.attributes.start_hint` are placed along the Brief
 *     when-axis (Y = 0) sorted lexicographically by `start_hint`.
 *   - Eras without `start_hint` cluster below the when-axis (temporal-unknown
 *     group) — mirrors the V1.122 undated-event convention so the Brief
 *     sweep stays legible.
 *   - No relationship edges on Brief (architect §8 + layer-feel spec §2.2 —
 *     minimal density, era sweep only).
 *
 * Relationship edges reuse the V1.74 World KB edge rendering verbatim
 * (`WorldKbEdgeData`); both the stored + symmetric_reverse projections are
 * rendered when a relationship is symmetric, mirroring the World KB adapter.
 *
 * V1.147 P2 T3 — compute merge. `events` (optional) carries the World's
 * `event_type=compute_result` + `status=canon` timeline log events (the T1
 * route already filters; the adapter re-enforces defensively). The Narrative
 * layer merges them as `timeline-compute-result` nodes — the machine-written
 * family alongside the author KB `block_type=event` family. Brief ignores
 * events entirely. See `mergeComputeEvents` for the family distinction.
 */
export function projectTimelineGraph(
  graph: TimelineGraph,
  layer: TimelineLayer = 'narrative',
  events?: TimelineEventInfo[],
  moduleNames?: ReadonlyMap<string, string>,
): {
  nodes: Node<TimelineNodeData>[];
  edges: Edge<TimelineEdgeData>[];
} {
  if (layer === 'brief') {
    return projectBriefLayer(graph);
  }
  return projectNarrativeLayer(graph, events, moduleNames);
}

/**
 * V1.123 P1 T2 — Brief layer projection (architect §2 + §8).
 *
 * Filters `entities[block_type=era]` onto the Brief when-axis. Non-era
 * entities (events, characters, ...) are EXCLUDED from the Brief layer —
 * they belong to the Narrative layer (V1.122). Relationship edges are NOT
 * rendered on Brief (minimal density per layer-feel §2.2).
 *
 * `simplify:` LR step metrics mirror the V1.122 Narrative lane scheme so
 * both layers share a familiar reading direction. Replace with an era-aware
 * temporal plugin if the Brief sweep grows beyond ~12 era markers (layer-feel
 * §2.2 density target).
 */
function projectBriefLayer(graph: TimelineGraph): {
  nodes: Node<TimelineNodeData>[];
  edges: Edge<TimelineEdgeData>[];
} {
  const entities = graph.entities ?? [];

  const eraEntities: WorldKbEntityProjection[] = entities.filter(
    (e) => e.block_type === 'era',
  );

  // Split eras by temporal signal; sort dated eras chronologically by
  // `body.attributes.start_hint`. Mirrors the V1.122 dated/undated event
  // split so the Brief sweep reads as "earliest era on the left".
  const datedEras: Array<{
    entity: WorldKbEntityProjection;
    startHint: string;
  }> = [];
  const undatedEras: WorldKbEntityProjection[] = [];
  for (const e of eraEntities) {
    const { startHint } = extractEraAttributes(e);
    if (startHint === undefined) {
      undatedEras.push(e);
    } else {
      datedEras.push({ entity: e, startHint });
    }
  }
  datedEras.sort((a, b) => {
    if (a.startHint < b.startHint) return -1;
    if (a.startHint > b.startHint) return 1;
    // Stable tiebreaker on key_block_id so identical start hints are
    // deterministic across refetches.
    return a.entity.key_block_id.localeCompare(b.entity.key_block_id);
  });

  const nodes: Node<TimelineNodeData>[] = [];

  datedEras.forEach(({ entity, startHint }, i) => {
    nodes.push({
      id: nodeIdOf(entity.key_block_id),
      type: 'timeline-brief-era',
      position: { x: ORIGIN_X + i * EVENT_STEP_X, y: WHEN_AXIS_Y },
      data: eraEntityToTimelineNodeData(entity, startHint),
    });
  });

  // Undated eras cluster below the when-axis (temporal-unknown group),
  // continuing the X spread from the rightmost dated era — same convention
  // as the V1.122 undated event cluster.
  const undatedOriginX = ORIGIN_X + datedEras.length * EVENT_STEP_X;
  undatedEras.forEach((entity, i) => {
    nodes.push({
      id: nodeIdOf(entity.key_block_id),
      type: 'timeline-brief-era',
      position: { x: undatedOriginX + i * EVENT_STEP_X, y: TEMPORAL_UNKNOWN_Y },
      data: eraEntityToTimelineNodeData(entity, undefined),
    });
  });

  // V1.126 P1 — Brief directed axis spine (decoration-only, Y=0, appended
  // after entity nodes so existing tests that access nodes[0] pass unchanged).
  // Only added when at least one dated era exists (eras with start_hint).
  // If all eras are undated, omit the spine — no temporal axis to render.
  if (datedEras.length > 0) {
    const eraBounds: BriefSpineConfig['eraBounds'] = datedEras.map(({ entity, startHint }) => {
      const { eraId, endHint } = extractEraAttributes(entity);
      return {
        startHint,
        endHint,
        eraId,
        eraLabel: entity.canonical_name || startHint,
      };
    });
    const briefSpineData: DirectedAxisSpineNodeData = {
      layer: 'brief',
      spineConfig: { kind: 'brief', eraBounds },
      accentColor: 'var(--color-canvas-layer-brief-accent)',
    };
    nodes.push({
      id: 'directed-axis-spine',
      type: 'directedAxisSpine',
      position: { x: 0, y: WHEN_AXIS_Y + SPINE_Y_OFFSET },
      data: briefSpineData as unknown as TimelineNodeData,
      selectable: false,
      focusable: false,
    });
  }

  // Brief layer: no relationship edges (architect §8 + layer-feel §2.2 —
  // minimal density, era sweep only). Edges belong to the Narrative layer.
  return { nodes, edges: [] };
}

function eraEntityToTimelineNodeData(
  entity: WorldKbEntityProjection,
  startHint: string | undefined,
): TimelineNodeData {
  const eraAttrs = extractEraAttributes(entity);
  const data: TimelineNodeData = {
    ...entity,
    layoutHint: 'brief',
  };
  if (startHint !== undefined) data.startHint = startHint;
  if (eraAttrs.eraId !== undefined) data.eraId = eraAttrs.eraId;
  if (eraAttrs.endHint !== undefined) data.endHint = eraAttrs.endHint;
  if (eraAttrs.worldSummary !== undefined) data.worldSummary = eraAttrs.worldSummary;
  return data;
}

/**
 * V1.122 Narrative projection — preserved verbatim for the Narrative layer,
 * with a single V1.123 additive change: era entities (`block_type='era'`)
 * are EXCLUDED from Context clusters per architect §5.2 (Context clusters =
 * entities.filter(e => !['event','era'].includes(e.block_type))). They are
 * Brief-layer-only markers.
 *
 * V1.147 P2 T3 — compute merge: canon compute_result log events are appended
 * to the Narrative projection as `timeline-compute-result` nodes (see
 * `mergeComputeEvents`).
 */
function projectNarrativeLayer(
  graph: TimelineGraph,
  events?: TimelineEventInfo[],
  moduleNames?: ReadonlyMap<string, string>,
): {
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
    } else if (e.block_type !== 'era') {
      // V1.123 architect §5.2: era entities are EXCLUDED from Context
      // clusters on the Narrative layer — they are Brief-layer-only markers.
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

  // V1.147 P2 T3 — merge canon compute_result log events (machine-written
  // family) after the authored KB block. Two event families coexist on the
  // Narrative when-axis: `entity:<kb_id>` (author KB `block_type=event`) and
  // `compute:<event_id>` (compute log events). The families are disjoint by
  // storage + node id, so a KB event and a compute log event can never
  // double-render the same story beat.
  const computeNodes = mergeComputeEvents(
    events ?? [],
    graph,
    datedEvents.length,
    moduleNames,
    undatedEvents.length,
  );
  nodes.push(...computeNodes);

  // V1.126 P1 — Narrative directed axis spine (decoration-only, Y=0,
  // appended after entity nodes so existing tests pass unchanged).
  // Only added when the layer has event data.
  if (datedEvents.length > 0) {
    const tickTimestamps: NarrativeSpineConfig['tickTimestamps'] = datedEvents.map(
      ({ occurredAt }) => occurredAt,
    );
    const narrativeSpineData: DirectedAxisSpineNodeData = {
      layer: 'narrative',
      spineConfig: { kind: 'narrative', tickTimestamps },
      accentColor: 'var(--color-canvas-layer-narrative-accent)',
    };
    nodes.push({
      id: 'directed-axis-spine',
      type: 'directedAxisSpine',
      position: { x: 0, y: WHEN_AXIS_Y + SPINE_Y_OFFSET },
      data: narrativeSpineData as unknown as TimelineNodeData,
      selectable: false,
      focusable: false,
    });
  }

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

// ─── Compute-result merge (V1.147 P2 T3) ────────────────────────────────────

/**
 * Narrow the daemon-stamped provenance namespace
 * (`extensions.compute = { module_id, module_version, run_id, source_kind }`).
 * The T1 route parses `extensions_nexus_json`; P0 Accept stamps
 * `source_kind: "direct_invoke"` (preset-path stamping lands separately).
 * Unknown shapes are treated as absent (no fabricated provenance).
 */
interface ComputeProvenanceShape {
  module_id?: unknown;
  module_version?: unknown;
  run_id?: unknown;
  source_kind?: unknown;
}

function computeProvenanceOf(event: TimelineEventInfo): ComputeProvenanceShape {
  const ext = event.extensions;
  if (ext === null || typeof ext !== 'object') return {};
  const compute = (ext as Record<string, unknown>).compute;
  if (compute === null || typeof compute !== 'object') return {};
  return compute as ComputeProvenanceShape;
}

function readString(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

/**
 * Merge gate — only machine-written `compute_result` log events in the
 * `canon` state project onto the Narrative layer (plan Global Constraints
 * merge discipline; the T1 route filters server-side, this re-enforces).
 */
export function isMergeableComputeEvent(event: TimelineEventInfo): boolean {
  return event.event_type === 'compute_result' && event.status === 'canon';
}

/**
 * Build the compute node payload from a log event + a prebuilt
 * `key_block_id → canonical_name` map (QC S-map: the map is built ONCE in
 * `mergeComputeEvents` and shared across all events — O(events + entities),
 * not O(events × entities)).
 *
 * `affectedEntries` resolves `affected_key_block_ids` against the map
 * (unknown ids fall back to the id itself — honest).
 * Exported for unit tests; consumed by `mergeComputeEvents`.
 */
export function buildComputeNodePayload(
  event: TimelineEventInfo,
  graphById: ReadonlyMap<string, string>,
  moduleNames?: ReadonlyMap<string, string>,
): ComputeNodePayload {
  const provenance = computeProvenanceOf(event);
  const affectedEntries = (event.affected_key_block_ids ?? []).map((id) => ({
    id,
    title: graphById.get(id) ?? id,
  }));
  const moduleId = readString(provenance.module_id) ?? '';
  const sourceKindRaw = readString(provenance.source_kind);
  const sourceKind: ComputeNodePayload['sourceKind'] =
    sourceKindRaw === 'direct_invoke' || sourceKindRaw === 'preset'
      ? sourceKindRaw
      : undefined;
  return {
    eventId: event.id,
    moduleId,
    moduleName: moduleId ? (moduleNames?.get(moduleId) ?? moduleId) : moduleId,
    moduleVersion: readString(provenance.module_version) ?? '',
    runId: readString(provenance.run_id),
    sourceKind,
    reportDigest: event.summary ?? undefined,
    affectedEntries,
  };
}

/**
 * Project canon compute_result log events onto the Narrative layer.
 *
 * Positioned AFTER the authored KB dated block on the when-axis (Y = 0),
 * sorted among themselves by ISO `created_at`. Dated-only (parseable
 * `created_at`); unparseable rows cluster in the temporal-unknown group —
 * mirrors the V1.122 undated-event convention. Compute nodes carry the
 * machine `created_at` as `occurredAtHint` (a real timestamp, not a
 * fabricated chronology — `sequence_no` is never used for ordering).
 *
 * `simplify:` compute events are appended after the KB dated block rather
 * than interleaved with the freeform `occurred_at` lexical ordering. Mixed
 * freeform-vs-ISO chronology is not canonical; a temporal-aware sort that
 * unifies both families is deferred (DF-V1122-DEEPER-WB).
 *
 * Undated rows continue the temporal-unknown group's X spread PAST the KB
 * undated cluster (`kbUndatedCount` offset) so the two undated families
 * never share coordinates on the y=220 lane (review F2 — the KB cluster
 * starts at `ORIGIN_X + kbDatedCount*EVENT_STEP_X`; without the offset the
 * compute undated base could land on top of it whenever
 * `datedComputeCount < kbUndatedCount`).
 */
export function mergeComputeEvents(
  events: TimelineEventInfo[],
  graph: TimelineGraph,
  kbDatedCount: number,
  moduleNames?: ReadonlyMap<string, string>,
  kbUndatedCount = 0,
): Node<TimelineNodeData>[] {
  const mergeable = events.filter(isMergeableComputeEvent);
  if (mergeable.length === 0) return [];

  // QC S-map: hoist the id → canonical_name map so `buildComputeNodePayload`
  // never rebuilds it per event (O(entities) once, then O(events) lookups).
  const graphById = new Map(
    (graph.entities ?? []).map((e) => [e.key_block_id, e.canonical_name]),
  );
  const worldId = (graph.entities ?? [])[0]?.world_id ?? '';

  const dated: Array<{ event: TimelineEventInfo; createdAt: string }> = [];
  const undated: TimelineEventInfo[] = [];
  for (const event of mergeable) {
    const createdAt = event.created_at;
    if (createdAt && !Number.isNaN(Date.parse(createdAt))) {
      dated.push({ event, createdAt });
    } else {
      undated.push(event);
    }
  }
  dated.sort((a, b) => {
    if (a.createdAt < b.createdAt) return -1;
    if (a.createdAt > b.createdAt) return 1;
    return a.event.id.localeCompare(b.event.id);
  });

  const nodes: Node<TimelineNodeData>[] = [];
  const computeStartX = ORIGIN_X + kbDatedCount * EVENT_STEP_X;

  dated.forEach(({ event, createdAt }, i) => {
    nodes.push({
      id: computeNodeIdOf(event.id),
      type: 'timeline-compute-result',
      position: { x: computeStartX + i * EVENT_STEP_X, y: WHEN_AXIS_Y },
      data: computeEventToTimelineNodeData(event, graphById, worldId, createdAt, moduleNames),
    });
  });

  // Undated compute rows cluster in the temporal-unknown group (y=220) AFTER
  // the KB undated cluster (review F2): `kbUndatedCount` shifts the X base so
  // the two families never stack on identical coordinates.
  const undatedOriginX =
    computeStartX + dated.length * EVENT_STEP_X + kbUndatedCount * EVENT_STEP_X;
  undated.forEach((event, i) => {
    nodes.push({
      id: computeNodeIdOf(event.id),
      type: 'timeline-compute-result',
      position: { x: undatedOriginX + i * EVENT_STEP_X, y: TEMPORAL_UNKNOWN_Y },
      data: computeEventToTimelineNodeData(event, graphById, worldId, undefined, moduleNames),
    });
  });

  return nodes;
}

function computeEventToTimelineNodeData(
  event: TimelineEventInfo,
  graphById: ReadonlyMap<string, string>,
  worldId: string,
  createdAt: string | undefined,
  moduleNames?: ReadonlyMap<string, string>,
): TimelineNodeData {
  const payload = buildComputeNodePayload(event, graphById, moduleNames);
  const data: TimelineNodeData = {
    // Synthetic entity-projection fields (see TimelineNodeData docblock):
    // compute nodes are NOT KB entities; the write path never sees them.
    key_block_id: `${COMPUTE_KEY_BLOCK_PREFIX}${event.id}`,
    world_id: worldId,
    block_type: 'event',
    canonical_name: event.title ?? payload.moduleName,
    status: event.status,
    version: 0,
    layoutHint: 'compute',
    compute: payload,
    source_anchor_count: 0,
    updated_at: event.created_at,
  };
  if (createdAt !== undefined) data.occurredAtHint = createdAt;
  return data;
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
 * The disclaimer is present whenever event entities are rendered (i.e. when
 * the graph has any `block_type=event` entity), and is omitted only for
 * zero-event graphs (which have their own honest empty-state copy per §7).
 *
 * Rationale (architect-locked §3.3 + §7): the adapter performs NO date
 * parsing in MVP — `occurred_at` is read from unvalidated entity attributes
 * and the when-axis ordering is plain lexical string sort. Freeform
 * non-date strings ("Spring 1042", "10", "2") are NOT canonical temporal
 * signals, so a left-to-right timeline that "looks dated" is still inferred
 * ordering, not chronology. The disclaimer must surface that honestly even
 * when every event carries a non-empty `occurred_at` string. A date parser
 * that could lift the disclaimer for ISO-8601-only graphs is out of V1.122
 * scope (`simplify:` future `timeline-date-inference` enhancement).
 *
 * `simplify:` plain English (no i18n) — same convention as the World KB
 * `graphSummary`. The canvas a11y summary is an SR-only live region, not a
 * visible label; the World KB precedent follows this rule. If a future
 * iteration localises the canvas a11y summary, mirror the change here and
 * in `world-kb/graph-projection.ts::graphSummary`.
 *
 * V1.147 P2 T3 — `computeEvents` (optional) appends the merged compute
 * family count ("N compute events") when present, so the SR summary reflects
 * the full Narrative projection, not only the KB half.
 */
export function summarizeTimelineGraph(
  graph: TimelineGraph,
  computeEvents?: TimelineEventInfo[],
): string {
  const entities = graph.entities ?? [];
  const relationships = graph.relationships ?? [];
  const anchors = graph.source_anchors ?? [];

  const events = entities.filter((e) => e.block_type === 'event');
  const contextCount = entities.length - events.length;
  const datedEvents = events.filter((e) => occurredAtOf(e) !== undefined);

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
  // V1.147 P2 T3 — merged compute family (machine-written log events).
  const computeCount = (computeEvents ?? []).filter(isMergeableComputeEvent).length;
  if (computeCount > 0) {
    parts.push(
      `${computeCount} ${computeCount === 1 ? 'compute event' : 'compute events'}`,
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

  // Disclaimer — lexical string sorting is never canonical chronology. The
  // adapter performs no date parsing in MVP, so every rendered event
  // ordering is inferred. The disclaimer is present whenever event entities
  // are rendered (block_type=event), and omitted only for zero-event graphs
  // (which surface their own honest empty-state copy via <EmptyState> per
  // §7).
  if (events.length > 0) {
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
 * V1.123 P1 T2 — `activeLayer` selects which projection `projectGraph(graph)`
 * delegates to. Default `'narrative'` for V1.122 backward compatibility (the
 * V1.122 test suite calls `createTimelineCanvasAdapter(ctxRef)` without a
 * layer argument; their existing assertions MUST stay green). Task 3 wires
 * the World-entry default layer to `'brief'` when era data exists, falling
 * back to `'narrative'` (Task 3 brief Step 4 + plan Global Constraints).
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
 *     region: non-empty for every graph state (empty + dense + freeform
 *     temporal signal); emits the ordering disclaimer whenever event
 *     entities are rendered (lexical string sort is never canonical
 *     chronology), and omits it only for zero-event graphs (PR #156 fix).
 *
 * V1.147 P2 T3 extensions over T5:
 *   - `timelineEvents` (optional) — the World's canon compute_result log
 *     events (T1 route). Captured at factory creation; the orchestrator
 *     rebuilds the adapter when the events array changes so
 *     `useCanvasSurface`'s `[graph, adapter]` memo re-projects with the
 *     merged family (data-driven rebuild, same as the layer swap).
 *   - `projectGraph(graph)` — delegates to `projectTimelineGraph(graph,
 *     activeLayer, timelineEvents)`; the Narrative layer merges compute
 *     events, Brief ignores them.
 *   - `renderInspector(node)` — `layoutHint === 'compute'` dispatches to
 *     `TimelineComputeInspector` (module + version + provenance + digests +
 *     Open Run hand-off via `ctxRef.current.onOpenRun`). Compute nodes never
 *     reach `TimelineInspector` — the `kb.patch_entity` write path is KB-only.
 */
export function createTimelineCanvasAdapter(
  ctxRef: MutableRefObject<TimelineCanvasAdapterContext>,
  activeLayer: TimelineLayer = 'narrative',
  timelineEvents?: TimelineEventInfo[],
  computeModuleNames?: ReadonlyMap<string, string>,
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
    // V1.123 P1 T4 — layer-dependent dagre options. Brief carries wider
    // `rankSep` + smaller `nodeSep` than the V1.122 Narrative default so
    // an explicit `relayout()` produces the horizontal era sweep feel
    // (layer-feel §2.2). The supplied era positions win on first open
    // (`hasSuppliedPositions: true`); these options only kick in on
    // explicit relayout.
    layoutOptions:
      activeLayer === 'brief' ? BRIEF_LAYOUT_OPTIONS : NARRATIVE_LAYOUT_OPTIONS,

    projectGraph(graph) {
      // V1.123 P1 T2 — delegates to the active layer. Default `'narrative'`
      // for V1.122 backward compat (the V1.122 regression suite verifies
      // event timeline projection unchanged). Task 3 passes `'brief'` when
      // the World entry has era data; the canvas component rebuilds the
      // adapter via `useMemo([activeLayer], ...)` so layer swap triggers a
      // fresh projection through `useCanvasSurface`.
      //
      // V1.147 P2 T3 — the captured `timelineEvents` merge into the Narrative
      // projection; Brief ignores them. `computeModuleNames` resolves module
      // display names from the registry map (module_id fallback).
      return projectTimelineGraph(graph, activeLayer, timelineEvents, computeModuleNames);
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
      // V1.123 P1 T4 — Brief-era nodes dispatch to a dedicated Brief-era
      // inspector that surfaces the era markers (`eraId`, `startHint`,
      // `endHint`, `worldSummary`) prominently. The generic Narrative
      // inspector (title + body JSON editor) remains the path for event
      // + context nodes (V1.122 regression). The dispatch discriminates
      // on `layoutHint` rather than `node.type` so the inspector stays
      // decoupled from the React Flow node-type registry.
      const data = node.data as TimelineNodeData;
      if (data.layoutHint === 'brief') {
        return <TimelineBriefEraInspector node={node} ctxRef={ctxRef} />;
      }
      // V1.147 P2 T3 — compute nodes get the compute inspector (module +
      // version + provenance + digests + Open Run). Compute nodes MUST NOT
      // reach the KB inspector: they are log events, not KB entities, and
      // the `kb.patch_entity` write path is KB-only.
      if (data.layoutHint === 'compute') {
        return <TimelineComputeInspector node={node} ctxRef={ctxRef} />;
      }
      return <TimelineInspector node={node} ctxRef={ctxRef} />;
    },

    renderAltView() {
      return <TimelineAltViewWrapper ctxRef={ctxRef} />;
    },

    summarizeGraph(graph) {
      // V1.147 P2 T3 — the a11y summary includes the merged compute family
      // count when events are present.
      return summarizeTimelineGraph(graph, timelineEvents);
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
