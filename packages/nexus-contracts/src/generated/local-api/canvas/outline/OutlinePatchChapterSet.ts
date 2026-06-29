import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus OutlinePatchChapterSet
 *
 * Fields to update on a chapter via the outline canvas patch route (V1.72).
 *
 * @schema_version 1
 * @source outline-patch-chapter-set.schema.json
 */

/** Inline enum type */
export type OutlinePatchChapterSetStatus = 'not_started' | 'outlined' | 'draft' | 'finalized' | 'published';

/** Fields to update on a chapter via the outline canvas patch route (V1.72). */
export interface OutlinePatchChapterSet {
  title?: string;
  slug?: string;
  planned_word_count?: number;
  actual_word_count?: number;
  volume?: number;
  status?: OutlinePatchChapterSetStatus;
  content?: string;
}
