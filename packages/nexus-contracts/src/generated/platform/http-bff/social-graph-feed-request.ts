/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for personalized social / activity feed listing (platform plan 17).
 */
export interface SocialGraphFeedRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Opaque pagination cursor
   */
  cursor?: string;
  /**
   * Max feed entries (server may cap)
   */
  limit?: number;
}
