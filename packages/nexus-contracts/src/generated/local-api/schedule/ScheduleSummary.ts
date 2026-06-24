import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ScheduleSummary
 *
 * Summary row for a schedule in list/inspect responses.
 *
 * @schema_version 1
 * @source schedule-summary.schema.json
 */
/** Summary row for a schedule in list/inspect responses. */
export interface ScheduleSummary {
  schedule_id: string;
  creator_id: string;
  preset_id: string;
  status: string;
  label?: string;
  current_core_context_version: number;
  created_at: string;
  updated_at: string;
}
