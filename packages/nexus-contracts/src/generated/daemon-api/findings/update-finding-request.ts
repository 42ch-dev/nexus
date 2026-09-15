/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/works/{work_id}/findings/{finding_id}. Every field is optional; `rule_suggestion` is tri-state (R-V1190-FINDINGS-TRISTATE-DUP): omitted does not touch the stored column, `null` clears it to SQL NULL, a string sets it.
 */
export interface UpdateFindingRequest {
  severity?: string;
  status?: string;
  title?: string;
  description?: string;
  target_executor?: string;
  kind?: string;
  rule_suggestion?: unknown;
}
