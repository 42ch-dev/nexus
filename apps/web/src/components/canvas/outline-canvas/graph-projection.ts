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
// Scene/Beat projection payload (V1.109 C2 → V1.200 DR-26 carrier swap)
//
// V1.200: the outline wire model carries `WorkOutline.scenes[]` / `.beats[]`
// (snake_case, the canonical carrier). The App projects THOSE arrays through
// {@link sceneBeatPayloadFromOutline}; the projection emits zero scene/beat
// children only when the canonical arrays are genuinely empty — empty
// canonical arrays mean real emptiness, never permission to substitute
// sample data.
//
// The `sceneBeatFixture` props remain as an EXPLICIT injection mode for
// Design Studio / component tests (a populated or deliberately empty fixture
// supplied by the caller), never as an automatic empty-data fallback:
// production callers pass no fixture.
// ---------------------------------------------------------------------------

/**
 * Scene/Beat lifecycle status (two-value, no pending tier). Shared by Scene
 * and Beat node data. Matches `OutlineSceneNodeData.status` consumed by the
 * Scene/Beat node components (`scene-beat-nodes.tsx`).
 */
export type OutlineSceneStatus = 'drafted' | 'completed';

/**
 * Projection shape for a single Scene — the camelCase form of the canonical
 * wire item (`WorkOutline.scenes[]` → `{scene_id, chapter_id, title, status}`).
 * `chapterId` ties the scene to its parent Chapter node
 * (`chapter:<chapterId>`).
 */
export interface SceneFixture {
  sceneId: string;
  chapterId: number;
  title: string | null;
  status: OutlineSceneStatus | null;
  /**
   * Owning Work id — present only when composing across Works (World
   * Timeline Moment reads bound Works' outlines; chapter numbers are
   * Work-local, so grouping needs the Work dimension to avoid collapsing
   * two Works' chapter 1 into one chapter region).
   */
  workId?: string;
}

/**
 * Projection shape for a single Beat — the camelCase form of the canonical
 * wire item (`WorkOutline.beats[]` → `{beat_id, scene_id, title, status}`).
 * `sceneId` ties the beat to its parent Scene node (`scene:<sceneId>`,
 * Scene→Beat nesting per §5.2 Q2).
 */
export interface BeatFixture {
  beatId: string;
  sceneId: string;
  title: string | null;
  status: OutlineSceneStatus | null;
  /** Owning Work id — see {@link SceneFixture.workId}. */
  workId?: string;
}

/**
 * Projection payload for scene/beat data consumed by the outline/Work/World
 * Timeline projections and by the Design Studio fixture components.
 */
export interface SceneBeatFixturePayload {
  scenes: SceneFixture[];
  beats: BeatFixture[];
}

/**
 * Map the canonical wire carrier (`WorkOutline.scenes[]` / `.beats[]`,
 * snake_case — V1.200 DR-26) onto the camelCase projection payload.
 *
 * `workId` tags every derived scene/beat with its owning Work for
 * cross-Work composition (World Timeline Moment); omit it for the Work-local
 * projection, where provenance is the graph's own `work_id`.
 */
export function sceneBeatPayloadFromOutline(
  outline: WorkOutline,
  workId?: string,
): SceneBeatFixturePayload {
  return {
    scenes: outline.scenes.map((scene) => ({
      sceneId: scene.scene_id,
      chapterId: scene.chapter_id,
      title: scene.title,
      status: scene.status,
      workId,
    })),
    beats: outline.beats.map((beat) => ({
      beatId: beat.beat_id,
      sceneId: beat.scene_id,
      title: beat.title,
      status: beat.status,
      workId,
    })),
  };
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
