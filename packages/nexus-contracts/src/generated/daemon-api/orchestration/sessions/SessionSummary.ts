import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus OrchestrationSessionSummary
 *
 * Summary of an active orchestration engine session.
 *
 * @schema_version 1
 * @source session-summary.schema.json
 */
/** Summary of an active orchestration engine session. */
export interface SessionSummary {
  session_id: string;
  creator_id: string;
  preset_id: string;
  status: string;
  current_task_id?: string;
}
