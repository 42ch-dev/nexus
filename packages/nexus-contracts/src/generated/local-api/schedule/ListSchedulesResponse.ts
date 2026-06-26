import type { PaginationInfo } from '../kb/PaginationInfo';
import type { ScheduleSummary } from './ScheduleSummary';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListSchedulesResponse
 *
 * Response for GET /v1/local/orchestration/schedules (cursor-based pagination, F-P3). The array field is `items`; the legacy `schedules` key was removed in `@42ch/nexus-contracts` 0.6.0.
 *
 * @schema_version 2
 * @source list-schedules-response.schema.json
 */
/** Response for GET /v1/local/orchestration/schedules (cursor-based pagination, F-P3). The array field is `items`; the legacy `schedules` key was removed in `@42ch/nexus-contracts` 0.6.0. */
export interface ListSchedulesResponse {
  items: ScheduleSummary[];
  pagination: PaginationInfo;
}
