import type { ChapterStatus } from './ChapterStatus';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ListChaptersQuery
 *
 * Query parameters for GET /v1/local/works/{work_id}/chapters (V1.65 P0). Cursor-based pagination with optional status filter.
 *
 * @schema_version 1
 * @source list-chapters-query.schema.json
 */
/** Query parameters for GET /v1/local/works/{work_id}/chapters (V1.65 P0). Cursor-based pagination with optional status filter. */
export interface ListChaptersQuery {
  status?: ChapterStatus;
  limit?: number;
  cursor?: string;
}
