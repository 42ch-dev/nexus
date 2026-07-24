/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * GET /creators/:id/runtime-policy 200 response body. Exposes Creator-level policy capabilities for CLI consumption. SSOT: v1-spec platform/local-first-runtime-policy-v1.md §4, §7.
 */
export interface CreatorRuntimePolicyResponse {
  /**
   * Wire revision; fixed to 1
   */
  schema_version: 1;
  /**
   * Creator identifier
   */
  creator_id: string;
  /**
   * Whether memory structured write is enabled for this Creator (spec §4)
   */
  memory_structured_write: boolean;
  /**
   * Whether memory vector indexing/embedding is enabled (spec §4)
   */
  memory_vector_index: boolean;
  /**
   * Remaining embedding quota for local_first creators (null if not applicable or unlimited)
   */
  local_first_embedding_remaining?: number;
}
