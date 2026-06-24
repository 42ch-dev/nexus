import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListSchedulesQuery
 *
 * Query parameters for GET /v1/local/orchestration/schedules.
 *
 * @schema_version 1
 * @source list-schedules-query.schema.json
 */
/** Query parameters for GET /v1/local/orchestration/schedules. */
export interface ListSchedulesQuery {
  creator_id?: string;
  status?: string;
}
