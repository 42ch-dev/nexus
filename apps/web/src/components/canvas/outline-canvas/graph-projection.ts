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

/** i18n key for each chapter lifecycle status. */
export const STATUS_LABEL_KEYS: Record<ChapterStatus, string> = {
  not_started: 'chapter.status.not_started',
  outlined: 'chapter.status.outlined',
  draft: 'chapter.status.draft',
  finalized: 'chapter.status.finalized',
  published: 'chapter.status.published',
};

/** Chapter lifecycle status value + i18n key for the inspector `<select>`. */
export const STATUS_OPTIONS: { value: ChapterStatus; labelKey: string }[] = [
  { value: 'not_started', labelKey: STATUS_LABEL_KEYS.not_started },
  { value: 'outlined', labelKey: STATUS_LABEL_KEYS.outlined },
  { value: 'draft', labelKey: STATUS_LABEL_KEYS.draft },
  { value: 'finalized', labelKey: STATUS_LABEL_KEYS.finalized },
  { value: 'published', labelKey: STATUS_LABEL_KEYS.published },
];

/** i18n keys for scene/beat lifecycle status. */
export const SCENE_STATUS_LABEL_KEYS: Record<OutlineSceneStatus, string> = {
  drafted: 'scene.status.drafted',
  completed: 'scene.status.completed',
};

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

// ---------------------------------------------------------------------------
// Scene/Beat fixture types (V1.109 C2 — fixture-driven read-projection)
//
// The outline wire model carries no scene/beat data (architect-locked §5.2 Q1).
// Design Studio / test fixtures inject scene/beat payloads at the UI projection
// layer. On real Works (no scene/beat data today), the projection emits zero
// scene/beat children — honest empty chrome.
// ---------------------------------------------------------------------------

/**
 * Scene/Beat lifecycle status (two-value, no pending tier). Shared by Scene
 * and Beat node data. Matches `OutlineSceneNodeData.status` consumed by the
 * Scene/Beat node components (`scene-beat-nodes.tsx`).
 */
export type OutlineSceneStatus = 'drafted' | 'completed';

/**
 * Fixture shape for a single Scene — injected at the projection call site.
 * `chapterId` ties the scene to its parent Chapter node (`chapter:<chapterId>`).
 */
export interface SceneFixture {
  sceneId: string;
  chapterId: number;
  title: string | null;
  status: OutlineSceneStatus | null;
}

/**
 * Fixture shape for a single Beat — injected at the projection call site.
 * `sceneId` ties the beat to its parent Scene node (`scene:<sceneId>`,
 * Scene→Beat nesting per §5.2 Q2).
 */
export interface BeatFixture {
  beatId: string;
  sceneId: string;
  title: string | null;
  status: OutlineSceneStatus | null;
}

/**
 * Fixture payload for scene/beat data injected into {@link projectOutlineGraph}.
 * Empty by default on real Works — chapters then render with zero scene/beat
 * children.
 */
export interface SceneBeatFixturePayload {
  scenes: SceneFixture[];
  beats: BeatFixture[];
}

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
  fallback?: string,
): string {
  const fallbackTitle = fallback ? `${fallback} ${chapter.chapter}` : `Chapter ${chapter.chapter}`;
  return (
    titles?.[String(chapter.chapter)] ??
    chapter.title ??
    fallbackTitle
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
      case 'unlink_foreshadow':
        return ['unlink_foreshadow'];
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
  if (set.content !== undefined) fields.push('chapter_outline_content');
  return fields;
}
