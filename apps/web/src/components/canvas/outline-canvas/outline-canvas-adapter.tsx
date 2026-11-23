/**
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
import type { TFunction } from 'i18next';

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

export interface OutlineSurfaceGraph {
  outline: WorkOutline;
  chapters: ChapterSummary[];
  sceneBeatFixture: SceneBeatFixturePayload;
}

export type OutlineNodeData =
  | OutlineVolumeNodeData
  | OutlineChapterNodeData
  | OutlineSceneNodeData
  | OutlineBeatNodeData
  | OutlineTimelineEventNodeData;

/**
 * All fields are read at projection/render time from the ref; the adapter
 * object itself is stable and never recreated.
 */
export interface OutlineCanvasAdapterContext {
  translateFallback: (chapter: number) => string;

  t: TFunction<'canvas'>;

  workId: string;
  outline: WorkOutline | undefined;
  chapters: ChapterSummary[];
  chapterById: Map<number, ChapterSummary>;
  /** Always-defined fixture used to resolve Beat/Scene entities from node data. */
  fixture: SceneBeatFixturePayload;
  altViewSceneBeatFixture?: SceneBeatFixturePayload;

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

/**
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
    // layoutOptions is undefined. upgrade: fix the dagre compound-graph
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

/**
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

  if (node.type === 'outline-beat') {
    const data = node.data as OutlineBeatNodeData;
    const beat = ctx.fixture.beats.find((b) => b.beatId === data.beatId) ?? null;
    const parentSceneTitle = beat
      ? (ctx.fixture.scenes.find((s) => s.sceneId === beat.sceneId)?.title ?? null)
      : null;
    return <BeatInspector beat={beat} parentSceneTitle={parentSceneTitle} />;
  }

  if (node.type === 'outline-scene') {
    const data = node.data as OutlineSceneNodeData;
    const scene = ctx.fixture.scenes.find((s) => s.sceneId === data.sceneId) ?? null;
    const parentChapterTitle = scene
      ? resolveChapterDisplayTitle(ctx, scene.chapterId)
      : null;
    return <SceneInspector scene={scene} parentChapterTitle={parentChapterTitle} />;
  }

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
