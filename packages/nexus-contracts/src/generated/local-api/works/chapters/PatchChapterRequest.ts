import type { ChapterStatus } from './ChapterStatus';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus PatchChapterRequest
 *
 * Request body for PATCH /v1/local/works/{work_id}/chapters/{n} (V1.65 P0). All fields optional. `title` is rejected because it is display-only until P0 materializes a title column.
 *
 * @schema_version 1
 * @source patch-chapter-request.schema.json
 */
/** Request body for PATCH /v1/local/works/{work_id}/chapters/{n} (V1.65 P0). All fields optional. `title` is rejected because it is display-only until P0 materializes a title column. */
export interface PatchChapterRequest {
  title?: string;
  slug?: string;
  planned_word_count?: number;
  volume?: number;
  status?: ChapterStatus;
  confirm_structural_edit?: boolean;
  transition_reason?: string;
}
