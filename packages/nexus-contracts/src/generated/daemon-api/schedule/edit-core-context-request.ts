/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/orchestration/schedules/{schedule_id}/core-context.
 */
export interface EditCoreContextRequest {
  /**
   * Edit operation: append, replace, struct_merge, struct_remove.
   */
  op: string;
  /**
   * Body text for append/replace operations.
   */
  body?: string;
  /**
   * JSON patch for struct_merge operation.
   */
  patch?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Key path for struct_remove operation.
   */
  path?: string;
}
