import type { ReadingAnnotation } from './ReadingAnnotation';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReadingAnnotationListResponse
 *
 * Response for GET /v1/local/reading/annotations. Returns all annotations for the current creator on the requested (work, chapter) as a flat list. No pagination — per-chapter annotation count is expected to stay bounded (dozens, not hundreds).
 *
 * @schema_version 1
 * @source reading-annotation-list-response.schema.json
 */
/** Response for GET /v1/local/reading/annotations. Returns all annotations for the current creator on the requested (work, chapter) as a flat list. No pagination — per-chapter annotation count is expected to stay bounded (dozens, not hundreds). */
export interface ReadingAnnotationListResponse {
  items: ReadingAnnotation[];
}
