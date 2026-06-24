import type { ScheduleSummary } from './ScheduleSummary';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListSchedulesResponse
 *
 * Response for GET /v1/local/orchestration/schedules.
 *
 * @schema_version 1
 * @source list-schedules-response.schema.json
 */
/** Response for GET /v1/local/orchestration/schedules. */
export interface ListSchedulesResponse {
  schedules: ScheduleSummary[];
}
