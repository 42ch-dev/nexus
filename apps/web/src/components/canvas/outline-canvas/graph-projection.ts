/**
 * Outline canvas — pure projection + shared logic (V1.73 B5 split,
 * `R-V172P0-QC1-002`).
 *
 * Holds the non-JSX projection helpers, shared constants, and conflict-state
 * types used across the outline-canvas modules. Extracted from the original
 * 825-line `outline-canvas.tsx` monolith so each canvas file stays focused
 * (≤250 lines) and Track A (World KB canvas) can reuse the conflict-shape
 * through the public facade.
 */
import type {
  ChapterStatus,
  ChapterSummary,
  OutlinePatchChapterRequest,
  OutlinePatchStructureRequest,
  TimelinePatchEventRequest,
  WorkOutline,
} from '@42ch/nexus-contracts';

import type { OutlineChangedField } from '@/components/canvas/outline-conflict-modal';

/** Chapter lifecycle status label + value options for the inspector `<select>`. */
export const STATUS_OPTIONS: { value: ChapterStatus; label: string }[] = [
  { value: 'not_started', label: 'Not started' },
  { value: 'outlined', label: 'Outlined' },
  { value: 'draft', label: 'Draft' },
  { value: 'finalized', label: 'Finalized' },
  { value: 'published', label: 'Published' },
];

/** Maps a chapter status onto a Badge variant for the structure projection. */
export const STATUS_VARIANT: Record<
  ChapterStatus,
  'neutral' | 'queued' | 'warning' | 'running' | 'preset'
> = {
  not_started: 'neutral',
  outlined: 'queued',
  draft: 'warning',
  finalized: 'running',
  published: 'preset',
};

/** A pending canvas patch awaiting confirmation, captured for conflict replay. */
export type PendingPatch =
  | { kind: 'structure'; request: OutlinePatchStructureRequest }
  | { kind: 'chapter'; chapter: number; request: OutlinePatchChapterRequest }
  | { kind: 'timeline'; request: TimelinePatchEventRequest };

/** Structured conflict state surfaced by a 409 from the daemon. */
export interface ConflictState {
  currentRevision: number;
  conflictingPath: string;
  pendingRequest: PendingPatch;
}

/**
 * Chapters in `chapters` that are not referenced by any volume in `outline`.
 * Used by the structure panel to render the "Unassigned" bucket.
 */
export function unassignedChaptersOf(
  outline: WorkOutline,
  chapters: ChapterSummary[],
): ChapterSummary[] {
  const assignedIds = new Set(outline.volumes.flatMap((v) => v.chapter_ids));
  return chapters.filter((c) => !assignedIds.has(c.chapter));
}

/**
 * Resolve the human-facing display title for a chapter, preferring the
 * outline's `chapter_titles` UI map, then the chapter's own title, then a
 * localized fallback.
 */
export function chapterDisplayTitle(
  chapter: { chapter: number; title?: string | null },
  titles: Record<string, string> | undefined,
  fallback = `Chapter`,
): string {
  return (
    titles?.[String(chapter.chapter)] ??
    chapter.title ??
    `${fallback} ${chapter.chapter}`
  );
}

/**
 * Project a pending patch into the conflict-modal's changed-field list.
 *
 * Structure/timeline patches surface their operation kind; chapter patches
 * surface each individually-edited `set` field.
 */
export function changedFieldsOf(pending: PendingPatch): OutlineChangedField[] {
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
