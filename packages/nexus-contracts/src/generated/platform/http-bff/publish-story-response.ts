/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/publish/story.
 */
export interface PublishStoryResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Publish result
   */
  outcome: "submitted" | "published" | "rejected" | "invalid_state";
  /**
   * Human-readable detail (validation errors, server notes)
   */
  message?: string;
  /**
   * Platform artifact id when published
   */
  published_artifact_id?: string;
  /**
   * Stable machine code when outcome is rejected or invalid_state
   */
  error_code?: string;
}
