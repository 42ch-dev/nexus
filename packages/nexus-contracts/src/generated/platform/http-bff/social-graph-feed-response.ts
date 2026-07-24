/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Paginated personalized feed for social graph (platform plan 17). Entries are activity rows; shape may evolve per v1-spec.
 */
export interface SocialGraphFeedResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Feed rows newest-first per platform policy
   */
  entries: {
    /**
     * Stable id for this feed row / dedupe
     */
    edge_id: string;
    /**
     * Creator ID (prefix: 'ctr_')
     */
    actor_creator_id?: string;
    /**
     * Activity verb for display
     */
    verb: "followed" | "favorited" | "published" | "commented" | "other";
    /**
     * Target entity id (world, manuscript, etc.)
     */
    target_entity_id?: string;
    /**
     * Interpretation of target_entity_id
     */
    target_kind?: "creator" | "world" | "manuscript" | "other";
    /**
     * Human-readable one-line summary
     */
    title?: string;
    /**
     * ISO 8601 / RFC 3339 UTC datetime string
     */
    occurred_at: string;
  }[];
  /**
   * Cursor for the next page when has_more is true
   */
  next_cursor?: string;
  has_more: boolean;
}
