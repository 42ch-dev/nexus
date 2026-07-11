/**
 * Outline+Timeline canvas — interactive structure surface for a Work (V1.72 β;
 * V1.108 P0 spatial React Flow parity).
 *
 * Thin orchestrator + public re-export facade. V1.73 B5 (`R-V172P0-QC1-002`)
 * split the 825-line monolith into focused sibling modules ≤250 lines per the
 * V1.71 `strategy-canvas.tsx` pattern. V1.108 P0 mounts the shared
 * `CanvasShell` with the RF projection so the outline opens as a spatial graph.
 */
import { useEffect, useMemo, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { useChapters, useWork, flattenPages } from '@/api/queries';
import { queryKeys } from '@/lib/nexus/query-keys';
import {
  isOutlineConflictError,
  usePatchOutlineChapter,
  usePatchOutlineStructure,
  usePatchTimelineEvent,
  useWorkOutline,
} from '@/lib/canvas/use-outline-data';

import { CanvasHeader } from './outline-canvas/canvas-layout';
import { OutlineConflictDialog } from './outline-canvas/conflict-modal';
import { BeatInspector } from './outline-canvas/inspectors/beat-inspector';
import { ChapterInspector } from './outline-canvas/inspectors/chapter-inspector';
import { TimelinePanel } from './outline-canvas/inspectors/event-inspector';
import { SceneInspector } from './outline-canvas/inspectors/scene-inspector';
import { OutlineStructurePanel } from './outline-canvas/inspectors/structure-inspector';
import type { ConflictState, SceneBeatFixturePayload } from './outline-canvas/graph-projection';
import { chapterDisplayTitle } from './outline-canvas/graph-projection';
import { outlineGraphSummary } from './outline-canvas/rf-projection';
import { outlineNodeTypes } from './outline-canvas/outline-nodes';
import { OutlineAltView } from './outline-canvas/outline-alt-view';
import { useOutlineCanvasGraph } from './outline-canvas/use-outline-canvas-graph';
import type {
  ChapterSummary,
  OutlinePatchChapterRequest,
  OutlinePatchStructureRequest,
  TimelinePatchEventRequest,
} from '@42ch/nexus-contracts';

/**
 * Stable empty fixture payload for real Works (no scene/beat data today).
 * Module-level so the hook's projection memo deps stay referentially stable
 * across re-renders — no new object identity per render.
 */
const EMPTY_SCENE_BEAT_FIXTURE: SceneBeatFixturePayload = { scenes: [], beats: [] };

export interface OutlineCanvasProps {
  workId: string;
  /**
   * Optional chapter id to preselect on mount (V1.75 F-QC3-001). Read once from
   * the route's `?chapter=N` query param by {@link OutlinePage} and used to
   * seed {@link selectedChapterId}; later user clicks override it normally.
   */
  initialSelectedChapterId?: number | null;
  /**
   * Optional Scene/Beat fixture payload (V1.109 C2 T4 — FB-C2-000/004).
   *
   * The outline wire model carries no scene/beat data today (architect-locked
   * §5.2 Q1), so real Works omit this prop → the projection emits zero
   * scene/beat children (honest empty chrome). Design Studio / test fixtures
   * inject populated payloads so the full Volume/Chapter/Scene/Beat hierarchy
   * renders for visual acceptance and integration testing.
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

export function OutlineCanvas({
  workId,
  initialSelectedChapterId = null,
  sceneBeatFixture,
}: OutlineCanvasProps) {
  const work = useWork(workId);
  const chaptersQuery = useChapters(workId);
  const outline = useWorkOutline(workId);

  const patchStructure = usePatchOutlineStructure(workId);
  const patchChapter = usePatchOutlineChapter(workId);
  const patchTimeline = usePatchTimelineEvent(workId);

  const [conflict, setConflict] = useState<ConflictState | null>(null);
  const [showAlt, setShowAlt] = useState(false);
  const qc = useQueryClient();
  // Bumped after a successful refetch so the inspector's content editor resets
  // its local dirty state (e.g. following conflict resolution / reapply).
  const [contentVersion, setContentVersion] = useState(0);

  const chapters = useMemo(() => flattenPages(chaptersQuery.data), [chaptersQuery.data]);

  // I-QC1-002 — auto-fetch all chapter pages so the spatial graph projects the
  // complete outline structure. Without this, paginated chapter data (20/page)
  // leaves volume→chapter edges pointing at unloaded chapter nodes. The graph
  // is a structural overview, so completeness matters more than lazy loading.
  useEffect(() => {
    if (chaptersQuery.hasNextPage && !chaptersQuery.isFetchingNextPage) {
      void chaptersQuery.fetchNextPage();
    }
  }, [chaptersQuery.hasNextPage, chaptersQuery.isFetchingNextPage, chaptersQuery.fetchNextPage]);
  const chapterById = useMemo(() => {
    const map = new Map<number, ChapterSummary>();
    chapters.forEach((c) => map.set(c.chapter, c));
    return map;
  }, [chapters]);

  // V1.109 C2 T2/T4 — Scene/Beat fixture payload injection point. The outline
  // wire model has no scene/beat data today (architect-locked §5.2 Q1), so
  // real Works pass nothing here → projection emits zero scene/beat children
  // (honest empty chrome). Design Studio / test fixtures inject populated
  // payloads via the `sceneBeatFixture` prop when scene/beat demo data is
  // needed. The empty default is stable (module-level constant) so the hook's
  // projection memo deps don't churn on re-render.
  const fixture = sceneBeatFixture ?? EMPTY_SCENE_BEAT_FIXTURE;

  // V1.109 P0 T1 — RF graph state extracted into `useOutlineCanvasGraph`
  // (R-V1108P0QC1-S001). The hook owns the projection memo, rfNodes/rfEdges
  // state, the position-merge sync effect (preserves dragged positions +
  // selection across chapter-page loads), the graph-click → inspector
  // selection-sync effect (FB-C1-003), and `selectedChapterId`. The panel
  // writes through the hook's setter so graph + panel selection stay in sync
  // via one state owner.
  const {
    rfNodes,
    rfEdges,
    onNodesChange,
    selectedChapterId,
    setSelectedChapterId,
    selectedSceneId,
    selectedBeatId,
    projection,
  } = useOutlineCanvasGraph({
    outline: outline.data,
    chapters,
    initialSelectedChapterId,
    sceneBeatFixture: fixture,
  });

  const selectedChapter = selectedChapterId ? chapterById.get(selectedChapterId) ?? null : null;

  // V1.109 C2 T4 — resolve the selected Scene/Beat from the fixture payload +
  // hook selection state (FB-C2-002). The hook exposes `selectedSceneId` /
  // `selectedBeatId` (driven by RF graph-click); the orchestrator resolves
  // them against the fixture to get the entity data + parent title for the
  // inspector. On real Works (empty fixture) both are always null — the
  // Chapter inspector remains the default.
  const selectedScene = selectedSceneId
    ? fixture.scenes.find((s) => s.sceneId === selectedSceneId) ?? null
    : null;
  const selectedBeat = selectedBeatId
    ? fixture.beats.find((b) => b.beatId === selectedBeatId) ?? null
    : null;

  // Parent titles for the *Part of* helper (Voice & Content lock).
  const sceneParentChapterTitle = selectedScene
    ? (() => {
        const ch = chapterById.get(selectedScene.chapterId);
        return ch ? chapterDisplayTitle(ch, outline.data?.chapter_titles as Record<string, string> | undefined) : null;
      })()
    : null;
  const beatParentSceneTitle = selectedBeat
    ? fixture.scenes.find((s) => s.sceneId === selectedBeat.sceneId)?.title ?? null
    : null;

  const summary = outlineGraphSummary(outline.data, chapters.length);

  function captureConflictState(
    error: unknown,
    base: Omit<ConflictState, 'currentRevision' | 'conflictingPath'>,
  ) {
    if (!isOutlineConflictError(error)) return;
    const details = error.details as
      | { current_version?: number; conflicting_path?: string }
      | undefined;
    setConflict({
      ...base,
      currentRevision: details?.current_version ?? outline.data?.outline_revision ?? 0,
      conflictingPath: details?.conflicting_path ?? base.pendingRequest.kind,
    });
  }

  function handleStructure(request: OutlinePatchStructureRequest) {
    const state: Omit<ConflictState, 'currentRevision' | 'conflictingPath'> = {
      pendingRequest: { kind: 'structure', request },
    };
    patchStructure.mutate(request, {
      onError: (error) => captureConflictState(error, state),
    });
  }

  function handleChapter(chapter: number, request: OutlinePatchChapterRequest) {
    const state: Omit<ConflictState, 'currentRevision' | 'conflictingPath'> = {
      pendingRequest: { kind: 'chapter', chapter, request },
    };
    patchChapter.mutate(
      { chapter, request },
      {
        onError: (error) => captureConflictState(error, state),
      },
    );
  }

  function handleTimeline(request: TimelinePatchEventRequest) {
    const state: Omit<ConflictState, 'currentRevision' | 'conflictingPath'> = {
      pendingRequest: { kind: 'timeline', request },
    };
    patchTimeline.mutate(request, {
      onError: (error) => captureConflictState(error, state),
    });
  }

  async function onUseCurrent() {
    setConflict(null);
    await outline.refetch();
    // Also invalidate the per-chapter outline cache (useChapterOutline, read by
    // the content editor). The work-level outline.refetch() above does NOT touch
    // the chapter outline query; without this invalidation the forced content
    // reset below would reload stale chapter prose — silently showing outdated
    // content when another writer concurrently edited the same chapter. The
    // content editor's content-sync effect guards on outline.isFetching, so it
    // waits for this refetch before applying the forced reset.
    void qc.invalidateQueries({
      queryKey: [...queryKeys.chapters.outlines(), workId],
    });
    // Force the content editor to discard its draft and reload the canonical
    // content. contentVersion is no longer bumped on ordinary patches, so this
    // bump is a reliable forced-reset signal that overrides the editor's
    // dirty/saving guard.
    setContentVersion((v) => v + 1);
  }

  function onDismiss() {
    setConflict(null);
  }

  async function onReapply() {
    if (!conflict) return;
    setConflict(null);
    const fresh = await outline.refetch();
    const baseRevision = fresh.data?.outline_revision;
    if (baseRevision === undefined) return;
    const { pendingRequest } = conflict;
    if (pendingRequest.kind === 'structure') {
      handleStructure({ ...pendingRequest.request, base_revision: baseRevision });
    } else if (pendingRequest.kind === 'chapter') {
      handleChapter(pendingRequest.chapter, {
        ...pendingRequest.request,
        base_revision: baseRevision,
      });
    } else {
      handleTimeline({ ...pendingRequest.request, base_revision: baseRevision });
    }
  }

  if (outline.isError || chaptersQuery.isError || work.isError) {
    return (
      <ErrorState
        title="Could not load outline"
        description="The outline or chapter list failed to load. Try again when the daemon is reachable."
        onRetry={() => {
          void outline.refetch();
          void chaptersQuery.refetch();
          void work.refetch();
        }}
      />
    );
  }

  if (outline.isLoading || chaptersQuery.isLoading || work.isLoading) {
    return <LoadingState label="Loading outline…" />;
  }

  if (!outline.data) {
    return (
      <EmptyState
        title="No outline found"
        description="This Work does not have an outline yet. Create chapters to populate the canvas."
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <CanvasHeader
        title={work.data?.title ?? 'Untitled Work'}
        subtitle="Outline and timeline structure for this Work."
        revision={outline.data.outline_revision}
        status={patchStructure.isPending ? 'dirty' : 'clean'}
        showAlt={showAlt}
        setShowAlt={setShowAlt}
      />

      {/* V1.109 C2 T4 — pass the original prop (undefined on real Works) so
          the alt view distinguishes "no fixture" from "empty fixture". Real
          Works → no scene/beat rows, no empty-under-chapter helper. Fixtures
          (even empty) → helper shows for visual acceptance. */}
      {showAlt ? (
        <OutlineAltView outline={outline.data} chapters={chapters} sceneBeatFixture={sceneBeatFixture} />
      ) : (
        <CanvasShell
          nodes={rfNodes}
          edges={rfEdges}
          nodeTypes={outlineNodeTypes}
          onNodesChange={onNodesChange}
          summaryText={summary}
          ariaLabel="Outline structure graph"
          surfaceKey={`outline:${workId}`}
        >
          {/* I-QC1-001 — when the projection has zero nodes, render the
              EmptyState as an in-shell overlay so CanvasShell is always
              mounted for the graph view (FB-C1-000 shared-shell parity). */}
          {projection && projection.nodes.length === 0 ? (
            <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
              <EmptyState
                title="No graph nodes"
                description="This outline has no volumes, chapters, or timeline events to display on the graph yet."
              />
            </div>
          ) : null}
        </CanvasShell>
      )}

      <div className="grid gap-4 lg:grid-cols-[1fr_360px]">
        <OutlineStructurePanel
          outline={outline.data}
          chapters={chapters}
          selectedChapterId={selectedChapterId}
          onSelectChapter={setSelectedChapterId}
          onMoveChapter={(chapterId, volumeId) =>
            handleStructure({
              work_id: workId,
              base_revision: outline.data.outline_revision,
              operation: 'move_chapter',
              chapter_id: chapterId,
              volume_id: volumeId,
            })
          }
        />

        <div className="flex flex-col gap-4">
          {/* V1.109 C2 T4 — Scene/Beat inspector mounting (FB-C2-002). The
              hook's selection coordination ensures only one of Beat/Scene/
              Chapter is selected at a time. When a Beat is selected, show the
              Beat inspector; when a Scene is selected, show the Scene
              inspector; otherwise fall through to the Chapter inspector
              (default — includes its empty state when no chapter is selected).
              On real Works (empty fixture), selectedBeat/selectedScene are
              always null → Chapter inspector is always shown (no regression). */}
          {selectedBeat ? (
            <BeatInspector beat={selectedBeat} parentSceneTitle={beatParentSceneTitle} />
          ) : selectedScene ? (
            <SceneInspector scene={selectedScene} parentChapterTitle={sceneParentChapterTitle} />
          ) : (
            <ChapterInspector
              workId={workId}
              outline={outline.data}
              chapter={selectedChapter}
              baseRevision={outline.data.outline_revision}
              onPatchChapter={handleChapter}
              onMove={(chapterId, volumeId) =>
                handleStructure({
                  work_id: workId,
                  base_revision: outline.data.outline_revision,
                  operation: 'move_chapter',
                  chapter_id: chapterId,
                  volume_id: volumeId,
                })
              }
              patchIsPending={patchChapter.isPending}
              isConflicting={conflict !== null}
              contentVersion={contentVersion}
            />
          )}

          <TimelinePanel
            outline={outline.data}
            selectedChapterId={selectedChapterId}
            baseRevision={outline.data.outline_revision}
            onPatchTimeline={handleTimeline}
          />
        </div>
      </div>

      <OutlineConflictDialog
        conflict={conflict}
        onUseCurrent={onUseCurrent}
        onReapply={onReapply}
        onDismiss={onDismiss}
      />
    </div>
  );
}
