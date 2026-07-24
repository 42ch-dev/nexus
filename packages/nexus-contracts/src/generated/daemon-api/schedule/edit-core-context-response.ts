/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for PATCH /v1/daemon/orchestration/schedules/{schedule_id}/core-context.
 */
export interface EditCoreContextResponse {
  /**
   * New core context version after edit.
   */
  new_version: number;
}
