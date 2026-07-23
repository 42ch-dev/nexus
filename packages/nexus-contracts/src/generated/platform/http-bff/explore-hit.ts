/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Single browse/search result row for Explore read APIs (platform contract; plan 16 slice).
 */
export interface ExploreHit {
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
