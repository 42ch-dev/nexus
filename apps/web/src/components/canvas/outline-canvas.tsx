/**
 * Outline+Timeline canvas — interactive structure surface for a Work (V1.72 β).
 *
 * Displays volumes/chapters on the left, a chapter inspector and timeline on
 * the right, and wires the three V1.72 patch routes through TanStack Query.
 * Conflicts surfaced by the daemon (HTTP 409) are resolved with the shared
 * conflict modal shell.
 */
import { useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  ArrowRight,
  BookOpen,
  CalendarPlus,
  ChevronLeft,
  ChevronRight,
  Link2,
  Save,
  Trash2,
} from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import {
  OutlineConflictModal,
  type OutlineChangedField,
} from '@/components/canvas/outline-conflict-modal';
import { useWork, useChapters, flattenPages } from '@/api/queries';
import {
  isOutlineConflictError,
  usePatchOutlineChapter,
  usePatchOutlineStructure,
  usePatchTimelineEvent,
  useWorkOutline,
} from '@/lib/canvas/use-outline-data';
import type {
  ChapterStatus,
  ChapterSummary,
  OutlinePatchChapterRequest,
  OutlinePatchStructureRequest,
  TimelinePatchEventRequest,
  WorkOutline,
} from '@42ch/nexus-contracts';

const STATUS_OPTIONS: { value: ChapterStatus; label: string }[] = [
  { value: 'not_started', label: 'Not started' },
  { value: 'outlined', label: 'Outlined' },
  { value: 'draft', label: 'Draft' },
  { value: 'finalized', label: 'Finalized' },
  { value: 'published', label: 'Published' },
];

const STATUS_VARIANT: Record<ChapterStatus, 'neutral' | 'queued' | 'warning' | 'running' | 'preset'> = {
  not_started: 'neutral',
  outlined: 'queued',
  draft: 'warning',
  finalized: 'running',
  published: 'preset',
};

interface OutlineCanvasProps {
  workId: string;
}

type PendingPatch =
  | { kind: 'structure'; request: OutlinePatchStructureRequest }
  | { kind: 'chapter'; chapter: number; request: OutlinePatchChapterRequest }
  | { kind: 'timeline'; request: TimelinePatchEventRequest };

interface ConflictState {
  currentRevision: number;
  conflictingPath: string;
  pendingRequest: PendingPatch;
}

export function OutlineCanvas({ workId }: OutlineCanvasProps) {
  const work = useWork(workId);
  const chaptersQuery = useChapters(workId);
  const outline = useWorkOutline(workId);

  const patchStructure = usePatchOutlineStructure(workId);
  const patchChapter = usePatchOutlineChapter(workId);
  const patchTimeline = usePatchTimelineEvent(workId);

  const [selectedChapterId, setSelectedChapterId] = useState<number | null>(null);
  const [conflict, setConflict] = useState<ConflictState | null>(null);

  const chapters = useMemo(() => flattenPages(chaptersQuery.data), [chaptersQuery.data]);
  const chapterById = useMemo(() => {
    const map = new Map<number, ChapterSummary>();
    chapters.forEach((c) => map.set(c.chapter, c));
    return map;
  }, [chapters]);

  const selectedChapter = selectedChapterId ? chapterById.get(selectedChapterId) ?? null : null;

  function captureConflictState(error: unknown, base: Omit<ConflictState, 'currentRevision' | 'conflictingPath'>) {
    if (!isOutlineConflictError(error)) return;
    const details = error.details as
      | { current_revision?: number; conflicting_path?: string }
      | undefined;
    setConflict({
      ...base,
      currentRevision: details?.current_revision ?? outline.data?.outline_revision ?? 0,
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

  function onUseCurrent() {
    setConflict(null);
    void outline.refetch();
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
        <RevisionBadge revision={outline.data.outline_revision} status={patchStructure.isPending ? 'dirty' : 'clean'} />
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
          />

          <TimelinePanel
            outline={outline.data}
            selectedChapterId={selectedChapterId}
            baseRevision={outline.data.outline_revision}
            onPatchTimeline={handleTimeline}
          />
        </div>
      </div>

      {conflict ? (
        <OutlineConflictModal
          open
          currentRevision={conflict.currentRevision}
          draft={{
            fields: changedFieldsOf(conflict.pendingRequest),
            conflictingPath: conflict.conflictingPath,
          }}
          onUseCurrent={onUseCurrent}
          onReapply={onReapply}
          onDismiss={onDismiss}
        />
      ) : null}
    </div>
  );
}

function changedFieldsOf(pending: PendingPatch): OutlineChangedField[] {
  if (pending.kind === 'structure') {
    switch (pending.request.operation) {
      case 'move_chapter':
        return ['move_chapter'];
      case 'attach_to_volume':
        return ['attach_to_volume'];
      case 'link_event':
        return ['link_event'];
      default:
        return [];
    }
  }
  if (pending.kind === 'timeline') {
    switch (pending.request.operation) {
      case 'add_event':
        return ['add_event'];
      case 'remove_event':
        return ['remove_event'];
      case 'attach_event_to_chapter':
        return ['attach_event_to_chapter'];
      case 'link_foreshadow':
        return ['link_foreshadow'];
      default:
        return [];
    }
  }
  const set = pending.request.set;
  const fields: OutlineChangedField[] = [];
  if (set.title !== undefined) fields.push('chapter_title');
  if (set.slug !== undefined) fields.push('chapter_slug');
  if (set.volume !== undefined) fields.push('chapter_volume');
  if (set.status !== undefined) fields.push('chapter_status');
  if (set.planned_word_count !== undefined) fields.push('chapter_planned_word_count');
  if (set.actual_word_count !== undefined) fields.push('chapter_actual_word_count');
  return fields;
}

function OutlineStructurePanel({
  outline,
  chapters,
  selectedChapterId,
  onSelectChapter,
  onMoveChapter,
}: {
  outline: WorkOutline;
  chapters: ChapterSummary[];
  selectedChapterId: number | null;
  onSelectChapter: (id: number | null) => void;
  onMoveChapter: (chapterId: number, volumeId: number) => void;
}) {
  const assignedIds = useMemo(
    () => new Set(outline.volumes.flatMap((v) => v.chapter_ids)),
    [outline.volumes],
  );
  const unassigned = chapters.filter((c) => !assignedIds.has(c.chapter));

  return (
    <Card className="min-h-[480px]">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <BookOpen className="h-5 w-5 text-purple-700" aria-hidden />
          Volumes & Chapters
        </CardTitle>
        <CardDescription>Select a chapter to inspect or move it between volumes.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {outline.volumes.length === 0 && unassigned.length === 0 ? (
          <EmptyState title="No chapters yet" description="Chapters appear here once created." />
        ) : (
          <div className="space-y-5">
            {outline.volumes.map((volume) => (
              <VolumeSection
                key={volume.volume_id}
                volume={volume}
                outline={outline}
                chapters={chapters}
                selectedChapterId={selectedChapterId}
                onSelectChapter={onSelectChapter}
                onMoveChapter={onMoveChapter}
              />
            ))}
            {unassigned.length > 0 && (
              <div>
                <h4 className="text-label-14 font-semibold text-gray-900">Unassigned</h4>
                <ul className="mt-2 space-y-1">
                  {unassigned.map((c) => (
                    <ChapterRow
                      key={c.chapter}
                      chapter={c}
                      outline={outline}
                      selected={selectedChapterId === c.chapter}
                      onSelect={() => onSelectChapter(c.chapter)}
                    />
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function VolumeSection({
  volume,
  outline,
  chapters,
  selectedChapterId,
  onSelectChapter,
  onMoveChapter,
}: {
  volume: WorkOutline['volumes'][number];
  outline: WorkOutline;
  chapters: ChapterSummary[];
  selectedChapterId: number | null;
  onSelectChapter: (id: number | null) => void;
  onMoveChapter: (chapterId: number, volumeId: number) => void;
}) {
  return (
    <div className="rounded-card border border-gray-alpha-300 bg-background-100 p-3">
      <div className="flex items-center justify-between">
        <h4 className="text-label-14 font-semibold text-gray-900">{volume.label || `Volume ${volume.volume_id}`}</h4>
        <span className="text-label-12 text-gray-700">{volume.chapter_ids.length} chapters</span>
      </div>
      <ul className="mt-2 space-y-1">
        {volume.chapter_ids.map((id) => {
          const chapter = chapters.find((c) => c.chapter === id);
          if (!chapter) return null;
          const nextVolume = outline.volumes.find((v) => v.volume_id === volume.volume_id + 1);
          return (
            <li key={id} className="flex items-center gap-2">
              <ChapterRow
                chapter={chapter}
                outline={outline}
                selected={selectedChapterId === id}
                onSelect={() => onSelectChapter(id)}
              />
              {nextVolume ? (
                <button
                  type="button"
                  onClick={() => onMoveChapter(id, nextVolume.volume_id)}
                  className="rounded-control p-1 text-gray-700 hover:bg-gray-alpha-100"
                  aria-label={`Move chapter ${id} to ${nextVolume.label || `Volume ${nextVolume.volume_id}`}`}
                  title={`Move to ${nextVolume.label || `Volume ${nextVolume.volume_id}`}`}
                >
                  <ChevronRight className="h-4 w-4" aria-hidden />
                </button>
              ) : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function ChapterRow({
  chapter,
  outline,
  selected,
  onSelect,
}: {
  chapter: ChapterSummary;
  outline: WorkOutline;
  selected: boolean;
  onSelect: () => void;
}) {
  const titles = outline.chapter_titles as Record<string, string> | undefined;
  const title = titles?.[String(chapter.chapter)] ?? chapter.title ?? `Chapter ${chapter.chapter}`;
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      className={[
        'flex flex-1 items-center justify-between rounded-control border px-3 py-2 text-left transition-colors duration-state ease-standard',
        selected
          ? 'border-blue-700 bg-blue-700/5'
          : 'border-gray-alpha-300 bg-background-100 hover:bg-background-200',
      ].join(' ')}
    >
      <span className="text-copy-14 text-gray-1000">
        <span className="font-mono text-gray-700">#{chapter.chapter}</span>{' '}
        {title}
      </span>
      <Badge variant={STATUS_VARIANT[chapter.status]}>{chapter.status.replace(/_/g, ' ')}</Badge>
    </button>
  );
}

function ChapterInspector({
  workId,
  outline,
  chapter,
  baseRevision,
  onPatchChapter,
  onMove,
}: {
  workId: string;
  outline: WorkOutline;
  chapter: ChapterSummary | null;
  baseRevision: number;
  onPatchChapter: (chapter: number, request: OutlinePatchChapterRequest) => void;
  onMove: (chapterId: number, volumeId: number) => void;
}) {
  const titles = outline.chapter_titles as Record<string, string> | undefined;
  const [title, setTitle] = useState('');
  const [slug, setSlug] = useState('');
  const [status, setStatus] = useState<ChapterStatus>('not_started');
  const [planned, setPlanned] = useState('');
  const [volume, setVolume] = useState('');

  // Reset local form when selection changes.
  useEffect(() => {
    if (!chapter) return;
    setTitle(titles?.[String(chapter.chapter)] ?? chapter.title ?? '');
    setSlug(chapter.slug ?? '');
    setStatus(chapter.status);
    setPlanned(String(chapter.planned_word_count ?? ''));
    const currentVolume = outline.volumes.find((v) =>
      v.chapter_ids.includes(chapter.chapter),
    );
    setVolume(String(currentVolume?.volume_id ?? ''));
  }, [chapter, outline.volumes, titles]);

  if (!chapter) {
    return (
      <Card>
        <CardContent className="py-12 text-center text-copy-14 text-gray-700">
          Select a chapter to inspect its outline metadata.
        </CardContent>
      </Card>
    );
  }

  const isPublished = chapter.status === 'published';
  const isFinalized = chapter.status === 'finalized';

  function save() {
    if (!chapter) return;
    const set: OutlinePatchChapterRequest['set'] = {};
    const currentTitle = titles?.[String(chapter.chapter)] ?? chapter.title ?? '';
    if (title !== currentTitle) set.title = title;
    if (slug !== (chapter.slug ?? '')) set.slug = slug;
    if (status !== chapter.status) set.status = status;
    if (planned !== String(chapter.planned_word_count ?? '')) {
      const n = Number.parseInt(planned, 10);
      if (!Number.isNaN(n)) set.planned_word_count = n;
    }
    const currentVolumeId = outline.volumes.find((v) => v.chapter_ids.includes(chapter.chapter))?.volume_id;
    if (volume !== String(currentVolumeId ?? '')) {
      const n = Number.parseInt(volume, 10);
      if (!Number.isNaN(n)) set.volume = n;
    }

    if (Object.keys(set).length === 0) return;

    if (
      isFinalized &&
      !window.confirm(
        'This chapter is finalized. Editing it will remove the finalized protection. Continue?',
      )
    ) {
      return;
    }

    onPatchChapter(chapter.chapter, {
      work_id: workId,
      chapter_id: chapter.chapter,
      base_revision: baseRevision,
      set,
    });
  }

  const currentVolume = outline.volumes.find((v) => v.chapter_ids.includes(chapter.chapter));
  const currentVolumeIndex = currentVolume
    ? outline.volumes.findIndex((v) => v.volume_id === currentVolume.volume_id)
    : -1;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Chapter Inspector</CardTitle>
        <CardDescription>
          <span className="font-mono">#{chapter.chapter}</span> metadata exposed on the outline canvas.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {isPublished ? (
          <div className="rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000">
            <AlertTriangle className="mr-1.5 inline h-4 w-4" aria-hidden />
            This chapter is published. Edits must be made through a fork or
            revision workflow.
          </div>
        ) : null}

        <label className="flex flex-col gap-1 text-copy-13">
          <span className="text-gray-700">Title</span>
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            disabled={isPublished}
            className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700 disabled:bg-gray-100 disabled:text-gray-700"
          />
        </label>

        <label className="flex flex-col gap-1 text-copy-13">
          <span className="text-gray-700">Slug</span>
          <input
            type="text"
            value={slug}
            onChange={(e) => setSlug(e.target.value)}
            disabled={isPublished}
            className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700 disabled:bg-gray-100 disabled:text-gray-700"
          />
        </label>

        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1 text-copy-13">
            <span className="text-gray-700">Status</span>
            <select
              value={status}
              onChange={(e) => setStatus(e.target.value as ChapterStatus)}
              disabled={isPublished}
              className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700 disabled:bg-gray-100 disabled:text-gray-700"
            >
              {STATUS_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
          </label>

          <label className="flex flex-col gap-1 text-copy-13">
            <span className="text-gray-700">Planned words</span>
            <input
              type="number"
              value={planned}
              onChange={(e) => setPlanned(e.target.value)}
              disabled={isPublished}
              className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700 disabled:bg-gray-100 disabled:text-gray-700"
            />
          </label>
        </div>

        <label className="flex flex-col gap-1 text-copy-13">
          <span className="text-gray-700">Volume</span>
          <select
            value={volume}
            onChange={(e) => {
              const next = Number.parseInt(e.target.value, 10);
              if (!Number.isNaN(next)) setVolume(String(next));
            }}
            disabled={isPublished}
            className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700 disabled:bg-gray-100 disabled:text-gray-700"
          >
            <option value="">Unassigned</option>
            {outline.volumes.map((v) => (
              <option key={v.volume_id} value={v.volume_id}>
                {v.label || `Volume ${v.volume_id}`}
              </option>
            ))}
          </select>
        </label>

        <div className="flex items-center gap-2 pt-1">
          {currentVolumeIndex > 0 ? (
            <Button
              variant="secondary"
              size="small"
              onClick={() =>
                onMove(chapter.chapter, outline.volumes[currentVolumeIndex - 1].volume_id)
              }
              disabled={isPublished}
            >
              <ChevronLeft className="h-4 w-4" aria-hidden /> Prev volume
            </Button>
          ) : null}
          {currentVolumeIndex >= 0 && currentVolumeIndex < outline.volumes.length - 1 ? (
            <Button
              variant="secondary"
              size="small"
              onClick={() =>
                onMove(chapter.chapter, outline.volumes[currentVolumeIndex + 1].volume_id)
              }
              disabled={isPublished}
            >
              Next volume <ChevronRight className="h-4 w-4" aria-hidden />
            </Button>
          ) : null}
          <Button
            variant="primary"
            size="small"
            onClick={save}
            disabled={isPublished}
            className="ml-auto"
          >
            <Save className="h-4 w-4" aria-hidden /> Save chapter
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function TimelinePanel({
  outline,
  selectedChapterId,
  baseRevision,
  onPatchTimeline,
}: {
  outline: WorkOutline;
  selectedChapterId: number | null;
  baseRevision: number;
  onPatchTimeline: (request: TimelinePatchEventRequest) => void;
}) {
  const [newTitle, setNewTitle] = useState('');
  const [newDescription, setNewDescription] = useState('');

  function addEvent() {
    if (!newTitle.trim()) return;
    onPatchTimeline({
      work_id: outline.work_id,
      base_revision: baseRevision,
      operation: 'add_event',
      title: newTitle.trim(),
      description: newDescription.trim() || undefined,
      realizes_chapter_id: selectedChapterId ?? undefined,
    });
    setNewTitle('');
    setNewDescription('');
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <CalendarPlus className="h-5 w-5 text-teal-700" aria-hidden />
          Timeline
        </CardTitle>
        <CardDescription>Events, beats, and foreshadow links.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {outline.timeline_events.length === 0 ? (
          <p className="text-copy-13 text-gray-700">No timeline events yet.</p>
        ) : (
          <ul className="space-y-2">
            {outline.timeline_events.map((event) => (
              <li
                key={event.event_id}
                className="flex items-start justify-between rounded-control border border-gray-alpha-300 bg-background-100 p-2"
              >
                <div>
                  <p className="text-copy-14 font-medium text-gray-1000">{event.title}</p>
                  {event.description ? (
                    <p className="text-copy-13 text-gray-700">{event.description}</p>
                  ) : null}
                  {event.realizes_chapter_id ? (
                    <p className="text-label-12 text-gray-700">
                      Chapter {event.realizes_chapter_id}
                    </p>
                  ) : null}
                </div>
                <div className="flex items-center gap-1">
                  {selectedChapterId && selectedChapterId !== event.realizes_chapter_id ? (
                    <button
                      type="button"
                      onClick={() =>
                        onPatchTimeline({
                          work_id: outline.work_id,
                          base_revision: baseRevision,
                          operation: 'attach_event_to_chapter',
                          event_id: event.event_id,
                          realizes_chapter_id: selectedChapterId,
                        })
                      }
                      className="rounded-control p-1 text-gray-700 hover:bg-gray-alpha-100"
                      aria-label={`Attach event to chapter ${selectedChapterId}`}
                      title="Attach to selected chapter"
                    >
                      <Link2 className="h-4 w-4" aria-hidden />
                    </button>
                  ) : null}
                  <button
                    type="button"
                    onClick={() =>
                      onPatchTimeline({
                        work_id: outline.work_id,
                        base_revision: baseRevision,
                        operation: 'remove_event',
                        event_id: event.event_id,
                      })
                    }
                    className="rounded-control p-1 text-gray-700 hover:bg-gray-alpha-100"
                    aria-label={`Remove event ${event.title}`}
                    title="Remove event"
                  >
                    <Trash2 className="h-4 w-4" aria-hidden />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}

        <div className="rounded-card border border-gray-alpha-300 bg-background-100 p-3 space-y-2">
          <p className="text-label-14 font-semibold text-gray-900">Add Event</p>
          <input
            type="text"
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            placeholder="Event title…"
            className="w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700"
          />
          <textarea
            value={newDescription}
            onChange={(e) => setNewDescription(e.target.value)}
            placeholder="Description (optional)…"
            rows={2}
            className="w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700"
          />
          <Button variant="secondary" size="small" onClick={addEvent} disabled={!newTitle.trim()}>
            <ArrowRight className="h-4 w-4" aria-hidden /> Add to timeline
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function RevisionBadge({ revision, status }: { revision: number; status: 'clean' | 'dirty' | 'conflict' }) {
  const color =
    status === 'conflict'
      ? 'border-canvas-write-conflict text-canvas-write-conflict bg-canvas-write-conflict/10'
      : status === 'dirty'
        ? 'border-canvas-write-dirty text-canvas-write-dirty bg-canvas-write-dirty/10'
        : 'border-gray-alpha-400 text-gray-700 bg-background-100';
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-pill border px-2 py-0.5 text-label-12 ${color}`}
      title={status === 'conflict' ? 'Revision conflict — refetch before editing' : undefined}
    >
      {status === 'conflict' ? <AlertTriangle className="h-3 w-3" aria-hidden /> : null}
      rev {revision}
    </span>
  );
}
