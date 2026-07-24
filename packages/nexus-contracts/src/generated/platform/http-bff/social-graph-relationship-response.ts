/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response envelope for social graph mutation endpoints (platform plan 17).
 */
export interface SocialGraphRelationshipResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  success: boolean;
  /**
   * Updated follow state when applicable
   */
  following?: boolean;
  /**
   * Updated favorite state when applicable
   */
  favorited?: boolean;
  /**
   * Machine- or human-readable error when success is false
   */
  error?: string;
}
