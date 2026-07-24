/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Paginated Explore results for browse and search responses (POST /v1/explore/browse | /v1/explore/search).
 */
export interface ExploreFeedResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  entries: NexusExploreHit[];
  /**
   * Opaque cursor for the next page; omit or null when not available
   */
  next_cursor?: string;
  /**
   * True when additional pages may exist
   */
  has_more: boolean;
}
/**
 * Single browse/search result row for Explore read APIs (platform contract; plan 16 slice).
 */
export interface NexusExploreHit {
  /**
   * Discriminator for entity_id interpretation
   */
  hit_type: "world" | "creator" | "manuscript" | "other";
  /**
   * Platform entity id (e.g. wld_*, ctr_*, stm_*)
   */
  entity_id: string;
  /**
   * Primary display label
   */
  title: string;
  /**
   * Secondary line (e.g. persona snippet)
   */
  subtitle?: string;
  /**
   * Effective visibility when exposed by Explore
   */
  visibility?: "private" | "unlisted" | "public";
}
