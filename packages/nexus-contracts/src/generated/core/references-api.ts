/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Handler-local wire DTOs for the reference-source registry read surface (`references-api` destination). The cloud-backed refresh stays the CLI's explicit external call and is deliberately not owned here.
 */
export type ReferencesApi = ReferenceListResponse;

/**
 * Response for `GET /v1/daemon/references`.
 */
export interface ReferenceListResponse {
  references: ReferenceSourceInfo[];
}
/**
 * Registry metadata for one reference source (legacy `reference` envelope; registry reads are global across creators in the local-first single-creator model).
 */
export interface ReferenceSourceInfo {
  reference_source_id: string;
  source_type: string;
  source_mutability: string;
  uri: string;
  title: string;
  /**
   * Workspace-relative content path when the source materialised locally.
   */
  content_path?: string;
  scan_status: string;
  created_at: string;
}
