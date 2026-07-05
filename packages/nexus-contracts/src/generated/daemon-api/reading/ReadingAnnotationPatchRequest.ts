import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReadingAnnotationPatchRequest
 *
 * Request body for PATCH /v1/daemon/reading/annotations/{annotation_id}. Edits the highlight color and/or optional note. Both fields are optional; at least one must be present. The annotation_id comes from the URL path, not the body.
 *
 * @schema_version 1
 * @source reading-annotation-patch-request.schema.json
 */

/** Inline enum type */
export type ReadingAnnotationPatchRequestColor = 'yellow' | 'blue' | 'green' | 'pink';

/** Request body for PATCH /v1/daemon/reading/annotations/{annotation_id}. Edits the highlight color and/or optional note. Both fields are optional; at least one must be present. The annotation_id comes from the URL path, not the body. */
export interface ReadingAnnotationPatchRequest {
  color?: ReadingAnnotationPatchRequestColor;
  note?: string;
}
