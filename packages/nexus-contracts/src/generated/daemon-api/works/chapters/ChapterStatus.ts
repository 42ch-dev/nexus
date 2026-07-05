/**
 * Nexus ChapterStatus
 *
 * Lifecycle status of a work chapter (V1.65 P0).
 *
 * @schema_version 1
 * @source chapter-status.schema.json
 */

/** Lifecycle status of a work chapter (V1.65 P0). */
export type ChapterStatus = 'not_started' | 'outlined' | 'draft' | 'finalized' | 'published';
