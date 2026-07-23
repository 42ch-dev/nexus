/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/works/{work_id}/findings/{finding_id}.
 */
export interface UpdateFindingRequest {
  severity?: string;
  status?: string;
  title?: string;
  description?: string;
  target_executor?: string;
  kind?: string;
  rule_suggestion?: string;
}
