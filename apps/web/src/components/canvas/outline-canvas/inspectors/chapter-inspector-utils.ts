/**
 * Chapter inspector — pure metadata-patch helpers (V1.76 B1 / `R-V175QC1-S001`).
 *
 * Extracted from `chapter-inspector.tsx` so that file stays ≤240 lines
 * (V1.73 split cap, `R-V172P0-QC1-002`). Behavior is unchanged: only fields
 * that differ from the chapter's canonical values land in the patch `set`, so
 * an unchanged form produces an empty `set` and the caller treats it as a
 * no-op save. Pure + unit-testable, no React, no side effects.
 */
import type {
  ChapterStatus,
  ChapterSummary,
  OutlinePatchChapterRequest,
  WorkOutline,
} from '@42ch/nexus-contracts';

/** Local form state held by the chapter inspector's metadata fields. */
export interface ChapterMetadataForm {
  title: string;
  slug: string;
  status: ChapterStatus;
  planned: string;
  volume: string;
}

/**
 * Build the patch `set` for a chapter metadata save by diffing the form
 * against the chapter's canonical outline values. Returns an empty object when
 * nothing changed (caller short-circuits). `planned`/`volume` parse to numbers;
 * non-numeric values are dropped (matches the pre-extraction behavior).
 */
export function buildChapterPatchSet(
  chapter: ChapterSummary,
  titles: Record<string, string> | undefined,
  form: ChapterMetadataForm,
  outline: WorkOutline,
): OutlinePatchChapterRequest['set'] {
  const set: OutlinePatchChapterRequest['set'] = {};

  const currentTitle = titles?.[String(chapter.chapter)] ?? chapter.title ?? '';
  if (form.title !== currentTitle) set.title = form.title;
  if (form.slug !== (chapter.slug ?? '')) set.slug = form.slug;
  if (form.status !== chapter.status) set.status = form.status;

  if (form.planned !== String(chapter.planned_word_count ?? '')) {
    const n = Number.parseInt(form.planned, 10);
    if (!Number.isNaN(n)) set.planned_word_count = n;
  }

  const currentVolume = outline.volumes.find((v) => v.chapter_ids.includes(chapter.chapter));
  if (form.volume !== String(currentVolume?.volume_id ?? '')) {
    const n = Number.parseInt(form.volume, 10);
    if (!Number.isNaN(n)) set.volume = n;
  }

  return set;
}
