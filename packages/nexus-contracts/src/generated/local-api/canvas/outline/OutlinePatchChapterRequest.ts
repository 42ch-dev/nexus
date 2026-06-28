import type { OutlinePatchChapterSet } from './OutlinePatchChapterSet';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus OutlinePatchChapterRequest
 *
 * Request body for POST /v1/local/works/{work_id}/chapters/{chapter_id}/patch (V1.72). Edits chapter-level metadata exposed on the outline canvas.
 *
 * @schema_version 1
 * @source outline-patch-chapter-request.schema.json
 */
/** Request body for POST /v1/local/works/{work_id}/chapters/{chapter_id}/patch (V1.72). Edits chapter-level metadata exposed on the outline canvas. */
export interface OutlinePatchChapterRequest {
  work_id: string;
  chapter_id: number;
  base_revision: number;
  set: OutlinePatchChapterSet;
}
