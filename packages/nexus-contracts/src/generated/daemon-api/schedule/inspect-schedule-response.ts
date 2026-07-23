/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/orchestration/schedules/{schedule_id}.
 */
export interface InspectScheduleResponse {
  schedule: NexusScheduleSummary;
  /**
   * Schedule IDs this schedule depends on.
   */
  depends_on: string[];
  /**
   * Concurrency mode description string.
   */
  concurrency_kind: string;
}
/**
 * The full schedule summary.
 */
export interface NexusScheduleSummary {
  /**
   * Unique schedule identifier.
   */
  schedule_id: string;
  /**
   * Owning creator ID.
   */
  creator_id: string;
  /**
   * Preset ID this schedule runs.
   */
  preset_id: string;
  /**
   * Current schedule status.
   */
  status: string;
  /**
   * Human-readable label.
   */
  label?: string;
  /**
   * Current core context version number.
   */
  current_core_context_version: number;
  /**
   * ISO-8601 creation timestamp.
   */
  created_at: string;
  /**
   * ISO-8601 last-update timestamp.
   */
  updated_at: string;
}
