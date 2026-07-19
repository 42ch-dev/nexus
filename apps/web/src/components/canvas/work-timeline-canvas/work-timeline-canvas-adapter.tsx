/**
 * Work Timeline canvas adapter — V1.123 P2 Task 2 (Narrative projection) +
 * Task 3 (Moment projection) + Task 4 (layer state).
 *
 * Projects a Work's V1.72 `WorkOutline` onto the Work Timeline surface
 * (`CanvasSurfaceKind = "work-timeline"` — Task 5 promotes the string
 * literal to a real enum value; until then this adapter casts so the
 * contract stays type-compatible).
 *
 * Architect-locked contract — see
 * `iterations/v1.123/specs/three-layer-architecture.md` §3 + §6 + §7 + §8 +
 * `iterations/v1.123/specs/layer-feel-differentiation.md` §2.3 + §2.4:
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
 *   - **Moment layer** (Task 3): outline Scene/Beat data via V1.108
 *     `OutlineSceneNodeData` / `OutlineBeatNodeData` (fixture-driven
 *     today; V1.124+ `DF-V1123-MOMENT-WIRE` upgrades the wire). Vertical
 *     scene-stack (TB layout direction per `layer-feel-differentiation.md`
 *     §2.4). Honest empty-state when no fixture / no scene-beat data.
 *
 * Layer model: `projectGraphForLayer(graph, 'narrative' | 'moment')`
 * selects the active layer. The default `projectGraph(graph)` delegates to
 * the adapter's active layer, which defaults to `'narrative'` per
 * architect §7.3 UX-risk override (the V1.72 `WorkOutline` wire has no
 * Scene/Beat data today; Moment-default would surface persistent
 * empty-state in nearly all real Works).
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
 * `wire_contracts_changed: false for P2` — Moment-on-Outline is frontend-
 * only; P2 adds zero `schemas/` / `crates/` / `packages/nexus-contracts`
 * diff. (The iteration-level `wire_contracts_changed: true` is
 * attributable entirely to P1's `BlockType = "era"`.)
 */
import type { MutableRefObject } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type { WorkOutline } from '@42ch/nexus-contracts';

import type {
  BeatFixture,
  OutlineSceneStatus,
  SceneBeatFixturePayload,
  SceneFixture,
} from '../outline-canvas/graph-projection';
import type { DirectedAxisSpineNodeData, MomentSpineConfig, NarrativeSpineConfig } from '../timeline-canvas/directed-axis-spine';
import { workTimelineNodeTypes } from './work-timeline-node-types';
import { renderWorkTimelineInspector } from './work-timeline-inspector';

// ─── Public types (architect-locked §7.1) ──────────────────────────────────

/** Single graph source — no wrapper, no join. Mirrors V1.122 §3.1. */
export type WorkTimelineGraph = WorkOutline;

/**
 * Layer kind on the Work Timeline surface (architect §7.1).
 *
 * - `'narrative'` — Work-scoped events from `outline.timeline_events[]`.
 * - `'moment'`    — Scene/Beat precision from outline Scene/Beat data
 *                   (fixture-driven today; V1.124+ wire upgrade tracked
 *                   as `DF-V1123-MOMENT-WIRE`).
 */
export type WorkTimelineLayer = 'narrative' | 'moment';

/**
 * Node data payload for the Work Timeline surface.
 *
 * Discriminated by `nodeKind`:
 *   - `'event'`  for Narrative layer (Task 2).
 *   - `'scene'`  for Moment layer scene cards (Task 3).
 *   - `'beat'`   for Moment layer beat pins (Task 3).
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
   * Manuscript anchor badge data (Moment layer — chapter/scene link).
   * Present for scene + beat nodes when anchor data exists.
   */
  manuscriptAnchor?: {
    chapterId: number;
    sceneId?: string;
    beatId?: string;
  };
  /** Event id (Narrative layer only). */
  eventId?: string;
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

// ─── Adapter context ───────────────────────────────────────────────────────

/**
 * Mutable context supplied by the orchestrator so the adapter can render
 * inspectors / wire "Edit in Outline" navigation without closing over stale
 * values. Read at render time from the ref; the adapter object itself stays
 * stable across renders (V1.114 §3.3.1 "stable factory that reads from a
 * mutable `React.RefObject` context").
 *
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
   * Optional `NexusClient` reference. The projection is a pure function of
   * the graph; the client slot exists so write-boundary isolation tests can
   * assert negative invocation against every forbidden method on a single
   * mocked client.
   */
  client?: unknown;
  /**
   * "Edit in Outline" hand-off — fired when the user clicks the affordance
   * on a Moment scene/beat or a Narrative event. The orchestrator navigates
   * to `/works/:workId` (Outline surface) with state hinting which node to
   * focus. The adapter performs NO writes (architect-locked §6 — Moment
   * read-only in V1.123; Narrative writes route through Outline).
   */
  onEditInOutline?: (node: Node<WorkTimelineNodeData>) => void;
  /**
   * P3 cross-surface navigation hand-off — fired when the user clicks "View
   * on World Timeline" on a Narrative event with a bound World event. The
   * orchestrator navigates to
   * `/worlds/:worldId/timeline?layer=narrative&event=:eventId`. Undefined
   * when no World is bound or when P3 is not yet shipped.
   */
  onViewOnWorldTimeline?: (node: Node<WorkTimelineNodeData>) => void;
  /**
   * V1.108 Scene/Beat fixture (Moment-on-Outline carrier). The V1.72
   * `WorkOutline` wire has no scene/beat data today (architect §3.4); the
   * orchestrator injects Design Studio / test fixtures at the projection
   * call site. When undefined or empty, the Moment layer emits honest
   * empty-state (zero nodes) per architect §3.2.
   *
   * When the WorkOutline wire extends to expose scenes/beats (V1.124+
   * `DF-V1123-MOMENT-WIRE`), this slot will be populated from the wire
   * itself; the adapter contract stays unchanged.
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

// ─── Adapter interface (architect §7.1 — extends V1.114 CanvasSurfaceAdapter) ─

export type WorkTimelineCanvasAdapter = CanvasSurfaceAdapter<
  WorkTimelineGraph,
  WorkTimelineNodeData,
  WorkTimelineEdgeData
>;

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

// ─── Projection constants ──────────────────────────────────────────────────

/**
 * Initial-position metrics for the Work Timeline Narrative + Moment layers.
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
 * Per-layer dagre layout options (architect §7.2).
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

function narrativeEventNodeId(eventId: string): string {
  return `${NARRATIVE_NODE_ID_PREFIX}${eventId}`;
}
function momentSceneNodeId(sceneId: string): string {
  return `${MOMENT_SCENE_NODE_ID_PREFIX}${sceneId}`;
}
function momentBeatNodeId(beatId: string): string {
  return `${MOMENT_BEAT_NODE_ID_PREFIX}${beatId}`;
}

// ─── Narrative projection (architect §7 + §8) ──────────────────────────────

/**
 * Project `WorkOutline.timeline_events[]` onto the Work Timeline Narrative
 * when-axis (LR direction). Events are sorted by `realizes_chapter_id`
 * (numeric ascending; `undefined` tailing) then by `event_id`
 * (lexicographic). Events without a chapter anchor tail right on the same
 * axis — the Work Timeline has no Context-cluster lane (it is Work-scoped,
 * not World-scoped).
 *
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

  // Sort: realizes_chapter_id ascending (undefined tails); event_id
  // lexicographic tiebreaker for stability across refetches.
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

  // V1.126 P1 — Work Timeline Narrative directed axis spine (decoration-only,
  // Y=0, appended after entity nodes). Uses event_id as tick markers for
  // cross-surface consistency with World Timeline Narrative.
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
      position: { x: 0, y: NARRATIVE_AXIS_Y - 8 },
      data: narrativeSpineData as unknown as WorkTimelineNodeData,
      selectable: false,
      focusable: false,
    });
  }

  const edges = deriveForeshadowEdges(graph);

  return { nodes, edges };
}

/**
 * Derive `foreshadows` edges from `outline.foreshadows[]` (V1.72). Dangling
 * edges (source or target event absent from `timeline_events`) are dropped
 * — mirrors the V1.108 outline projection's dangling-edge guard.
 *
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
      // Foreshadow edges reuse the V1.108 outline token; selectable +
      // focusable for a11y. Visual styling lands in P4 layer-feel polish.
      selectable: false,
      focusable: true,
      style: { stroke: 'var(--color-canvas-outline-foreshadow-edge)' },
    });
  }
  return edges;
}

// ─── Moment projection (architect §3 + §7 + §8 + layer-feel §2.4) ──────────

/**
 * Project outline Scene/Beat data onto the Work Timeline Moment layer
 * (vertical scene-stack, TB direction).
 *
 * Carrier: Moment-on-Outline (frontend-only projection of V1.108
 * `OutlineSceneNodeData` / `OutlineBeatNodeData` fixture data). The V1.72
 * `WorkOutline` wire has no scene/beat data today (architect §3.4); the
 * orchestrator injects Design Studio / test fixtures via
 * `ctxRef.current.sceneBeatFixture`. When the fixture is absent or empty,
 * the projection emits zero nodes (honest empty-state per architect §3.2 +
 * product spec §4.5 — Task 7 owns the visible copy).
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
  if (!fixture || (fixture.scenes.length === 0 && fixture.beats.length === 0)) {
    // Honest empty-state — Task 7 owns the visible copy. The adapter's
    // contract is to emit zero nodes; the orchestrator renders the
    // empty-state panel when the active layer has no projectable data.
    return { nodes: [], edges: [] };
  }

  // Group scenes by chapter so the vertical stack reads chapter → scene →
  // beat top-to-bottom. Chapter ordering is numeric ascending.
  const scenesByChapter = new Map<number, SceneFixture[]>();
  for (const scene of fixture.scenes) {
    const bucket = scenesByChapter.get(scene.chapterId);
    if (bucket) bucket.push(scene);
    else scenesByChapter.set(scene.chapterId, [scene]);
  }
  const sortedChapterIds = [...scenesByChapter.keys()].sort((a, b) => a - b);

  const emittedSceneIds = new Set<string>();
  const nodes: Node<WorkTimelineNodeData>[] = [];

  // Scene cards — vertical stack per chapter region (X groups by chapter).
  sortedChapterIds.forEach((chapterId, chapterIdx) => {
    const scenes = scenesByChapter.get(chapterId) ?? [];
    // Stable sort by sceneId within the chapter so the stack is
    // deterministic across refetches.
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

  // Beat pins — children of Scene cards conceptually; positioned in a
  // column next to / below their scene. Beats whose sceneId is absent
  // from the emitted scenes are dropped (mirrors V1.108 rf-projection
  // orphan guard).
  const beatsByScene = new Map<string, BeatFixture[]>();
  for (const beat of fixture.beats) {
    if (!emittedSceneIds.has(beat.sceneId)) continue;
    const bucket = beatsByScene.get(beat.sceneId);
    if (bucket) bucket.push(beat);
    else beatsByScene.set(beat.sceneId, [beat]);
  }

  // Index scenes by id → position so beats can stack relative to their
  // scene's position.
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

  // V1.126 P1 — Moment directed axis spine (decoration-only, Y=0).
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
      position: { x: 0, y: MOMENT_ORIGIN_Y - 12 },
      data: momentSpineData as unknown as WorkTimelineNodeData,
      selectable: false,
      focusable: false,
    });
  }

  // Moment layer: no explicit edges in V1.123 MVP. Beat succession within
  // scene is encoded spatially by the vertical stack (layer-feel §2.4).
  // `realizes_event` light links (beat → Narrative event) are P4 polish.
  return { nodes, edges: [] };
}

// ─── Layer-aware projection (architect §7.1) ───────────────────────────────

/**
 * Project the Work Timeline graph for a specific layer.
 *
 * Narrative layer reads from `WorkOutline.timeline_events[]` (V1.72 wire).
 * Moment layer reads from the V1.108 `SceneBeatFixturePayload` carried in
 * the adapter context (`ctxRef.current.sceneBeatFixture`); when absent,
 * the Moment layer emits honest empty-state (zero nodes).
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
): {
  nodes: Node<WorkTimelineNodeData>[];
  edges: Edge<WorkTimelineEdgeData>[];
} {
  if (layer === 'moment') {
    return projectMomentLayer(graph, fixture);
  }
  return projectNarrativeLayer(graph);
}

// ─── Honest summary (architect §7) ─────────────────────────────────────────

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

// ─── Stable factory (architect §7.1 + V1.114 §3.3.1) ───────────────────────

/**
 * Build a stable Work Timeline canvas adapter that reads mutable values
 * from the supplied context ref (V1.114 §3.3.1 "stable factory that reads
 * from a mutable `React.RefObject` context").
 *
 * The returned object MUST stay referentially stable across renders —
 * `useCanvasSurface` memoises on `adapter` and would otherwise re-project
 * on every orchestrator state change. The factory is therefore called
 * once per orchestrator mount (e.g. via `useMemo([activeLayer], ...)`);
 * only `activeLayer` invalidates the memo (Task 4 wires the layer swap).
 *
 * `activeLayer` selects which projection `projectGraph(graph)` delegates
 * to. Default `'narrative'` per architect §7.3 UX-risk override.
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
): WorkTimelineLayerAdapter {
  return {
    // V1.123 P2 Task 2 — the additive `'work-timeline'` value is part of
    // the `CanvasSurfaceKind` enum (extended in the same Task 2 commit so
    // the adapter typechecks). Task 5 owns the full peer-surface
    // integration (route + canvas-nav resolver + sidebar entry); the enum
    // value alone is the minimum additive addition for adapter type-safety.
    surfaceKind: 'work-timeline',
    defaultLayer: 'narrative',
    nodeTypes: workTimelineNodeTypes,
    edgeTypes: undefined,
    layoutOptions:
      activeLayer === 'moment' ? MOMENT_LAYOUT_OPTIONS : NARRATIVE_LAYOUT_OPTIONS,

    projectGraph(graph) {
      // Delegate to the active layer. The Moment projection reads the
      // V1.108 Scene/Beat fixture from `ctxRef.current.sceneBeatFixture`
      // (Moment-on-Outline carrier — frontend-only projection).
      if (activeLayer === 'moment') {
        return projectMomentLayer(graph, ctxRef.current.sceneBeatFixture);
      }
      return projectNarrativeLayer(graph);
    },

    projectGraphForLayer(graph, layer) {
      // Public layer-aware projection. Reads the fixture from the context
      // for the Moment layer (carrier = V1.108 Scene/Beat fixture).
      if (layer === 'moment') {
        return projectMomentLayer(graph, ctxRef.current.sceneBeatFixture);
      }
      return projectNarrativeLayer(graph);
    },

    adaptConflict(_error) {
      // Orchestrator-owned — the orchestrator renders the Outline-flavored
      // conflict modal (`OutlineConflictError` 409 + `OutlineValidationError`
      // 422, V1.72 reused verbatim — no Work-Timeline-specific conflict DTO)
      // from the structured `WorkTimelineConflictInfo` parsed elsewhere.
      // Returning null mirrors the V1.122 Timeline adapter.
      return null;
    },

    renderInspector(node) {
      // Task 6 — dispatch the selected node to the right Work Timeline
      // inspector by `node.data.nodeKind`. Reads the current `workId` from
      // the adapter context so the "Edit in Outline" CTA can navigate to
      // `/works/:workId/outline` without closing over a stale id.
      //
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
  };
}
