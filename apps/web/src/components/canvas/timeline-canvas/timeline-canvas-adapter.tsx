/**
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
 * selects the active layer; V1.156 P1 T1 extends the layer union with
 * `'moment'` (World Timeline Moment — see `TimelineLayer`). The default
 * `projectGraph(graph)` delegates to the adapter's active layer, which
 * defaults to `'narrative'` for V1.122 backward compatibility (Task 3 wires
 * Brief-default-on-World-entry).
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
 * envelope into a `TimelineConflictInfo`; the modal is orchestrator-owned,
 * mirroring the World KB adapter.
 *
 * `wire_contracts_changed: true` — attributable to Task 1's single additive
 * `BlockType = "era"` enum value (schema + Rust companion + codegen). V1.123
 * P1 Tasks 2+3 add zero wire diff: the adapter is a pure frontend filter
 * over the V1.73 `WorldKbGraphResponse` (architect §5).
 *
 * V1.156 P1 T1 (World Timeline Moment layer) — `wire_contracts_changed: false`:
 * the Moment layer is a frontend-only read/projection. The `TimelineLayer`
 * union extension is UI-only (not a wire DTO); the Moment carrier reuses the
 * V1.108 `SceneBeatFixturePayload` fixture slot (the `WorkOutline` wire
 * exposes no scenes/beats today — fixture-driven for dev/testing, honest
 * empty-state in production; DR-26 tracks the future wire extension). No
 * `schemas/` / codegen / daemon Rust / `@42ch/nexus-contracts` bump.
 */
import type { MutableRefObject, ReactNode } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type {
  TimelineEventInfo,
  WorldKbEntityPatch as WorldKbEntityPatchWire,
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import type { WorldKbEdgeData } from '../world-kb/types';
import type {
  BeatFixture,
  SceneBeatFixturePayload,
  SceneFixture,
} from '../outline-canvas/graph-projection';
import type { WorkTimelineNodeData } from '../work-timeline-canvas/work-timeline-canvas-adapter';
import { workTimelineNodeTypes } from '../work-timeline-canvas/work-timeline-node-types';
import { TimelineInspector } from './timeline-inspector';
import { TimelineComputeInspector } from './timeline-compute-inspector';
import { TimelineBriefEraInspector } from './timeline-brief-era-inspector';
import { renderTimelineMomentInspector } from './timeline-moment-inspector';
import { TimelineAltView } from './timeline-alt-view';
import type { BriefSpineConfig, DirectedAxisSpineNodeData, MomentSpineConfig, NarrativeSpineConfig } from './directed-axis-spine';
import { SPINE_Y_OFFSET } from './directed-axis-spine';
import { timelineNodeTypes } from './timeline-node-types';
import { buildEraTree } from './brief-era-tree';
import { BriefTimeBands } from './brief-time-bands';


/** Single graph source — no wrapper, no join. */
export type TimelineGraph = WorldKbGraphResponse;

/**
 * V1.123 layer kind on the World Timeline surface.
 *
 * - `'brief'`     — world-shape era sweep (`block_type=era` KeyBlocks on the
 *                   Brief when-axis; minimal density; no relationship edges).
 * - `'narrative'` — V1.122 event timeline (`block_type=event` on the when-axis
 *                   + Context clusters + relationship edges).
 * - `'moment'`    — V1.156 P1 T1 read/projection of bound Works' Scene/Beat
 *                   data (fixture-driven via `SceneBeatFixturePayload`;
 *                   mirrors the Work Timeline Moment layer feel — same
 *                   carrier, same node types per V1.123 layer-feel §2.4).
 *
 * Architect-locked in `three-layer-architecture.md` §8 + `layer-feel-
 * differentiation.md` §2 + `canvas-strategy-surface.md` §3.3.3 V1.156
 * amendment. The adapter's `projectGraphForLayer(graph, layer)` selects the
 * active projection; `projectGraph(graph)` delegates to the adapter's active
 * layer (default `'narrative'` for V1.122 backward compat). UI-only union
 * type — NOT a wire DTO (`wire_contracts_changed: false` for V1.156 P1).
 */
export type TimelineLayer = 'brief' | 'narrative' | 'moment';

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
   */
  moduleVersion: string;
  moduleName: string;
  /** Run correlation id — direct lane only (absent for preset-path nodes). */
  runId?: string;
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
  layoutHint: 'event' | 'context' | 'brief' | 'compute';
  occurredAtHint?: string;
  /**
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
  compute?: ComputeNodePayload;
}

/** Verbatim reuse of the V1.74 World KB relationship edge payload. */
export type TimelineEdgeData = WorldKbEdgeData;

/**
 * Patchable fields on a `WorldKbEntityProjection` (V1.73 `WorldKbEntityPatch`).
 * The V1.122 architect lock scoped the Timeline surface to the four scalar
 * fields; v1.203 P2 O3 widens it SPA-side with `modules` so the event
 * inspector can author `modules.observation` on narrative-log events (the
 * wire has carried `modules` since V1.165 P2 — `wire_contracts_changed:
 * false`, whole-first-level-value upsert per AR-4/PD-12).
 */
export type TimelinePatchField = 'title' | 'body' | 'aliases' | 'block_type' | 'modules';

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
  modules?: WorldKbEntityPatchWire['modules'];
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
  client?: unknown;
  /**
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
  onConflict?: (info: TimelineConflictInfo) => void;
  nodes?: Node<TimelineNodeData>[];
  selectedNodeId?: string | null;
  onSelectNode?: (nodeId: string) => void;
  /**
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
   * Undefined when no event-level match exists for the current selection —
   * the V1.123 surface-level fallback (`boundWorkId` +
   * `onViewInWorkTimeline` → `?layer=narrative` only) applies instead
   * (PD-5 honest three-state matrix).
   */
  boundWorkEventId?: string;
  boundWorkEventWorkId?: string;
  onViewInWorkTimeline?: () => void;
  /**
   * Fires with the full run id + module id when the author activates "Open
   * Run" on a Compute result node; the orchestrator navigates to
   * `/modules` run detail (deep link). Undefined when the compute
   * inspector is read-only (tests without wiring).
   */
  onOpenRun?: (runId: string, moduleId: string) => void;
  /**
   * Fires with the compute event's timeline event id when the author
   * activates "Branch this world's timeline from here" on a Compute result
   * node. The orchestrator opens the fork-create dialog pre-seeded with
   * that fork point (deriving the parent branch from the event's own
   * `branch_id`). Undefined when the compute inspector is read-only (tests
   * without wiring) — the button hides.
   */
  onCreateFork?: (eventId: string) => void;
  /**
   * V1.156 P1 fix-wave 1 (F3) — the adapter factory's captured
   * `sceneBeatFixture` parameter takes precedence over this slot at
   * projection time (the orchestrator passes the fixture through the memo
   * deps so identity changes re-project); this slot remains as the
   * fallback for direct ctxRef wiring (tests / legacy callers).
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

export type TimelineCanvasAdapter = CanvasSurfaceAdapter<
  TimelineGraph,
  TimelineNodeData,
  TimelineEdgeData
> & {
  renderBriefTimeBands?(graph: TimelineGraph): ReactNode;
};

/**
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
 * `simplify:` deterministic vertical stack mirroring the Work Timeline
 * adapter. P4 may swap in a richer manuscript-aware layout (anchored
 * scene-card grid) per layer-feel §9.
 */
const MOMENT_ORIGIN_X = 40;
const MOMENT_ORIGIN_Y = 40;
const MOMENT_SCENE_STEP_Y = 160;
const MOMENT_BEAT_ORIGIN_Y = 56;
const MOMENT_BEAT_STEP_Y = 44;
const MOMENT_CHAPTER_STEP_X = 360;

/**
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
export const BRIEF_LAYOUT_OPTIONS = {
  direction: 'LR' as const,
  rankSep: 240,
  nodeSep: 40,
  hasSuppliedPositions: true,
};
const NARRATIVE_LAYOUT_OPTIONS = {
  direction: 'LR' as const,
  hasSuppliedPositions: true,
};
const MOMENT_LAYOUT_OPTIONS = {
  direction: 'TB' as const,
  rankSep: 60,
  nodeSep: 30,
  hasSuppliedPositions: true,
};

const NODE_ID_PREFIX = 'entity:';
/** V1.147 P2 T3 — compute log-event node id prefix (`compute:<event_id>`). */
const COMPUTE_NODE_ID_PREFIX = 'compute:';
const COMPUTE_KEY_BLOCK_PREFIX = 'log:';

export function nodeIdOf(keyBlockId: string): string {
  return `${NODE_ID_PREFIX}${keyBlockId}`;
}

function computeNodeIdOf(eventId: string): string {
  return `${COMPUTE_NODE_ID_PREFIX}${eventId}`;
}

const MOMENT_SCENE_NODE_ID_PREFIX = 'wt-scene:';
const MOMENT_BEAT_NODE_ID_PREFIX = 'wt-beat:';

function momentSceneNodeId(sceneId: string): string {
  return `${MOMENT_SCENE_NODE_ID_PREFIX}${sceneId}`;
}

function momentBeatNodeId(beatId: string): string {
  return `${MOMENT_BEAT_NODE_ID_PREFIX}${beatId}`;
}

/**
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
  fixture?: SceneBeatFixturePayload,
  worldId?: string,
): {
  nodes: Node<TimelineNodeData>[];
  edges: Edge<TimelineEdgeData>[];
} {
  if (layer === 'brief') {
    return projectBriefLayer(graph);
  }
  if (layer === 'moment') {
    // V1.200 DR-26 — the World is identified by the orchestrator's supplied
    // `worldId` (never by the first KB entity, which an empty KB graph does
    // not have); the carrier `fixture` is the composed bound-Work payload.
    return projectMomentLayer(fixture, worldId) as unknown as {
      nodes: Node<TimelineNodeData>[];
      edges: Edge<TimelineEdgeData>[];
    };
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
 * V1.156 P2 T1 — exported for the Work Timeline Brief layer, which projects
 * the bound World's Brief VERBATIM through this function (Work-Brief feel
 * ≡ World-Brief feel — same carrier `block_type=era`, same `TimelineNodeData`
 * with `layoutHint: 'brief'`, same spine). The era-marker extraction
 * (`extractEraAttributes`) is reused transitively.
 *
 * DUAL-PROJECTION BRIDGE (QC1-I-001) — do not delete this path while
 * Brief-band mode is live: the World Timeline Brief layer renders the
 * vertical time-band model (`renderBriefTimeBands` → `buildEraTree` →
 * `<BriefTimeBands />`), which SUPERSEDES this horizontal sweep visually.
 * However, band clicks still route through `onSelectNode(nodeIdOf(eraId))`
 * → React Flow node selection → the era inspector. This RF projection is
 * therefore still REQUIRED as the selection/inspector carrier even though
 * it is hidden behind the band UI — the two era models (band tree vs RF
 * sweep) stay aligned by construction on the same graph read. When a
 * tree-native selection carrier exists (e.g. selection driven directly from
 * `EraTreeNode` + a thin selection map), drop this projection for Brief
 * bands and keep it only for Work-Brief / non-band Brief fallback.
 *
 * `simplify:` LR step metrics mirror the V1.122 Narrative lane scheme so
 * both layers share a familiar reading direction. Replace with an era-aware
 * temporal plugin if the Brief sweep grows beyond ~12 era markers (layer-feel
 * §2.2 density target).
 */
export function projectBriefLayer(graph: TimelineGraph): {
  nodes: Node<TimelineNodeData>[];
  edges: Edge<TimelineEdgeData>[];
} {
  const entities = graph.entities ?? [];

  const eraEntities: WorldKbEntityProjection[] = entities.filter(
    (e) => e.block_type === 'era',
  );

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

  const undatedOriginX = ORIGIN_X + datedEras.length * EVENT_STEP_X;
  undatedEras.forEach((entity, i) => {
    nodes.push({
      id: nodeIdOf(entity.key_block_id),
      type: 'timeline-brief-era',
      position: { x: undatedOriginX + i * EVENT_STEP_X, y: TEMPORAL_UNKNOWN_Y },
      data: eraEntityToTimelineNodeData(entity, undefined),
    });
  });

  if (datedEras.length > 0) {
    const eraBounds: BriefSpineConfig['eraBounds'] = datedEras.map(({ entity, startHint }) => {
      const { eraId, endHint } = extractEraAttributes(entity);
      return {
        startHint,
        endHint,
        eraId,
        eraLabel: entity.canonical_name ?? startHint,
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
      contextEntities.push(e);
    }
  }

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

  contextEntities.forEach((entity, i) => {
    nodes.push({
      id: nodeIdOf(entity.key_block_id),
      type: 'timeline-key-block',
      position: { x: ORIGIN_X + i * CONTEXT_STEP_X, y: CONTEXT_CLUSTER_Y },
      data: entityToTimelineNodeData(entity, 'context', undefined),
    });
  });

  const computeNodes = mergeComputeEvents(
    events ?? [],
    graph,
    datedEvents.length,
    moduleNames,
    undatedEvents.length,
  );
  nodes.push(...computeNodes);

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

/**
 * Mirrors the Work Timeline adapter's `projectMomentLayer` EXACTLY — same
 * node-id prefix (`wt-scene:` / `wt-beat:`), same layout metrics, same
 * fixture-slot pattern, same node types (World-Moment feel ≡ Work-Moment
 * feel per V1.123 layer-feel-differentiation §2.4 — no new node component
 * family).
 *
 * Carrier (V1.200 DR-26): `SceneBeatFixturePayload`, composed by the
 * orchestrator from the bound Works' canonical `WorkOutline.scenes[]` /
 * `.beats[]` (each entry tagged with its owning Work id), or supplied
 * EXPLICITLY by Design Studio / tests through `sceneBeatFixture`. An
 * explicitly supplied payload is projected verbatim; omitting it means
 * "project the composed canonical carrier", never an automatic empty-data
 * substitution. An empty payload emits zero nodes (honest empty-state per
 * product semantics PD-3 — World Timeline Moment is a READ/projection
 * layer: Moments remain Work-owned, this is NOT a World Moment authoring
 * surface, no World-owned Moment write flow).
 *
 * Node attribution (V1.200 DR-26): a scene/beat coming from a bound Work
 * carries that Work's id into `WorkTimelineNodeData.workId`; an explicitly
 * injected fixture entry without provenance falls back to the supplied
 * `worldId`. (The World is never identified by the first KB entity — an
 * empty World KB graph has no entity, and a multi-Work World has no single
 * answer. No current consumer reads `workId` on Moment nodes: the node
 * components + the World Moment inspector render without it, and that
 * inspector deliberately renders NO Edit-in-Outline CTA.)
 *
 * Product semantics (PD-3 — HARD): primary readable set = Moments from
 * Works bound to that World, projected into the World Timeline. Node
 * payloads reuse `WorkTimelineNodeData` (the Work-owned carrier) so the
 * shared node components render unchanged.
 *
 * Scenes stack vertically by chapter order (numeric `chapterId` ascending);
 * beats stack vertically inside their scene. The X coordinate groups scenes
 * by chapter region so the chapter→scene→beat hierarchy reads spatially.
 *
 * `simplify:` deterministic vertical stack. P4 may swap in a richer
 * manuscript-aware layout (anchored scene-card grid) per layer-feel §9.
 */
/** One vertical scene-stack region: a Work's chapters are Work-local. */
interface MomentChapterGroup {
  workId: string | undefined;
  chapterId: number;
  scenes: SceneFixture[];
}

function disambiguatedWorkHandles(workIds: string[]): Map<string, string> {
  const bare = workIds.map((id) => (id.startsWith('wrk_') ? id.slice(4) : id));
  const maxLen = Math.max(...bare.map((b) => b.length));
  for (let len = 8; len < maxLen; len++) {
    const candidates = bare.map((b) => b.slice(0, len));
    if (new Set(candidates).size === candidates.length) {
      return new Map(workIds.map((id, i) => [id, candidates[i]]));
    }
  }
  return new Map(workIds.map((id, i) => [id, bare[i]]));
}

function projectMomentLayer(
  fixture: SceneBeatFixturePayload | undefined,
  worldId: string | undefined,
): {
  nodes: Node<WorkTimelineNodeData>[];
  edges: Edge<TimelineEdgeData>[];
} {
  if (!fixture || (fixture.scenes.length === 0 && fixture.beats.length === 0)) {
    return { nodes: [], edges: [] };
  }

  const groupsByKey = new Map<string, MomentChapterGroup>();
  for (const scene of fixture.scenes) {
    const key = `${scene.workId ?? ''}#${scene.chapterId}`;
    const bucket = groupsByKey.get(key);
    if (bucket) bucket.scenes.push(scene);
    else {
      groupsByKey.set(key, {
        workId: scene.workId,
        chapterId: scene.chapterId,
        scenes: [scene],
      });
    }
  }
  const groups = [...groupsByKey.values()].sort((a, b) => {
    const aWork = a.workId ?? '';
    const bWork = b.workId ?? '';
    if (aWork !== bWork) return aWork.localeCompare(bWork);
    return a.chapterId - b.chapterId;
  });

  const emittedSceneIds = new Set<string>();
  const nodes: Node<WorkTimelineNodeData>[] = [];

  groups.forEach((group, groupIdx) => {
    const sortedScenes = [...group.scenes].sort((a, b) => a.sceneId.localeCompare(b.sceneId));
    sortedScenes.forEach((scene, sceneIdx) => {
      emittedSceneIds.add(scene.sceneId);
      const data: WorkTimelineNodeData = {
        workId: scene.workId ?? worldId ?? '',
        nodeKind: 'scene',
        nodeId: scene.sceneId,
        sceneId: scene.sceneId,
        label: scene.title ?? '',
        status: scene.status,
        manuscriptAnchor: { chapterId: group.chapterId, sceneId: scene.sceneId },
        realizesChapterId: group.chapterId,
      };
      nodes.push({
        id: momentSceneNodeId(scene.sceneId),
        type: 'work-timeline-moment-scene',
        position: {
          x: MOMENT_ORIGIN_X + groupIdx * MOMENT_CHAPTER_STEP_X,
          y: MOMENT_ORIGIN_Y + sceneIdx * MOMENT_SCENE_STEP_Y,
        },
        data,
      });
    });
  });

  const beatsByScene = new Map<string, BeatFixture[]>();
  for (const beat of fixture.beats) {
    if (!emittedSceneIds.has(beat.sceneId)) continue;
    const bucket = beatsByScene.get(beat.sceneId);
    if (bucket) bucket.push(beat);
    else beatsByScene.set(beat.sceneId, [beat]);
  }

  const scenePositionById = new Map<string, { x: number; y: number; chapterId: number }>();
  for (const node of nodes) {
    if (node.data.nodeKind === 'scene') {
      const d = node.data as WorkTimelineNodeData;
      scenePositionById.set(d.sceneId!, {
        x: node.position.x,
        y: node.position.y,
        chapterId: d.realizesChapterId ?? 0,
      });
    }
  }

  for (const [sceneId, beats] of beatsByScene) {
    const scenePos = scenePositionById.get(sceneId);
    if (!scenePos) continue;
    const sortedBeats = [...beats].sort((a, b) => a.beatId.localeCompare(b.beatId));
    sortedBeats.forEach((beat, beatIdx) => {
      const data: WorkTimelineNodeData = {
        workId: beat.workId ?? worldId ?? '',
        nodeKind: 'beat',
        nodeId: beat.beatId,
        beatId: beat.beatId,
        sceneId: beat.sceneId,
        label: beat.title ?? '',
        status: beat.status,
        manuscriptAnchor: {
          chapterId: scenePos.chapterId,
          sceneId: beat.sceneId,
          beatId: beat.beatId,
        },
        realizesChapterId: scenePos.chapterId,
      };
      nodes.push({
        id: momentBeatNodeId(beat.beatId),
        type: 'work-timeline-moment-beat',
        position: {
          x: scenePos.x + 16,
          y: scenePos.y + MOMENT_BEAT_ORIGIN_Y + beatIdx * MOMENT_BEAT_STEP_Y,
        },
        data,
      });
    });
  }

  // Density-encoded: segment length proportional to scene count per ND-A1.
  // Mirrors the Work Timeline Moment spine (same rhythm break from the
  // Brief+Narrative time-span convention).
  if (groups.length > 0) {
    const chapterOwners = new Map<number, string[]>();
    for (const group of groups) {
      if (!group.workId) continue;
      const owners = chapterOwners.get(group.chapterId);
      if (owners) owners.push(group.workId);
      else chapterOwners.set(group.chapterId, [group.workId]);
    }
    const handlesByChapter = new Map<number, Map<string, string>>();
    for (const [chapterId, owners] of chapterOwners) {
      if (owners.length > 1) handlesByChapter.set(chapterId, disambiguatedWorkHandles(owners));
    }
    const chapterSegments: MomentSpineConfig['chapterSegments'] = groups.map((group) => {
      const handle = group.workId
        ? handlesByChapter.get(group.chapterId)?.get(group.workId)
        : undefined;
      return {
        chapterId: group.chapterId,
        workId: group.workId,
        chapterLabel: handle ? `Ch. ${group.chapterId} · ${handle}` : `Ch. ${group.chapterId}`,
        sceneCount: group.scenes.length,
        sceneTicks: group.scenes.map((s) => s.sceneId),
      };
    });
    const momentSpineData: DirectedAxisSpineNodeData = {
      layer: 'moment',
      spineConfig: {
        kind: 'moment',
        chapterSegments,
      },
      accentColor: 'var(--color-canvas-layer-moment-accent)',
    };
    nodes.push({
      id: 'directed-axis-spine',
      type: 'directedAxisSpine',
      position: { x: 0, y: MOMENT_ORIGIN_Y + SPINE_Y_OFFSET },
      data: momentSpineData as unknown as WorkTimelineNodeData,
      selectable: false,
      focusable: false,
    });
  }

  return { nodes, edges: [] };
}

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

function relationshipLabel(rel: WorldKbRelationshipProjection): string {
  if (rel.relation_type === 'custom' && rel.custom_label) return rel.custom_label;
  return RELATIONSHIP_KIND_LABELS[rel.relation_type] ?? rel.relation_type;
}

const ORDERING_DISCLAIMER =
  'Ordering inferred from available event data; not a canonical chronology.';

/**
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
  const computeCount = (computeEvents ?? []).filter(isMergeableComputeEvent).length;
  if (computeCount > 0) {
    parts.push(
      `${computeCount} ${computeCount === 1 ? 'compute event' : 'compute events'}`,
    );
  }

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

  if (events.length > 0) {
    summary = `${summary} ${ORDERING_DISCLAIMER}`;
  }

  return summary;
}

/**
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
 *   - Conflict UX is orchestrator-owned (mirrors World KB). Use
 *     `extractConflict(error)` for the typed parse.
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
  sceneBeatFixture?: SceneBeatFixturePayload,
): TimelineCanvasAdapter {
  return {
    surfaceKind: 'timeline',
    nodeTypes: {
      ...timelineNodeTypes,
      'work-timeline-moment-scene': workTimelineNodeTypes['work-timeline-moment-scene'],
      'work-timeline-moment-beat': workTimelineNodeTypes['work-timeline-moment-beat'],
    },
    edgeTypes: undefined,
    layoutOptions:
      activeLayer === 'brief'
        ? BRIEF_LAYOUT_OPTIONS
        : activeLayer === 'moment'
          ? MOMENT_LAYOUT_OPTIONS
          : NARRATIVE_LAYOUT_OPTIONS,

    projectGraph(graph) {
      // V1.147 P2 T3 — the captured `timelineEvents` merge into the Narrative
      // projection; Brief ignores them. `computeModuleNames` resolves module
      // display names from the registry map (module_id fallback).
      //
      // V1.156 P1 T1 — the Moment projection reads the bound-Works
      // Scene/Beat fixture (fixture-driven read-projection; honest
      // empty-state when absent).
      //
      // V1.156 P1 fix-wave 1 (F3) — the fixture is CAPTURED at factory
      // creation (like `activeLayer` + `timelineEvents`) so the
      // orchestrator's adapter memo deps can include it: a fixture identity
      // change recreates the adapter and re-projects deterministically on
      // the same render. (Reading only via `ctxRef.current` left the
      // projection stale — the ctxRef assignment runs AFTER the adapter
      // memo, so a deps-only change would re-project with the previous
      // render's fixture.) The ctxRef slot remains as a fallback for direct
      // ctxRef wiring (tests / legacy callers); the captured value wins.
      return projectTimelineGraph(
        graph,
        activeLayer,
        timelineEvents,
        computeModuleNames,
        sceneBeatFixture ?? ctxRef.current.sceneBeatFixture,
        ctxRef.current.worldId,
      );
    },

    renderInspector(node) {
      const data = node.data as TimelineNodeData;
      if (data.layoutHint === 'brief') {
        return <TimelineBriefEraInspector node={node} ctxRef={ctxRef} />;
      }
      if (data.layoutHint === 'compute') {
        return <TimelineComputeInspector node={node} ctxRef={ctxRef} />;
      }
      const momentData = node.data as unknown as WorkTimelineNodeData;
      if (momentData.nodeKind === 'scene' || momentData.nodeKind === 'beat') {
        return renderTimelineMomentInspector(
          node as unknown as Node<WorkTimelineNodeData>,
        );
      }
      return <TimelineInspector node={node} ctxRef={ctxRef} />;
    },

    renderAltView() {
      return <TimelineAltViewWrapper ctxRef={ctxRef} />;
    },

    summarizeGraph(graph) {
      return summarizeTimelineGraph(graph, timelineEvents);
     },

    renderBriefTimeBands(graph) {
      const tree = buildEraTree(
        graph.entities ?? [],
        graph.relationships ?? [],
      );
      return (
        <BriefTimeBands
          tree={tree}
          onSelectEra={(eraId) => ctxRef.current.onSelectNode?.(nodeIdOf(eraId))}
        />
      );
    },
  };
}

/**
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
  const entityNodes = (ctx.nodes ?? []).filter(
    (n) => typeof n.data.block_type === 'string',
  );
  return (
    <TimelineAltView
      nodes={entityNodes}
      selectedNodeId={ctx.selectedNodeId ?? null}
      onSelectNode={(nodeId) => ctx.onSelectNode?.(nodeId)}
    />
  );
}

interface WorldKbConflictDetailsShape {
  current_version?: number;
  entity_id?: string;
  conflicting_path?: string;
  recovery_hint?: string;
}

interface WorldKbValidationDetailsShape {
  validation_summary?: { errors?: string[] };
}

type WorldKbErrorCode =
  | 'world_kb_conflict'
  | 'world_kb_validation_failed';

interface NexusClientErrorLike {
  status?: number;
  code?: string;
  details?: unknown;
}

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
 * v1.203 P2 O3 — map one daemon 422 entry onto a Timeline inspector form
 * field when it carries the frozen prefix grammar
 * (`modules.<dialect>[.<index>].<field>: <reason>`, write contract §3). The
 * Timeline event inspector authors exactly one dialect — `observation` — and
 * its complete editable member set (`observers` / `access`) maps 1:1; entries
 * addressing anything else (mental / belief / unknown observation members)
 * stay section-level with the original entry text verbatim, mirroring T2's
 * `mapValidationEntry` in the entity inspector. Pure: no React.
 */
export function mapTimelineValidationEntry(entry: string): {
  key?: string;
  message: string;
} {
  if (!entry.startsWith('modules.')) return { message: entry };
  const sep = entry.indexOf(': ');
  if (sep === -1) return { message: entry };
  const addr = entry.slice('modules.'.length, sep);
  const reason = entry.slice(sep + 2);
  const parts = addr.split('.');
  if (
    parts[0] === 'observation' &&
    parts.length === 2 &&
    (parts[1] === 'observers' || parts[1] === 'access')
  ) {
    return { key: `observation.${parts[1]}`, message: reason };
  }
  return { message: entry };
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
