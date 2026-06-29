/**
 * Outline canvas — chapter inspector (V1.73 B5 split, `R-V172P0-QC1-002`;
 * V1.75 A3 mounts the outline-content editor below the metadata fields).
 *
 * Edits chapter outline metadata (title/slug/status/planned words/volume) via
 * `patch_outline_chapter`, with prev/next volume moves. V1.75 closes the V1.65
 * prose-editing parity gap by mounting `ChapterOutlineContentEditor`. Published
 * chapters are read-only.
 */
import { useEffect, useState } from 'react';
import { AlertTriangle, ChevronLeft, ChevronRight, Save } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

import { STATUS_OPTIONS } from '../graph-projection';
import { ChapterOutlineContentEditor } from './chapter-outline-content-editor';
import type {
  ChapterStatus,
  ChapterSummary,
  OutlinePatchChapterRequest,
  WorkOutline,
} from '@42ch/nexus-contracts';

interface ChapterInspectorProps {
  workId: string;
  outline: WorkOutline;
  chapter: ChapterSummary | null;
  baseRevision: number;
  onPatchChapter: (chapter: number, request: OutlinePatchChapterRequest) => void;
  onMove: (chapterId: number, volumeId: number) => void;
  /** True while a chapter patch mutation is in flight (metadata or content). */
  patchIsPending: boolean;
  /** Monotonic counter the orchestrator bumps to request an editor reset after
   * a refetch (e.g. following conflict resolution). */
  contentVersion: number;
}

export function ChapterInspector({
  workId,
  outline,
  chapter,
  baseRevision,
  onPatchChapter,
  onMove,
  patchIsPending,
  contentVersion,
}: ChapterInspectorProps) {
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
  const currentVolume = outline.volumes.find((v) => v.chapter_ids.includes(chapter.chapter));
  const currentVolumeIndex = currentVolume
    ? outline.volumes.findIndex((v) => v.volume_id === currentVolume.volume_id)
    : -1;

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
    if (volume !== String(currentVolume?.volume_id ?? '')) {
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
            This chapter is published. Edits must be made through a fork or revision workflow.
          </div>
        ) : null}

        <MetaField label="Title">
          <input type="text" value={title} onChange={(e) => setTitle(e.target.value)} disabled={isPublished} className={INPUT_CLASS} />
        </MetaField>

        <MetaField label="Slug">
          <input type="text" value={slug} onChange={(e) => setSlug(e.target.value)} disabled={isPublished} className={INPUT_CLASS} />
        </MetaField>

        <div className="grid grid-cols-2 gap-3">
          <MetaField label="Status">
            <select
              value={status}
              onChange={(e) => setStatus(e.target.value as ChapterStatus)}
              disabled={isPublished}
              className={INPUT_CLASS}
            >
              {STATUS_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
          </MetaField>

          <MetaField label="Planned words">
            <input type="number" value={planned} onChange={(e) => setPlanned(e.target.value)} disabled={isPublished} className={INPUT_CLASS} />
          </MetaField>
        </div>

        <MetaField label="Volume">
          <select
            value={volume}
            onChange={(e) => {
              const next = Number.parseInt(e.target.value, 10);
              if (!Number.isNaN(next)) setVolume(String(next));
            }}
            disabled={isPublished}
            className={INPUT_CLASS}
          >
            <option value="">Unassigned</option>
            {outline.volumes.map((v) => (
              <option key={v.volume_id} value={v.volume_id}>
                {v.label || `Volume ${v.volume_id}`}
              </option>
            ))}
          </select>
        </MetaField>

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

        <div className="border-t border-gray-alpha-400 pt-3">
          <ChapterOutlineContentEditor
            workId={workId}
            chapterNumber={chapter.chapter}
            baseRevision={baseRevision}
            volume={currentVolume?.volume_id}
            disabled={isPublished}
            onPatchChapter={onPatchChapter}
            patchIsPending={patchIsPending}
            contentVersion={contentVersion}
          />
        </div>
      </CardContent>
    </Card>
  );
}

/** Shared form-control class for the metadata inputs (DESIGN.md tokens). */
const INPUT_CLASS =
  'rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700 disabled:bg-gray-100 disabled:text-gray-700';

/** Label + control wrapper for the metadata fields. */
function MetaField({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1 text-copy-13">
      <span className="text-gray-700">{label}</span>
      {children}
    </label>
  );
}
