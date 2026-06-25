import type { ChapterSummary } from './ChapterSummary';
import type { PaginationInfo } from '../../kb/PaginationInfo';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ListChaptersResponse
 *
 * Response for GET /v1/local/works/{work_id}/chapters (V1.65 P0). Cursor-based pagination over ChapterSummary rows. Uses `items` key per F-P3.
 *
 * @schema_version 1
 * @source list-chapters-response.schema.json
 */
/** Response for GET /v1/local/works/{work_id}/chapters (V1.65 P0). Cursor-based pagination over ChapterSummary rows. Uses `items` key per F-P3. */
export interface ListChaptersResponse {
  items: ChapterSummary[];
  pagination: PaginationInfo;
}
