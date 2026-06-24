import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus WorkSummary
 *
 * Summary row for a work in list responses.
 *
 * @schema_version 1
 * @source work-summary.schema.json
 */
/** Summary row for a work in list responses. */
export interface WorkSummary {
  work_id: string;
  title: string;
  status: string;
  intake_status: string;
  primary_preset_id: string;
  updated_at: string;
  completion_locked_at?: string;
}
