/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/findings/batch. Bulk-updates status and/or target_executor for up to 100 findings. Creator-scoped; each individual update reuses the existing update_finding DAO validation.
 */
export interface BatchUpdateFindingsRequest {
  /**
   * IDs of the findings to update. Must be unique; cap enforced at 100; >100 returns HTTP 422 with code `too_many_findings`.
   *
   * @minItems 1
   * @maxItems 100
   */
  finding_ids: [string, ...string[]];
  patch: NexusFindingBatchPatch;
}
/**
 * Fields to patch on each matching finding in a batch update. At least one field should be present.
 */
export interface NexusFindingBatchPatch {
  status?: string;
  target_executor?: string;
}
