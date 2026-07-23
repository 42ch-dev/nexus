/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Fields to patch on each matching finding in a batch update. At least one field should be present.
 */
export interface FindingBatchPatch {
  status?: string;
  target_executor?: string;
}
