/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/explore/browse — read-only directory-style listing.
 */
export interface ExploreBrowseRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Opaque pagination cursor from a prior response
   */
  cursor?: string;
  /**
   * Max entries to return (server may cap)
   */
  limit?: number;
  /**
   * Optional filter for entry kinds
   */
  scope?: "all" | "worlds" | "creators" | "manuscripts";
}
