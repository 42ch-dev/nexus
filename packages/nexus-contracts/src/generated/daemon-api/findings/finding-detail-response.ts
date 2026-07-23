/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/works/{work_id}/findings/{finding_id} and create/update responses.
 */
export interface FindingDetailResponse {
  finding_id: string;
  work_id: string;
  chapter?: number;
  severity: string;
  status: string;
  title: string;
  description: string;
  target_executor: string;
  kind: string;
  rule_suggestion?: string;
  created_at: number;
  updated_at: number;
  routing_hint?: string;
}
