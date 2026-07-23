/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/explore/search — read-only full-text style query.
 */
export interface ExploreSearchRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Search query string
   */
  query: string;
  /**
   * Opaque pagination cursor from a prior response
   */
  cursor?: string;
  /**
   * Max hits to return (server may cap)
   */
  limit?: number;
}
