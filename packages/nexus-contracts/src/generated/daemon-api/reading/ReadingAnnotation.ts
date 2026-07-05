import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReadingAnnotation
 *
 * Shared annotation detail object returned by POST, PATCH, and as list items in GET /v1/daemon/reading/annotations. Represents a single persistent highlight with optional note, anchored by character offsets into the chapter body plain text.
 *
 * @schema_version 1
 * @source reading-annotation.schema.json
 */

/** Inline enum type */
export type ReadingAnnotationColor = 'yellow' | 'blue' | 'green' | 'pink';

/** Shared annotation detail object returned by POST, PATCH, and as list items in GET /v1/daemon/reading/annotations. Represents a single persistent highlight with optional note, anchored by character offsets into the chapter body plain text. */
export interface ReadingAnnotation {
  annotation_id: string;
  work_id: string;
  chapter: number;
  start_offset: number;
  end_offset: number;
  selected_text: string;
  color: ReadingAnnotationColor;
  note?: string;
  created_at: string;
  updated_at: string;
}
