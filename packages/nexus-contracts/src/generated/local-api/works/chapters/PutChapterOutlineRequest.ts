import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus PutChapterOutlineRequest
 *
 * Request body for PUT /v1/local/works/{work_id}/chapters/{n}/outline (V1.65 P0).
 *
 * @schema_version 1
 * @source put-chapter-outline-request.schema.json
 */
/** Request body for PUT /v1/local/works/{work_id}/chapters/{n}/outline (V1.65 P0). */
export interface PutChapterOutlineRequest {
  content: string;
}
