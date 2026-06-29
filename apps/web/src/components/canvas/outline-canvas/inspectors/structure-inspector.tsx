/**
 * Outline canvas — structure inspector (V1.73 B5 split, `R-V172P0-QC1-002`).
 *
 * Renders the Volumes & Chapters tree: the per-volume sections, the
 * "Unassigned" bucket, and the selectable chapter rows. Extracted from the
 * original `outline-canvas.tsx` monolith; behavior is unchanged.
 */
import { useMemo } from 'react';
import { BookOpen, ChevronRight } from 'lucide-react';
import { FixedSizeList, type ListChildComponentProps } from 'react-window';

import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState } from '@/components/ui/states';

import {
  STATUS_VARIANT,
  chapterDisplayTitle,
  unassignedChaptersOf,
} from '../graph-projection';
import type { ChapterSummary, WorkOutline } from '@42ch/nexus-contracts';

const CHAPTER_ROW_HEIGHT = 48;
const MAX_LIST_HEIGHT = 384;

interface OutlineStructurePanelProps {
  outline: WorkOutline;
  chapters: ChapterSummary[];
  selectedChapterId: number | null;
  onSelectChapter: (id: number | null) => void;
  onMoveChapter: (chapterId: number, volumeId: number) => void;
}

export function OutlineStructurePanel({
  outline,
  chapters,
  selectedChapterId,
  onSelectChapter,
  onMoveChapter,
}: OutlineStructurePanelProps) {
  const unassigned = useMemo(
    () => unassignedChaptersOf(outline, chapters),
    [outline, chapters],
  );

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
                <VirtualUnassignedList
                  unassigned={unassigned}
                  outline={outline}
                  selectedChapterId={selectedChapterId}
                  onSelectChapter={onSelectChapter}
                />
              </div>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

interface VolumeSectionProps {
  volume: WorkOutline['volumes'][number];
  outline: WorkOutline;
  chapters: ChapterSummary[];
  selectedChapterId: number | null;
  onSelectChapter: (id: number | null) => void;
  onMoveChapter: (chapterId: number, volumeId: number) => void;
}

function VolumeSection({
  volume,
  outline,
  chapters,
  selectedChapterId,
  onSelectChapter,
  onMoveChapter,
}: VolumeSectionProps) {
  const listHeight = Math.min(volume.chapter_ids.length * CHAPTER_ROW_HEIGHT, MAX_LIST_HEIGHT);
  return (
    <div className="rounded-card border border-gray-alpha-300 bg-background-100 p-3">
      <div className="flex items-center justify-between">
        <h4 className="text-label-14 font-semibold text-gray-900">{volume.label || `Volume ${volume.volume_id}`}</h4>
        <span className="text-label-12 text-gray-700">{volume.chapter_ids.length} chapters</span>
      </div>
      <VirtualVolumeList
        volume={volume}
        outline={outline}
        chapters={chapters}
        selectedChapterId={selectedChapterId}
        onSelectChapter={onSelectChapter}
        onMoveChapter={onMoveChapter}
        height={listHeight}
      />
    </div>
  );
}

interface VolumeListData {
  volume: WorkOutline['volumes'][number];
  outline: WorkOutline;
  chapters: ChapterSummary[];
  selectedChapterId: number | null;
  onSelectChapter: (id: number | null) => void;
  onMoveChapter: (chapterId: number, volumeId: number) => void;
}

function VolumeRow({ index, style, data }: ListChildComponentProps<VolumeListData>) {
  const { volume, outline, chapters, selectedChapterId, onSelectChapter, onMoveChapter } = data;
  const id = volume.chapter_ids[index];
  const chapter = chapters.find((c) => c.chapter === id);
  if (!chapter) return null;
  const nextVolume = outline.volumes.find((v) => v.volume_id === volume.volume_id + 1);
  return (
    <li key={id} style={style} className="flex items-center gap-2">
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
}

function VirtualVolumeList({
  volume,
  outline,
  chapters,
  selectedChapterId,
  onSelectChapter,
  onMoveChapter,
  height,
}: VolumeListData & { height: number }) {
  const itemData = useMemo(
    () => ({
      volume,
      outline,
      chapters,
      selectedChapterId,
      onSelectChapter,
      onMoveChapter,
    }),
    [volume, outline, chapters, selectedChapterId, onSelectChapter, onMoveChapter],
  );

  return (
    <FixedSizeList
      className="mt-2"
      innerElementType="ul"
      itemCount={volume.chapter_ids.length}
      itemData={itemData}
      itemSize={CHAPTER_ROW_HEIGHT}
      height={height}
      width="100%"
    >
      {VolumeRow}
    </FixedSizeList>
  );
}

interface UnassignedListData {
  unassigned: ChapterSummary[];
  outline: WorkOutline;
  selectedChapterId: number | null;
  onSelectChapter: (id: number | null) => void;
}

function UnassignedRow({ index, style, data }: ListChildComponentProps<UnassignedListData>) {
  const { unassigned, outline, selectedChapterId, onSelectChapter } = data;
  const chapter = unassigned[index];
  return (
    <li key={chapter.chapter} style={style}>
      <ChapterRow
        chapter={chapter}
        outline={outline}
        selected={selectedChapterId === chapter.chapter}
        onSelect={() => onSelectChapter(chapter.chapter)}
      />
    </li>
  );
}

function VirtualUnassignedList({
  unassigned,
  outline,
  selectedChapterId,
  onSelectChapter,
}: UnassignedListData) {
  const listHeight = Math.min(unassigned.length * CHAPTER_ROW_HEIGHT, MAX_LIST_HEIGHT);
  const itemData = useMemo(
    () => ({ unassigned, outline, selectedChapterId, onSelectChapter }),
    [unassigned, outline, selectedChapterId, onSelectChapter],
  );

  return (
    <FixedSizeList
      className="mt-2"
      innerElementType="ul"
      itemCount={unassigned.length}
      itemData={itemData}
      itemSize={CHAPTER_ROW_HEIGHT}
      height={listHeight}
      width="100%"
    >
      {UnassignedRow}
    </FixedSizeList>
  );
}

interface ChapterRowProps {
  chapter: ChapterSummary;
  outline: WorkOutline;
  selected: boolean;
  onSelect: () => void;
}

function ChapterRow({ chapter, outline, selected, onSelect }: ChapterRowProps) {
  const titles = outline.chapter_titles as Record<string, string> | undefined;
  const title = chapterDisplayTitle(chapter, titles);
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
