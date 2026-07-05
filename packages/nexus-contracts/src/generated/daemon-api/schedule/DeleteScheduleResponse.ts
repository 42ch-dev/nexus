import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus DeleteScheduleResponse
 *
 * Response for DELETE /v1/daemon/orchestration/schedules/{schedule_id}.
 *
 * @schema_version 1
 * @source delete-schedule-response.schema.json
 */
/** Response for DELETE /v1/daemon/orchestration/schedules/{schedule_id}. */
export interface DeleteScheduleResponse {
  deleted: boolean;
}
