/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for social graph mutations: follow / unfollow / favorite / unfavorite (platform plan 17).
 */
export interface SocialGraphRelationshipRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Mutation to apply relative to the authenticated creator
   */
  action: "follow" | "unfollow" | "favorite" | "unfavorite";
  /**
   * Other creator the edge refers to
   */
  target_creator_id: string;
  /**
   * Optional collection scope when action involves curated lists (platform extension)
   */
  collection_id?: string;
}
