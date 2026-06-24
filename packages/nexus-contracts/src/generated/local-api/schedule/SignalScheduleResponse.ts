import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SignalScheduleResponse
 *
 * Response for POST /v1/local/orchestration/schedules/{schedule_id}/signal.
 *
 * @schema_version 1
 * @source signal-schedule-response.schema.json
 */
/** Response for POST /v1/local/orchestration/schedules/{schedule_id}/signal. */
export interface SignalScheduleResponse {
  schedule_id: string;
  status: string;
}
