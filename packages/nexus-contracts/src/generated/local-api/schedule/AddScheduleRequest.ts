import type { ScheduleConcurrencyRequest } from './ScheduleConcurrencyRequest';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus AddScheduleRequest
 *
 * Request body for POST /v1/local/orchestration/schedules — create a new schedule.
 *
 * @schema_version 1
 * @source add-schedule-request.schema.json
 */
/** Request body for POST /v1/local/orchestration/schedules — create a new schedule. */
export interface AddScheduleRequest {
  creator_id: string;
  preset_id: string;
  seed?: string;
  label?: string;
  depends_on?: string[];
  concurrency?: ScheduleConcurrencyRequest;
  scheduled_at?: string;
  input?: unknown;
  force_gates?: boolean;
  reason?: string;
}
