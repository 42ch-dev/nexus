import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SignalScheduleRequest
 *
 * Request body for POST /v1/local/orchestration/schedules/{schedule_id}/signal.
 *
 * @schema_version 1
 * @source signal-schedule-request.schema.json
 */
/** Request body for POST /v1/local/orchestration/schedules/{schedule_id}/signal. */
export interface SignalScheduleRequest {
  signal: string;
}
