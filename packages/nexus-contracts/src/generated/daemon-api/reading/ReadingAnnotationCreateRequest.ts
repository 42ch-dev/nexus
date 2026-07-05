import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReadingAnnotationCreateRequest
 *
 * Request body for POST /v1/daemon/reading/annotations. Creates a persistent highlight anchored by character offsets into the chapter body plain text. Creator scope is inferred from the active session.
 *
 * @schema_version 1
 * @source reading-annotation-create-request.schema.json
 */

/** Inline enum type */
export type ReadingAnnotationCreateRequestColor = 'yellow' | 'blue' | 'green' | 'pink';

/** Request body for POST /v1/daemon/reading/annotations. Creates a persistent highlight anchored by character offsets into the chapter body plain text. Creator scope is inferred from the active session. */
export interface ReadingAnnotationCreateRequest {
  work_id: string;
  chapter: number;
  start_offset: number;
  end_offset: number;
  selected_text: string;
  color: ReadingAnnotationCreateRequestColor;
  note?: string;
}
