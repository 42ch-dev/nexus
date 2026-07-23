/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/publish/history — paginated publish history with optional filters (platform API).
 */
export interface PublishHistoryRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Optional world scope filter
   */
  world_id?: string;
  /**
   * Optional manuscript scope; platform may omit
   */
  manuscript_id?: string;
  /**
   * Optional filter by published artifact kind
   */
  artifact_type?: "chapter" | "story";
  /**
   * Opaque pagination cursor from a prior response
   */
  cursor?: string;
  /**
   * Max entries to return (server may cap)
   */
  limit?: number;
}
