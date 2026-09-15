/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for `GET /v1/daemon/references/{reference_id}`; 404 for an unknown id.
 */
export interface ReferenceGetResponse {
  reference: ReferenceSourceInfo;
}
/**
 * Registry metadata for one reference source (registry reads are global across creators in the local-first single-creator model).
 */
export interface ReferenceSourceInfo {
  reference_source_id: string;
  source_type: string;
  source_mutability: string;
  uri: string;
  title: string;
  content_path?: string;
  scan_status: string;
  created_at: string;
}
