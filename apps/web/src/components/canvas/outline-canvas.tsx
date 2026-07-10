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
import type { Edge, Node } from '@xyflow/react';

import { CanvasShell, useNodeChangeHandler } from '@/components/canvas/canvas-shell';
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
import { ChapterInspector } from './outline-canvas/inspectors/chapter-inspector';
import { TimelinePanel } from './outline-canvas/inspectors/event-inspector';
import { OutlineStructurePanel } from './outline-canvas/inspectors/structure-inspector';
import type { ConflictState } from './outline-canvas/graph-projection';
import {
  outlineGraphSummary,
  projectOutlineGraph,
  selectedChapterIdFromNodes,
} from './outline-canvas/rf-projection';
import { outlineNodeTypes } from './outline-canvas/outline-nodes';
import { OutlineAltView } from './outline-canvas/outline-alt-view';
import type {
  ChapterSummary,
  OutlinePatchChapterRequest,
  OutlinePatchStructureRequest,
  TimelinePatchEventRequest,
} from '@42ch/nexus-contracts';

export interface OutlineCanvasProps {
  workId: string;
  /**
   * Optional chapter id to preselect on mount (V1.75 F-QC3-001). Read once from
   * the route's `?chapter=N` query param by {@link OutlinePage} and used to
   * seed {@link selectedChapterId}; later user clicks override it normally.
   */
  initialSelectedChapterId?: number | null;
}

export function OutlineCanvas({ workId, initialSelectedChapterId = null }: OutlineCanvasProps) {
  const work = useWork(workId);
  const chaptersQuery = useChapters(workId);
  const outline = useWorkOutline(workId);

  const patchStructure = usePatchOutlineStructure(workId);
  const patchChapter = usePatchOutlineChapter(workId);
  const patchTimeline = usePatchTimelineEvent(workId);

  const [selectedChapterId, setSelectedChapterId] = useState<number | null>(
    initialSelectedChapterId ?? null,
  );
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

  const selectedChapter = selectedChapterId ? chapterById.get(selectedChapterId) ?? null : null;

  // V1.108 P0 — project the outline into a spatial React Flow graph.
  // The graph is the primary view (FB-C1-000); the panel below remains as a
  // structural inspector companion. T2 wires graph-click → inspector selection.
  const projection = useMemo(
    () => (outline.data ? projectOutlineGraph(outline.data, chapters) : null),
    [outline.data, chapters],
  );
  const [rfNodes, setRfNodes] = useState<Node[]>([]);
  const [rfEdges, setRfEdges] = useState<Edge[]>([]);
  const onNodesChange = useNodeChangeHandler(setRfNodes);

  // Sync RF state when the projection changes (data refetch, chapter list update).
  // PR-review fix: merge instead of replace so the author's graph interactions
  // (dragged positions, selection) survive incremental chapter-page loads.
  // `chapters` grows as each cursor page arrives (I-QC1-002 auto-fetch), which
  // rebuilds the projection; a bare `setRfNodes(projection.nodes)` wiped every
  // node's user-moved position and selection on each page fetch. For nodes that
  // persist across the rebuild (same id), preserve their `position` and
  // `selected` flag; new nodes use the projected position, dropped nodes fall
  // away. Edges carry no per-interaction state, so they are replaced directly.
  useEffect(() => {
    if (!projection) return;
    setRfEdges(projection.edges);
    setRfNodes((prev) => {
      if (prev.length === 0) return projection.nodes;
      const prevById = new Map(prev.map((n) => [n.id, n]));
      return projection.nodes.map((node) => {
        const existing = prevById.get(node.id);
        if (!existing) return node;
        return { ...node, position: existing.position, selected: existing.selected };
      });
    });
  }, [projection]);

  // Graph click → inspector selection sync (FB-C1-003).
  // React Flow tracks selection via the node `selected` flag (set through
  // onNodesChange). Resolve it to `selectedChapterId` so graph clicks drive the
  // chapter inspector — same pattern as `world-kb-canvas.tsx`.
  //
  // PR-review fix: `selectedChapterIdFromNodes` returns `null` both when no
  // node is selected AND when the selected node does not resolve to a chapter
  // (volume node, or a timeline event with no `realizes_chapter_id`). The old
  // guard left the previous chapter selection in place in the second case,
  // leaving a stale chapter in the inspector. Distinguish the two: when a node
  // IS selected but resolves to no chapter, clear the chapter selection so the
  // inspector does not show a chapter that is no longer the active graph
  // selection. When nothing is selected at all, leave the current selection
  // intact (preserves V1.75 `?chapter=N` preselect and click-to-keep-while-
  // panning behavior).
  useEffect(() => {
    const chapterId = selectedChapterIdFromNodes(rfNodes);
    if (chapterId !== null) {
      setSelectedChapterId(chapterId);
    } else if (rfNodes.some((n) => n.selected)) {
      setSelectedChapterId(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rfNodes]);

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

      {showAlt ? (
        <OutlineAltView outline={outline.data} chapters={chapters} />
      ) : (
        <CanvasShell
          nodes={rfNodes}
          edges={rfEdges}
          nodeTypes={outlineNodeTypes}
          onNodesChange={onNodesChange}
          summaryText={summary}
          ariaLabel="Outline structure graph"
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
