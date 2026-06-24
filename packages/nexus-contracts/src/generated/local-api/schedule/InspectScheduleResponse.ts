import type { ScheduleSummary } from './ScheduleSummary';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus InspectScheduleResponse
 *
 * Response for GET /v1/local/orchestration/schedules/{schedule_id}.
 *
 * @schema_version 1
 * @source inspect-schedule-response.schema.json
 */
/** Response for GET /v1/local/orchestration/schedules/{schedule_id}. */
export interface InspectScheduleResponse {
  schedule: ScheduleSummary;
  depends_on: string[];
  concurrency_kind: string;
}
