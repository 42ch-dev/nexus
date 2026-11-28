/**
 * Projects a Work's V1.72 `WorkOutline` onto the Work Timeline surface
 * (`CanvasSurfaceKind = "work-timeline"` — Task 5 promotes the string
 * literal to a real enum value; until then this adapter casts so the
 * contract stays type-compatible).
 *
 * Architect-locked contract — see
 * `iterations/v1.123/specs/three-layer-architecture.md` §3 + §6 + §7 + §8 +
 * `iterations/v1.123/specs/layer-feel-differentiation.md` §2.3 + §2.4 +
 * `canvas-strategy-surface.md` §3.3.3 V1.156 amendment:
 *   - Single graph source: `WorkOutline` (V1.72 shipped). No wrapper, no
 *     join with other DTOs (`WorkTimelineGraph = WorkOutline`).
 *   - **Narrative layer** (Task 2): `outline.timeline_events[]` →
 *     `WorkTimelineNarrativeEventNode` on the Work Timeline Narrative
 *     when-axis, sorted by `realizes_chapter_id` (ascending, undefined
 *     tailing) then `event_id` (lexicographic tiebreaker). Events without
 *     a chapter anchor tail right on the same axis — the Work Timeline
 *     has no Context-cluster lane (Work Timeline is Work-scoped, not
 *     World-scoped). `foreshadows[]` → `Edge<relation: 'foreshadows'>`
 *     event→event.
 *   - **Moment layer** (Task 3): the canonical `WorkOutline.scenes[]` /
 *     `beats[]` carrier (V1.200 DR-26 — mapped snake→camel through
 *     `sceneBeatPayloadFromOutline`), projected via V1.108
 *     `OutlineSceneNodeData` / `OutlineBeatNodeData`. Vertical scene-stack
 *     (TB layout direction per `layer-feel-differentiation.md` §2.4).
 *     Honest empty-state only when the canonical arrays are empty; the
 *     `sceneBeatFixture` slot survives as an explicit Studio/test
 *     injection mode, never an automatic empty-data fallback.
 *   - **Brief layer** (V1.156 P2 T1): read-only **projection** of the
 *     bound World's Brief (`WorldKbGraphResponse.entities[block_type=era]`,
 *     V1.73) onto the Work Timeline Brief when-axis — Work-Brief feel ≡
 *     World-Brief feel (same `TimelineNodeData` with `layoutHint: 'brief'`,
 *     same `timeline-brief-era` node type + directed-axis spine). Brief
 *     remains World spine (PD-2); the Work does NOT gain an authored Brief
 *     and no Work-owned Brief write flow exists. The bound World's graph
 *     is supplied separately (factory param / ctxRef slot) — it is NOT
 *     part of `WorkTimelineGraph`. Honest empty-state when no bound World /
 *     no era data.
 *
 * Layer model: `projectGraphForLayer(graph, 'brief' | 'narrative' |
 * 'moment')` selects the active layer. The default `projectGraph(graph)`
 * delegates to the adapter's active layer, which defaults to `'narrative'`
 * per architect §7.3 UX-risk override. V1.200 keeps that default unchanged
 * (Task 3 scope): flipping to a Moment default is a UX decision for a later
 * iteration now that the canonical carrier exists.
 *
 * Write boundary (architect-locked §6 — read-only in V1.123 P2): the Work
 * Timeline surface performs NO direct writes. Edits route through the
 * Outline surface (`outline.patch_chapter` / `outline.patch_structure`,
 * V1.72) via the `onEditInOutline` callback; the orchestrator owns the
 * navigation. The Work Timeline adapter therefore ships no `onPatch*`
 * callback in its context — only navigation hand-offs.
 *
 * Conflict policy (architect-locked §6): reuses `OutlineConflictError`
 * (HTTP 409) + `OutlineValidationError` (HTTP 422); no Work-Timeline-
 * specific conflict DTO. The orchestrator renders an Outline-flavored
 * conflict modal when writes (from the Outline surface) return 409/422.
 *
 * `wire_contracts_changed: false for P2` — HISTORICAL (V1.123 P2, version-
 * scoped): Moment-on-Outline was frontend-only then; that P2 added zero
 * `schemas/` / `crates/` / `packages/nexus-contracts` diff. V1.200 (DR-26)
 * IS a wire change: `WorkOutline` now carries `scenes[]` / `beats[]` and
 * this adapter projects them.
 *
 * V1.156 P2 T1 — `wire_contracts_changed: false`: the Work Timeline Brief
 * layer is frontend-only. `WorkTimelineLayer` gains `'brief'` as a UI-only
 * union value (not a wire DTO); the Brief projection reuses the V1.73
 * `GET /worlds/{id}/kb/graph` route + V1.123 Brief carrier
 * (`WorldKbGraphResponse.entities[block_type=era]`) verbatim. No schema /
 * codegen / daemon / contracts diff.
 */
import type { MutableRefObject, ReactNode } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type { WorkOutline, WorldKbGraphResponse } from '@42ch/nexus-contracts';

import type {
  BeatFixture,
  OutlineSceneStatus,
  SceneBeatFixturePayload,
  SceneFixture,
} from '../outline-canvas/graph-projection';
import { sceneBeatPayloadFromOutline } from '../outline-canvas/graph-projection';
import type { DirectedAxisSpineNodeData, MomentSpineConfig, NarrativeSpineConfig } from '../timeline-canvas/directed-axis-spine';
import { SPINE_Y_OFFSET } from '../timeline-canvas/directed-axis-spine';
import { buildEraTree } from '../timeline-canvas/brief-era-tree';
import { BriefTimeBands } from '../timeline-canvas/brief-time-bands';
import {
  BRIEF_LAYOUT_OPTIONS,
  projectBriefLayer as projectWorldBriefLayer,
} from '../timeline-canvas/timeline-canvas-adapter';
import { timelineNodeTypes } from '../timeline-canvas/timeline-node-types';
import { workTimelineNodeTypes } from './work-timeline-node-types';
import { renderWorkTimelineInspector } from './work-timeline-inspector';

/** Single graph source — no wrapper, no join. Mirrors V1.122 §3.1. */
export type WorkTimelineGraph = WorkOutline;

/**
 * Layer kind on the Work Timeline surface (architect §7.1 + V1.156 §3.3.3
 * amendment — 3×2 matrix completion).
 *
 * - `'brief'`    — V1.156 P2 T1: read-only projection of the bound World's
 *                  Brief (`WorldKbGraphResponse.entities[block_type=era]`,
 *                  V1.73). Brief remains World spine (PD-2); the Work does
 *                  NOT gain an authored Brief. Work-Brief feel ≡ World-Brief
 *                  feel (same `TimelineNodeData` with `layoutHint: 'brief'`).
 * - `'narrative'` — Work-scoped events from `outline.timeline_events[]`.
 * - `'moment'`    — Scene/Beat precision from outline Scene/Beat data
 *                   (fixture-driven today; V1.124+ wire upgrade tracked
 *                   as `DF-V1123-MOMENT-WIRE`).
 *
 * UI-only union type — NOT a wire DTO (`wire_contracts_changed: false`).
 */
export type WorkTimelineLayer = 'brief' | 'narrative' | 'moment';

/**
 * Node data payload for the Work Timeline surface.
 *
 * Discriminated by `nodeKind`:
 *   - `'event'`  for Narrative layer (Task 2).
 *   - `'scene'`  for Moment layer scene cards (Task 3).
 *   - `'beat'`   for Moment layer beat pins (Task 3).
 *
 * V1.156 P2 T1 — Brief layer nodes do NOT carry this payload: Work-Brief
 * reuses the World Timeline's `TimelineNodeData` carrier (`layoutHint:
 * 'brief'`, spread `WorldKbEntityProjection` + era markers). The Brief
 * projection's `Node<TimelineNodeData>[]` result is cast to this surface's
 * `Node<WorkTimelineNodeData>[]` (deliberate cross-surface reuse — the Work
 * surface never reads `WorkTimelineNodeData` fields on Brief nodes).
 *
 * Optional fields are present conditionally on the discriminator. The
 * `[key: string]: unknown` index signature satisfies React Flow's
 * `Node<TNodeData extends Record<string, unknown>>` constraint.
 */
export interface WorkTimelineNodeData {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  /** Work id the node belongs to. */
  workId: string;
  /** Discriminator for node rendering: 'event' for Narrative; 'scene'/'beat' for Moment. */
  nodeKind: 'event' | 'scene' | 'beat';
  /** Stable node id source (event_id for Narrative; sceneId/beatId for Moment). */
  nodeId: string;
  /** Human-readable label (event title / scene title / beat title). */
  label: string;
  /** Optional description (event description / scene summary / beat summary). */
  description?: string;
  /** Chapter this node realizes (event realizes_chapter_id; scene chapterId). */
  realizesChapterId?: number;
  /**
   * Present for scene + beat nodes when anchor data exists.
   */
  manuscriptAnchor?: {
    chapterId: number;
    sceneId?: string;
    beatId?: string;
  };
  /** Event id (Narrative layer only). */
  eventId?: string;
  worldEventId?: string;
  /** Scene id (Moment layer only). */
  sceneId?: string;
  /** Beat id (Moment layer only). */
  beatId?: string;
  /** OutlineSceneStatus (Moment layer only; null/undefined → no status chip). */
  status?: OutlineSceneStatus | null;
}

/**
 * Edge data payload for the Work Timeline surface.
 *
 * Reuses the V1.72 outline edge semantics without inventing a new edge DTO
 * family. `realizes_event` / `foreshadows` are Work-outline projection
 * labels (same as V1.108); the Work Timeline surface introduces NO new
 * edge types.
 */
export type WorkTimelineEdgeData = {
  /** React Flow requires an index signature on edge data. */
  [key: string]: unknown;
  /** Edge relation: 'realizes_event' (chapter → event) or 'foreshadows' (event → event). */
  relation: 'realizes_event' | 'foreshadows';
  /** Stable edge id source (event_id pair). */
  sourceEventId?: string;
  targetEventId?: string;
};

/**
 * The Work Timeline adapter is read-only in V1.123 (architect §6). Writes
 * route through the Outline surface (`outline.patch_chapter` /
 * `outline.patch_structure`, V1.72) via the `onEditInOutline` callback;
 * the orchestrator owns the navigation.
 */
export interface WorkTimelineCanvasAdapterContext {
  workId: string;
  /** Optional bound World id (for P3 cross-surface navigation to World Timeline). */
  worldId?: string;
  /**
   * The Work Timeline Brief layer projects the bound World's Brief
   * (`WorldKbGraphResponse.entities[block_type=era]`) — Brief is World
   * spine; Work-Brief is a read-only projection (PD-2). The orchestrator
   * fetches it from the bound World's `GET /v1/daemon/worlds/{world_id}/kb/graph`
   * (V1.73, no new route) via `Work.world_id`. Undefined when no World is
   * bound or the graph is still loading → the Brief layer emits honest
   * empty-state (zero nodes). The factory also captures it as a parameter
   * (captured value wins; this slot is the ctxRef fallback for direct
   * wiring/tests, mirroring the World adapter's captured-fixture pattern).
   */
  boundWorldGraph?: WorldKbGraphResponse;
  client?: unknown;
  onEditInOutline?: (node: Node<WorkTimelineNodeData>) => void;
  onViewOnWorldTimeline?: (node: Node<WorkTimelineNodeData>) => void;
  /**
   * An explicitly supplied payload (populated OR deliberately empty) is
   * projected verbatim and never acts as an automatic empty-data fallback:
   * omitting this slot means "project the canonical wire arrays", not
   * "substitute empty data".
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

export type WorkTimelineCanvasAdapter = CanvasSurfaceAdapter<
  WorkTimelineGraph,
  WorkTimelineNodeData,
  WorkTimelineEdgeData
> & {
  renderBriefTimeBands?(graph: WorkTimelineGraph): ReactNode;
};

/**
 * The Work Timeline adapter extends the base V1.114 `CanvasSurfaceAdapter`
 * with layer-aware projection (architect §7.1). The base `projectGraph(graph)`
 * MUST delegate to the active layer so `useCanvasSurface()` (V1.114) composes
 * unchanged.
 *
 * `defaultLayer: 'narrative'` — architect UX-risk override §7.3. The V1.72
 * `WorkOutline` wire has no Scene/Beat data today, so Moment-default would
 * surface persistent empty-state in nearly all real Works. Narrative-default
 * with Moment one click away is the safer V1.123 position. When the
 * WorkOutline wire extends to expose scenes/beats (V1.124+
 * `DF-V1123-MOMENT-WIRE`), this default may flip to Moment at that time
 * without breaking the adapter contract.
 */
export interface WorkTimelineLayerAdapter extends WorkTimelineCanvasAdapter {
  /** Work Timeline surface kind (architect §7.1). */
  surfaceKind: 'work-timeline';
  /** Default layer on Work Timeline entry. Architect-locked: 'narrative' (§7.3). */
  defaultLayer: 'narrative';
  /** Project the graph for a specific layer. */
  projectGraphForLayer(
    graph: WorkTimelineGraph,
    layer: WorkTimelineLayer,
  ): { nodes: Node<WorkTimelineNodeData>[]; edges: Edge<WorkTimelineEdgeData>[] };
  /** Inherited from V1.114 CanvasSurfaceAdapter — MUST delegate to active layer. */
  projectGraph(
    graph: WorkTimelineGraph,
  ): { nodes: Node<WorkTimelineNodeData>[]; edges: Edge<WorkTimelineEdgeData>[] };
}

/**
 * The adapter sets `layoutOptions.hasSuppliedPositions = true` so
 * `useAutoLayout` honors these positions on first open and does NOT collapse
 * the projection onto dagre's generic graph layout. The author can still
 * trigger an explicit `relayout()` to force dagre.
 *
 * `simplify:` deterministic metrics mirroring the V1.122 Timeline adapter's
 * `WHEN_AXIS_Y` / `EVENT_STEP_X` constants. Replace with a chapter-aware
 * layout plugin if the Work Timeline grows beyond ~30 events per chapter.
 */
const NARRATIVE_AXIS_Y = 0;
const NARRATIVE_ORIGIN_X = 40;
const NARRATIVE_EVENT_STEP_X = 280;

/**
 * Moment layer vertical scene-stack metrics (per layer-feel §2.4). Scenes
 * stack top-to-bottom by chapter order; beats stack top-to-bottom inside
 * each scene. The X coordinate groups scenes by chapter region so the
 * chapter→scene→beat hierarchy reads spatially.
 *
 * `simplify:` deterministic vertical stack. P4 may swap in a richer
 * manuscript-aware layout (anchored scene-card grid) per layer-feel §9.
 */
const MOMENT_ORIGIN_X = 40;
const MOMENT_ORIGIN_Y = 40;
const MOMENT_SCENE_STEP_Y = 160;
const MOMENT_BEAT_ORIGIN_Y = 56;
const MOMENT_BEAT_STEP_Y = 44;
const MOMENT_CHAPTER_STEP_X = 360;

/**
 *
 * `layer-feel-differentiation.md` §2.3 + §2.4 locks the feel: Narrative
 * inherits the V1.122 LR balanced timeline; Moment prefers a vertical
 * scene-stack (TB direction) with **tighter** rankSep / nodeSep so the
 * chapter→scene→beat hierarchy reads as a dense manuscript stack (Plan
 * Task 6 Step 2: "tight `nodeSep` (e.g., 30), `rankSep` (e.g., 60)").
 * The `hasSuppliedPositions: true` flag is preserved on both layers so
 * the adapter's deterministic positions survive first open; these
 * `direction` / `rankSep` / `nodeSep` values only take effect on an
 * explicit `relayout()`.
 *
 * Narrative does NOT carry Moment-specific rankSep/nodeSep — its LR baseline
 * inherits V1.122 default spacing. The differentiation axis is direction +
 * Moment density, not Narrative re-tuning.
 */
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

const NARRATIVE_NODE_ID_PREFIX = 'wt-event:';
const MOMENT_SCENE_NODE_ID_PREFIX = 'wt-scene:';
const MOMENT_BEAT_NODE_ID_PREFIX = 'wt-beat:';

/**
 * V1.163 P1 Task 3 — exported for the orchestrator's inbound `?event=` focus
 * (architect lock: Work Timeline selects `wt-event:${eventParam}` after
 * projection). Mirrors the World adapter's exported `nodeIdOf`.
 */
export function narrativeEventNodeId(eventId: string): string {
  return `${NARRATIVE_NODE_ID_PREFIX}${eventId}`;
}
function momentSceneNodeId(sceneId: string): string {
  return `${MOMENT_SCENE_NODE_ID_PREFIX}${sceneId}`;
}
function momentBeatNodeId(beatId: string): string {
  return `${MOMENT_BEAT_NODE_ID_PREFIX}${beatId}`;
}

/**
 * `foreshadows[]` → `Edge<relation: 'foreshadows'>` event→event. Dangling
 * edges (source or target event absent) are dropped — mirrors the V1.108
 * outline projection's dangling-edge guard.
 *
 * The architect §7 invariant "do not fabricate chronology" is honored by
 * surfacing the ordering disclaimer in `summarizeWorkTimelineGraph`
 * whenever event entities are rendered. Chapter-anchor sort is a
 * structural hint, not a canonical chronology.
 */
function projectNarrativeLayer(graph: WorkTimelineGraph): {
  nodes: Node<WorkTimelineNodeData>[];
  edges: Edge<WorkTimelineEdgeData>[];
} {
  const events = graph.timeline_events ?? [];

  const sorted = [...events].sort((a, b) => {
    const aCh = a.realizes_chapter_id;
    const bCh = b.realizes_chapter_id;
    if (aCh === undefined && bCh === undefined) {
      return a.event_id.localeCompare(b.event_id);
    }
    if (aCh === undefined) return 1; // a tails
    if (bCh === undefined) return -1; // b tails
    if (aCh !== bCh) return aCh - bCh;
    return a.event_id.localeCompare(b.event_id);
  });

  const nodes: Node<WorkTimelineNodeData>[] = sorted.map((evt, index) => {
    const data: WorkTimelineNodeData = {
      workId: graph.work_id,
      nodeKind: 'event',
      nodeId: evt.event_id,
      eventId: evt.event_id,
      label: evt.title,
    };
    if (evt.description !== undefined && evt.description !== null) {
      data.description = evt.description;
    }
    if (evt.realizes_chapter_id !== undefined && evt.realizes_chapter_id !== null) {
      data.realizesChapterId = evt.realizes_chapter_id;
      data.manuscriptAnchor = { chapterId: evt.realizes_chapter_id };
    }
    // Present only when the outline event realizes a World Narrative event;
    // the inspector's CTA deep-links to the specific World event when set.
    if (evt.world_event_id !== undefined && evt.world_event_id !== null) {
      data.worldEventId = evt.world_event_id;
    }
    return {
      id: narrativeEventNodeId(evt.event_id),
      type: 'work-timeline-narrative-event',
      position: {
        x: NARRATIVE_ORIGIN_X + index * NARRATIVE_EVENT_STEP_X,
        y: NARRATIVE_AXIS_Y,
      },
      data,
    };
  });

  if (sorted.length > 0) {
    const tickTimestamps: NarrativeSpineConfig['tickTimestamps'] = sorted.map(
      (evt) => evt.event_id,
    );
    const narrativeSpineData: DirectedAxisSpineNodeData = {
      layer: 'narrative',
      spineConfig: { kind: 'narrative', tickTimestamps },
      accentColor: 'var(--color-canvas-layer-narrative-accent)',
    };
    nodes.push({
      id: 'directed-axis-spine',
      type: 'directedAxisSpine',
      position: { x: 0, y: NARRATIVE_AXIS_Y + SPINE_Y_OFFSET },
      data: narrativeSpineData as unknown as WorkTimelineNodeData,
      selectable: false,
      focusable: false,
    });
  }

  const edges = deriveForeshadowEdges(graph);

  return { nodes, edges };
}

/**
 * Architect §6.2: the Work Timeline surface introduces NO new edge types.
 * `realizes_event` is reserved for future chapter-context clustering; the
 * Narrative layer emits `foreshadows` only on the Work Timeline today.
 */
function deriveForeshadowEdges(graph: WorkTimelineGraph): Edge<WorkTimelineEdgeData>[] {
  const events = graph.timeline_events ?? [];
  const knownEventIds = new Set(events.map((e) => e.event_id));
  const foreshadows = graph.foreshadows ?? [];

  const edges: Edge<WorkTimelineEdgeData>[] = [];
  for (const link of foreshadows) {
    if (!knownEventIds.has(link.source_event_id)) continue;
    if (!knownEventIds.has(link.target_event_id)) continue;
    const data: WorkTimelineEdgeData = {
      relation: 'foreshadows',
      sourceEventId: link.source_event_id,
      targetEventId: link.target_event_id,
    };
    edges.push({
      id: `wt-foreshadow:${link.source_event_id}:${link.target_event_id}`,
      source: narrativeEventNodeId(link.source_event_id),
      target: narrativeEventNodeId(link.target_event_id),
      type: 'straight',
      data,
      selectable: false,
      focusable: true,
      style: { stroke: 'var(--color-canvas-outline-foreshadow-edge)' },
    });
  }
  return edges;
}

/**
 * Carrier (V1.200 DR-26 swap): the canonical `WorkOutline.scenes[]` /
 * `.beats[]` wire arrays, mapped snake→camel through
 * {@link sceneBeatPayloadFromOutline}. Empty canonical arrays mean real
 * emptiness — zero nodes, honest empty-state (product spec §4.5 — Task 7
 * owns the visible copy; the orchestrator renders the panel).
 *
 * `ctxRef.current.sceneBeatFixture` stays as an EXPLICIT injection mode for
 * Design Studio / component tests: an explicitly supplied payload (populated
 * OR deliberately empty) is projected verbatim and is never an automatic
 * empty-data fallback. Production callers pass no fixture.
 *
 * Scenes stack vertically by chapter order (numeric `chapterId` ascending);
 * beats stack vertically inside their scene. The X coordinate groups scenes
 * by chapter region so the chapter→scene→beat hierarchy reads spatially.
 *
 * `simplify:` deterministic vertical stack. P4 may swap in a richer
 * manuscript-aware layout (anchored scene-card grid) per layer-feel §9.
 */
function projectMomentLayer(
  graph: WorkTimelineGraph,
  fixture: SceneBeatFixturePayload | undefined,
): {
  nodes: Node<WorkTimelineNodeData>[];
  edges: Edge<WorkTimelineEdgeData>[];
} {
  const payload = fixture ?? sceneBeatPayloadFromOutline(graph);
  if (payload.scenes.length === 0 && payload.beats.length === 0) {
    return { nodes: [], edges: [] };
  }

  const scenesByChapter = new Map<number, SceneFixture[]>();
  for (const scene of payload.scenes) {
    const bucket = scenesByChapter.get(scene.chapterId);
    if (bucket) bucket.push(scene);
    else scenesByChapter.set(scene.chapterId, [scene]);
  }
  const sortedChapterIds = [...scenesByChapter.keys()].sort((a, b) => a - b);

  const emittedSceneIds = new Set<string>();
  const nodes: Node<WorkTimelineNodeData>[] = [];

  sortedChapterIds.forEach((chapterId, chapterIdx) => {
    const scenes = scenesByChapter.get(chapterId) ?? [];
    const sortedScenes = [...scenes].sort((a, b) => a.sceneId.localeCompare(b.sceneId));
    sortedScenes.forEach((scene, sceneIdx) => {
      emittedSceneIds.add(scene.sceneId);
      const data: WorkTimelineNodeData = {
        workId: graph.work_id,
        nodeKind: 'scene',
        nodeId: scene.sceneId,
        sceneId: scene.sceneId,
        label: scene.title ?? '',
        status: scene.status,
        manuscriptAnchor: { chapterId: scene.chapterId, sceneId: scene.sceneId },
        realizesChapterId: scene.chapterId,
      };
      nodes.push({
        id: momentSceneNodeId(scene.sceneId),
        type: 'work-timeline-moment-scene',
        position: {
          x: MOMENT_ORIGIN_X + chapterIdx * MOMENT_CHAPTER_STEP_X,
          y: MOMENT_ORIGIN_Y + sceneIdx * MOMENT_SCENE_STEP_Y,
        },
        data,
      });
    });
  });

  const beatsByScene = new Map<string, BeatFixture[]>();
  for (const beat of payload.beats) {
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
        workId: graph.work_id,
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
  // This is a deliberate rhythm break from Brief+Narrative's time-span
  // convention (see spec ND-A1).
  if (sortedChapterIds.length > 0) {
    const chapterSegments: MomentSpineConfig['chapterSegments'] = sortedChapterIds.map(
      (chapterId) => {
        const scenes = scenesByChapter.get(chapterId) ?? [];
        const sceneTicks = scenes.map((s) => s.sceneId);
        return {
          chapterId,
          chapterLabel: `Ch. ${chapterId}`,
          sceneCount: scenes.length,
          sceneTicks,
        };
      },
    );
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

/**
 * V1.156 P2 T1 — Work Timeline Brief layer projection. Completes the Work
 * Timeline 3-layer matrix (Brief + Narrative + Moment).
 *
 * Work-Brief = read-only **projection** of the bound World's Brief (PD-2):
 * Brief remains World spine; the Work does NOT gain an authored Brief and
 * there is NO Work-owned Brief write flow. The projection IS the World
 * Timeline Brief layer (`timeline-canvas-adapter.tsx::projectBriefLayer`,
 * which internally reuses `extractEraAttributes`) called verbatim — same
 * carrier (`WorldKbGraphResponse.entities[block_type=era]`, V1.73), same
 * node data (`TimelineNodeData` with `layoutHint: 'brief'`), same era
 * markers (`eraId` / `startHint` / `endHint` / `worldSummary`), same node
 * type (`timeline-brief-era`), same directed-axis spine — so Work-Brief
 * feel ≡ World-Brief feel (V1.123 layer-feel §2.2). No Work-relevant
 * filtering (architect LOCK — full bound-World Brief with source-World
 * attribution; tighten later if dogfood shows noise).
 *
 * The bound World's graph is NOT part of `WorkTimelineGraph`
 * (`WorkOutline`); it is supplied separately (captured factory param or
 * `ctxRef.current.boundWorldGraph`) and fetched by the orchestrator from
 * the bound World's `GET /v1/daemon/worlds/{world_id}/kb/graph` (V1.73 —
 * no new route, no schema change; `wire_contracts_changed: false`).
 *
 * Honest empty-state: no bound World / no bound-World graph → zero nodes
 * (the orchestrator owns the visible empty-state copy — T2).
 *
 * `simplify:` the cast is deliberate cross-surface reuse — the World
 * `projectBriefLayer` produces `Node<TimelineNodeData>` (the World carrier)
 * while the Work surface types nodes as `Node<WorkTimelineNodeData>`. The
 * Work surface never reads `WorkTimelineNodeData` fields on Brief nodes
 * (rendering dispatches on `node.type` / `layoutHint`; the Brief-era
 * inspector is orchestrator-owned in T2). Mirrors the World adapter's
 * Moment reuse of the Work carrier in reverse.
 */
function projectBriefLayer(
  boundWorldGraph: WorldKbGraphResponse | undefined,
): {
  nodes: Node<WorkTimelineNodeData>[];
  edges: Edge<WorkTimelineEdgeData>[];
} {
  if (!boundWorldGraph) {
    return { nodes: [], edges: [] };
  }
  return projectWorldBriefLayer(boundWorldGraph) as unknown as {
    nodes: Node<WorkTimelineNodeData>[];
    edges: Edge<WorkTimelineEdgeData>[];
  };
}

/**
 *
 * Narrative layer reads from `WorkOutline.timeline_events[]` (V1.72 wire).
 * Moment layer reads the canonical `WorkOutline.scenes[]` / `.beats[]`
 * carrier (V1.200 DR-26) unless an explicit `fixture` payload is supplied
 * (Design Studio / tests); empty canonical arrays emit honest empty-state
 * (zero nodes).
 * Brief layer (V1.156 P2 T1) reads the bound World's `WorldKbGraphResponse`
 * (`entities[block_type=era]`) passed via `boundWorldGraph`; when absent,
 * the Brief layer emits honest empty-state (zero nodes).
 *
 * Exposed publicly so layer-specific tests can call
 * `projectWorkTimelineGraph(graph, layer)` without instantiating the
 * adapter. The adapter's `projectGraph(graph)` delegates here via its
 * active layer.
 */
export function projectWorkTimelineGraph(
  graph: WorkTimelineGraph,
  layer: WorkTimelineLayer = 'narrative',
  fixture?: SceneBeatFixturePayload,
  boundWorldGraph?: WorldKbGraphResponse,
): {
  nodes: Node<WorkTimelineNodeData>[];
  edges: Edge<WorkTimelineEdgeData>[];
} {
  if (layer === 'brief') {
    return projectBriefLayer(boundWorldGraph);
  }
  if (layer === 'moment') {
    return projectMomentLayer(graph, fixture);
  }
  return projectNarrativeLayer(graph);
}

const ORDERING_DISCLAIMER =
  'Ordering inferred from chapter anchors and event ids; not a canonical chronology.';

/**
 * Build the screen-reader live-region summary for the Work Timeline canvas.
 *
 * The disclaimer is present whenever event entities are rendered (i.e. when
 * the outline has any `timeline_events[]`), and is omitted only for
 * zero-event outlines (which surface their own honest empty-state copy via
 * `<EmptyState>` per architect §7 — Task 7 owns the visible copy).
 *
 * Architect §7 invariant: chapter-anchor sort is a structural hint, not a
 * canonical chronology. Free-form event_ids are NOT canonical temporal
 * signals; the disclaimer must surface that honestly.
 *
 * `simplify:` plain English (no i18n) — same convention as the V1.122
 * Timeline `summarizeTimelineGraph`. The canvas a11y summary is an SR-only
 * live region, not a visible label. If a future iteration localises the
 * canvas a11y summary, mirror the change here.
 */
export function summarizeWorkTimelineGraph(graph: WorkTimelineGraph): string {
  const events = graph.timeline_events ?? [];
  const foreshadows = graph.foreshadows ?? [];
  const volumes = graph.volumes ?? [];

  const parts: string[] = [];
  parts.push(`${events.length} ${events.length === 1 ? 'event' : 'events'}`);
  parts.push(
    `${foreshadows.length} ${foreshadows.length === 1 ? 'foreshadow' : 'foreshadows'}`,
  );
  if (volumes.length > 0) {
    parts.push(
      `${volumes.length} ${volumes.length === 1 ? 'volume' : 'volumes'}`,
    );
  }

  let summary = `Work Timeline: ${parts.join(', ')}.`;

  if (events.length > 0) {
    summary = `${summary} ${ORDERING_DISCLAIMER}`;
  }

  return summary;
}

/**
 * The returned object MUST stay referentially stable across renders —
 * `useCanvasSurface` memoises on `adapter` and would otherwise re-project
 * on every orchestrator state change. The factory is therefore called
 * once per orchestrator mount (e.g. via `useMemo([activeLayer, ...], ...)`);
 * only `activeLayer` + `boundWorldGraph` (V1.156 P2 T1 — the Brief layer
 * era data source) invalidate the memo (Task 4 wires the layer swap; T2
 * adds the bound-World graph to the memo deps).
 *
 * `activeLayer` selects which projection `projectGraph(graph)` delegates
 * to. Default `'narrative'` per architect §7.3 UX-risk override.
 *
 * V1.156 P2 T1 — `boundWorldGraph` carries the bound World's KB graph for
 * the Brief layer (era entities, `WorldKbGraphResponse`, V1.73). Captured
 * at factory creation like `activeLayer` so the orchestrator's adapter
 * memo deps can include it (a graph identity change recreates the adapter
 * and re-projects deterministically on the same render — mirroring the
 * World adapter's captured-fixture pattern). The ctxRef slot
 * (`ctxRef.current.boundWorldGraph`) remains as a fallback for direct
 * ctxRef wiring (tests / legacy callers); the captured value wins.
 *
 * Task 5 promotes `'work-timeline'` to a real `CanvasSurfaceKind` enum
 * value. Until then this adapter casts the string literal so the contract
 * stays type-compatible with V1.114 `CanvasSurfaceAdapter` consumers
 * (`CanvasShell` does not enforce surface registration at runtime — it
 * routes by adapter, not by an in-shell registry).
 */
export function createWorkTimelineCanvasAdapter(
  ctxRef: MutableRefObject<WorkTimelineCanvasAdapterContext>,
  activeLayer: WorkTimelineLayer = 'narrative',
  boundWorldGraph?: WorldKbGraphResponse,
): WorkTimelineLayerAdapter {
  return {
    surfaceKind: 'work-timeline',
    defaultLayer: 'narrative',
    nodeTypes: {
      ...workTimelineNodeTypes,
      'timeline-brief-era': timelineNodeTypes['timeline-brief-era'],
    },
    edgeTypes: undefined,
    layoutOptions:
      activeLayer === 'moment'
        ? MOMENT_LAYOUT_OPTIONS
        : activeLayer === 'brief'
          ? BRIEF_LAYOUT_OPTIONS
          : NARRATIVE_LAYOUT_OPTIONS,

    projectGraph(graph) {
      if (activeLayer === 'brief') {
        return projectBriefLayer(
          boundWorldGraph ?? ctxRef.current.boundWorldGraph,
        );
      }
      if (activeLayer === 'moment') {
        return projectMomentLayer(graph, ctxRef.current.sceneBeatFixture);
      }
      return projectNarrativeLayer(graph);
    },

    projectGraphForLayer(graph, layer) {
      if (layer === 'brief') {
        return projectBriefLayer(
          boundWorldGraph ?? ctxRef.current.boundWorldGraph,
        );
      }
      if (layer === 'moment') {
        return projectMomentLayer(graph, ctxRef.current.sceneBeatFixture);
      }
      return projectNarrativeLayer(graph);
    },

    renderInspector(node) {
      // V1.123 P3 Task 4 — also forwards the cross-surface navigation slots
      // (`worldId` + `onViewOnWorldTimeline`) from the adapter context so the
      // Narrative event inspector can render the "View on World Timeline"
      // affordance when the orchestrator wires them. Honest scope cut: either
      // slot absent → the inspector hides the affordance (per plan §).
      //
      // Architect §6 (read-only in V1.123): every inspector renders
      // read-only details + the Edit-in-Outline hand-off; no write is
      // invoked from the Work Timeline surface.
      return renderWorkTimelineInspector(node, ctxRef.current.workId, {
        worldId: ctxRef.current.worldId,
        onViewOnWorldTimeline: ctxRef.current.onViewOnWorldTimeline,
      });
    },

    summarizeGraph(graph) {
      return summarizeWorkTimelineGraph(graph);
    },

    // The era data source is the captured `boundWorldGraph` (ctxRef
    // fallback), NOT the Work outline graph — `WorkTimelineGraph`
    // (`WorkOutline.timeline_events[]`) carries no era entities. The
    // `graph` argument is therefore unused (the `useCanvasSurface` slot
    // contract passes it; the bound World's graph is supplied separately
    // as the captured factory param, mirroring the Brief projection).
    //
    // Work Brief is a read-only projection (PD-2) with no era selection
    // hand-off — `WorkTimelineCanvasAdapterContext` has no `onSelectNode`
    // — so `onSelectEra` is omitted (`BriefTimeBands` renders
    // selection-free bands; the prop is optional).
    renderBriefTimeBands(_graph) {
      const worldGraph = boundWorldGraph ?? ctxRef.current.boundWorldGraph;
      const tree = buildEraTree(
        worldGraph?.entities ?? [],
        worldGraph?.relationships ?? [],
      );
      return <BriefTimeBands tree={tree} />;
    },
  };
}
