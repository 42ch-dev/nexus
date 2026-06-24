import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus AddScheduleResponse
 *
 * Response for POST /v1/local/orchestration/schedules.
 *
 * @schema_version 1
 * @source add-schedule-response.schema.json
 */
/** Response for POST /v1/local/orchestration/schedules. */
export interface AddScheduleResponse {
  schedule_id: string;
  status: string;
  core_context_version: number;
}
