/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/orchestration/schedules.
 */
export interface AddScheduleResponse {
  /**
   * Created schedule ID.
   */
  schedule_id: string;
  /**
   * Initial schedule status.
   */
  status: string;
  /**
   * Initial core context version.
   */
  core_context_version: number;
}
