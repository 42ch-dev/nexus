/**
 * Outline+Timeline canvas — interactive structure surface for a Work (V1.72 β).
 *
 * Thin orchestrator + public re-export facade. V1.73 B5 (`R-V172P0-QC1-002`)
 * split the 825-line monolith into focused sibling modules ≤250 lines per the
 * V1.71 `strategy-canvas.tsx` pattern. Behavior and the public `OutlineCanvas`
 * export are unchanged.
 */
import { useMemo, useState } from 'react';

import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { useChapters, useWork, flattenPages } from '@/api/queries';
import {
  isOutlineConflictError,
  usePatchOutlineChapter,
  usePatchOutlineStructure,
  usePatchTimelineEvent,
  useWorkOutline,
} from '@/lib/canvas/use-outline-data';

import { RevisionBadge } from './outline-canvas/canvas-layout';
import { OutlineConflictDialog } from './outline-canvas/conflict-modal';
import { ChapterInspector } from './outline-canvas/inspectors/chapter-inspector';
import { TimelinePanel } from './outline-canvas/inspectors/event-inspector';
import { OutlineStructurePanel } from './outline-canvas/inspectors/structure-inspector';
import type { ConflictState } from './outline-canvas/graph-projection';
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
  // Bumped after a successful refetch so the inspector's content editor resets
  // its local dirty state (e.g. following conflict resolution / reapply).
  const [contentVersion, setContentVersion] = useState(0);

  const chapters = useMemo(() => flattenPages(chaptersQuery.data), [chaptersQuery.data]);
  const chapterById = useMemo(() => {
    const map = new Map<number, ChapterSummary>();
    chapters.forEach((c) => map.set(c.chapter, c));
    return map;
  }, [chapters]);

  const selectedChapter = selectedChapterId ? chapterById.get(selectedChapterId) ?? null : null;

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
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h1 className="text-heading-24 font-heading text-gray-1000">
            {work.data?.title ?? 'Untitled Work'}
          </h1>
          <p className="text-copy-14 text-gray-900">
            Outline and timeline structure for this Work.
          </p>
        </div>
        <RevisionBadge
          revision={outline.data.outline_revision}
          status={patchStructure.isPending ? 'dirty' : 'clean'}
        />
      </div>

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
