import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListSchedulesQuery
 *
 * Query parameters for GET /v1/local/orchestration/schedules (cursor-based pagination + sort, F-F1).
 *
 * @schema_version 2
 * @source list-schedules-query.schema.json
 */
/** Query parameters for GET /v1/local/orchestration/schedules (cursor-based pagination + sort, F-F1). */
export interface ListSchedulesQuery {
  creator_id?: string;
  status?: string;
  limit?: number;
  cursor?: string;
  sort?: string;
}
