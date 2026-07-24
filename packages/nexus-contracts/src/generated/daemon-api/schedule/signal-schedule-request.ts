/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/orchestration/schedules/{schedule_id}/signal.
 */
export interface SignalScheduleRequest {
  /**
   * Signal action: pause, resume, cancel.
   */
  signal: string;
}
