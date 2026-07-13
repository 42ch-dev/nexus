/**
 * Outline canvas adapter — projects the daemon outline + chapters into React
 * Flow nodes/edges and renders surface-specific chrome (inspectors, alt-view,
 * a11y summary).
 *
 * V1.115 P0 T1a: implements the shared {@link CanvasSurfaceAdapter} interface
 * so the orchestrator can delegate projection, node types, summary, and
 * alt-view to the surface (mirrors the V1.114 Strategy + World KB adapters).
 * T1b (separate dispatch) rewires `outline-canvas.tsx` to consume
 * `useCanvasSurface()`.
 *
 * The returned adapter object is stable; it reads mutable values from the
 * supplied context ref so the orchestrator can update state without
 * invalidating `useCanvasSurface`'s memoized graph projection.
 */
import type { MutableRefObject } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type {
  ChapterSummary,
  OutlinePatchChapterRequest,
  WorkOutline,
} from '@42ch/nexus-contracts';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type { ConflictModalProps } from '../conflict-modal';
import { outlineNodeTypes } from './outline-nodes';
import {
  outlineGraphSummary,
  projectOutlineGraph,
  type OutlineBeatNodeData,
  type OutlineChapterNodeData,
  type OutlineEdgeData,
  type OutlineSceneNodeData,
  type OutlineTimelineEventNodeData,
  type OutlineVolumeNodeData,
} from './rf-projection';
import { chapterDisplayTitle, type SceneBeatFixturePayload } from './graph-projection';
import { OutlineAltView } from './outline-alt-view';
import { BeatInspector } from './inspectors/beat-inspector';
import { ChapterInspector } from './inspectors/chapter-inspector';
import { SceneInspector } from './inspectors/scene-inspector';

// ---------------------------------------------------------------------------
// Graph payload + node data union
// ---------------------------------------------------------------------------

/** Graph payload consumed by the Outline surface adapter. */
export interface OutlineSurfaceGraph {
  outline: WorkOutline;
  chapters: ChapterSummary[];
  sceneBeatFixture: SceneBeatFixturePayload;
}

/**
 * Union of all Outline RF node data payloads. The adapter interface requires a
 * single `TNodeData`; `renderInspector` narrows by `node.type` before reading
 * kind-specific fields (each data shape carries `[key: string]: unknown`).
 */
export type OutlineNodeData =
  | OutlineVolumeNodeData
  | OutlineChapterNodeData
  | OutlineSceneNodeData
  | OutlineBeatNodeData
  | OutlineTimelineEventNodeData;

// ---------------------------------------------------------------------------
// Adapter context (mutable; supplied by the orchestrator via a ref)
// ---------------------------------------------------------------------------

/**
 * Mutable context supplied by the orchestrator so the adapter can project the
 * graph, render inspectors / alt-view, and produce an a11y summary without
 * closing over stale values.
 *
 * All fields are read at projection/render time from the ref; the adapter
 * object itself is stable and never recreated.
 */
export interface OutlineCanvasAdapterContext {
  // Projection-time i18n — localized "Chapter N" fallback for untitled chapters.
  translateFallback: (chapter: number) => string;

  // Summary-time i18n — drives `outlineGraphSummary` interpolation.
  t: (key: string, options?: Record<string, unknown>) => string;

  // Inspector routing + ChapterInspector write-boundary props.
  workId: string;
  outline: WorkOutline | undefined;
  chapters: ChapterSummary[];
  chapterById: Map<number, ChapterSummary>;
  /** Always-defined fixture used to resolve Beat/Scene entities from node data. */
  fixture: SceneBeatFixturePayload;
  /**
   * Original `sceneBeatFixture` prop (undefined on real Works). Drives the
   * alt-view's "no fixture vs. empty fixture" distinction — when undefined,
   * chapters render without the empty-under-chapter helper.
   */
  altViewSceneBeatFixture?: SceneBeatFixturePayload;

  // ChapterInspector write-boundary handlers/state.
  onPatchChapter: (chapter: number, request: OutlinePatchChapterRequest) => void;
  onMove: (chapterId: number, volumeId: number) => void;
  patchChapterIsPending: boolean;
  isConflicting: boolean;
  contentVersion: number;
}

export type OutlineCanvasAdapter = CanvasSurfaceAdapter<
  OutlineSurfaceGraph,
  OutlineNodeData,
  OutlineEdgeData
>;

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/**
 * Create a stable Outline canvas adapter.
 *
 * The adapter reads mutable values from the supplied context ref so the
 * orchestrator can update state without invalidating the hook's memoized graph
 * projection. Projection delegates to {@link projectOutlineGraph} — its
 * signature is unchanged; the function simply becomes adapter-owned.
 */
export function createOutlineCanvasAdapter(
  ctxRef: MutableRefObject<OutlineCanvasAdapterContext>,
): OutlineCanvasAdapter {
  return {
    surfaceKind: 'outline',
    nodeTypes: outlineNodeTypes,
    edgeTypes: undefined,
    // simplify: dagre layout omitted because @dagrejs/dagre@3.0.0 crashes on
    // compound graphs where an edge targets a node that has children via
    // setParent (the Outline projection creates volume→chapter edges where
    // chapter nodes have scene/beat children — exactly this pattern). The
    // projection's built-in deterministic grid layout (projectOutlineGraph)
    // supplies positions directly; useAutoLayout returns pass-through when
    // layoutOptions is undefined. Upgrade path: fix the dagre compound-graph
    // ranker issue, or switch to a layout engine that handles nesting (e.g.
    // elk-js), then restore `layoutOptions: { direction: 'LR' }`.
    layoutOptions: undefined,

    projectGraph(graph) {
      const { translateFallback } = ctxRef.current;
      const projected = projectOutlineGraph(
        graph.outline,
        graph.chapters,
        graph.sceneBeatFixture,
        translateFallback,
      );
      return {
        nodes: projected.nodes as Node<OutlineNodeData>[],
        edges: projected.edges as Edge<OutlineEdgeData>[],
      };
    },

    adaptConflict(_error) {
      // Outline write-boundary conflicts (409 on patch) are surface-owned:
      // the orchestrator captures them via `captureConflictState` and renders
      // `OutlineConflictDialog` — a surface-specific modal whose props
      // (`ConflictState`) do not match the shared `ConflictModalProps` shape
      // (Strategy-specific: `ConflictModalDraft` / `canonicalState`). Returning
      // null mirrors the World KB precedent; the conflict modal stays
      // orchestrator-rendered (T1b concern).
      return null as ConflictModalProps | null;
    },

    renderInspector(node) {
      return <OutlineInspectorRouter node={node} ctxRef={ctxRef} />;
    },

    renderAltView() {
      return <OutlineAltViewWrapper ctxRef={ctxRef} />;
    },

    summarizeGraph(graph) {
      const { t } = ctxRef.current;
      return outlineGraphSummary(graph.outline, graph.chapters.length, t);
    },
  };
}

// ---------------------------------------------------------------------------
// Inspector routing — chapter / scene / beat
// ---------------------------------------------------------------------------

/**
 * Adapter-driven inspector router. Reads the selected node's type and resolves
 * the matching entity (chapter / scene / beat) from the context ref, then
 * renders the appropriate inspector.
 *
 * Mirrors the routing that previously lived inline in `outline-canvas.tsx`:
 *   • outline-beat    → BeatInspector (read-only; resolves beat from fixture)
 *   • outline-scene   → SceneInspector (read-only; resolves scene from fixture)
 *   • outline-chapter → ChapterInspector (resolves chapter from chapterById)
 *   • outline-timeline-event → ChapterInspector (resolves via realizesChapterId)
 *   • outline-volume / default → ChapterInspector with chapter=null (empty state)
 */
function OutlineInspectorRouter({
  node,
  ctxRef,
}: {
  node: Node<OutlineNodeData>;
  ctxRef: MutableRefObject<OutlineCanvasAdapterContext>;
}) {
  const ctx = ctxRef.current;

  // Beat node → BeatInspector (read-only).
  if (node.type === 'outline-beat') {
    const data = node.data as OutlineBeatNodeData;
    const beat = ctx.fixture.beats.find((b) => b.beatId === data.beatId) ?? null;
    const parentSceneTitle = beat
      ? (ctx.fixture.scenes.find((s) => s.sceneId === beat.sceneId)?.title ?? null)
      : null;
    return <BeatInspector beat={beat} parentSceneTitle={parentSceneTitle} />;
  }

  // Scene node → SceneInspector (read-only).
  if (node.type === 'outline-scene') {
    const data = node.data as OutlineSceneNodeData;
    const scene = ctx.fixture.scenes.find((s) => s.sceneId === data.sceneId) ?? null;
    const parentChapterTitle = scene
      ? resolveChapterDisplayTitle(ctx, scene.chapterId)
      : null;
    return <SceneInspector scene={scene} parentChapterTitle={parentChapterTitle} />;
  }

  // Chapter / timeline-event / volume / default → ChapterInspector.
  if (!ctx.outline) return null;

  let chapter: ChapterSummary | null = null;
  if (node.type === 'outline-chapter') {
    const data = node.data as OutlineChapterNodeData;
    chapter = ctx.chapterById.get(data.chapterId) ?? null;
  } else if (node.type === 'outline-timeline-event') {
    const data = node.data as OutlineTimelineEventNodeData;
    chapter =
      data.realizesChapterId !== null
        ? (ctx.chapterById.get(data.realizesChapterId) ?? null)
        : null;
  }

  return (
    <ChapterInspector
      workId={ctx.workId}
      outline={ctx.outline}
      chapter={chapter}
      baseRevision={ctx.outline.outline_revision}
      onPatchChapter={ctx.onPatchChapter}
      onMove={ctx.onMove}
      patchIsPending={ctx.patchChapterIsPending}
      isConflicting={ctx.isConflicting}
      contentVersion={ctx.contentVersion}
    />
  );
}

/** Resolve the human-facing display title for a chapter (parent-scene helper). */
function resolveChapterDisplayTitle(
  ctx: OutlineCanvasAdapterContext,
  chapterId: number,
): string | null {
  const chapter = ctx.chapterById.get(chapterId);
  if (!chapter) return null;
  return chapterDisplayTitle(
    chapter,
    ctx.outline?.chapter_titles as Record<string, string> | undefined,
    ctx.translateFallback(chapterId),
  );
}

// ---------------------------------------------------------------------------
// Alt-view wrapper — reads fresh outline/chapters from the context ref
// ---------------------------------------------------------------------------

function OutlineAltViewWrapper({
  ctxRef,
}: {
  ctxRef: MutableRefObject<OutlineCanvasAdapterContext>;
}) {
  const ctx = ctxRef.current;
  if (!ctx.outline) return null;
  return (
    <OutlineAltView
      outline={ctx.outline}
      chapters={ctx.chapters}
      sceneBeatFixture={ctx.altViewSceneBeatFixture}
    />
  );
}
