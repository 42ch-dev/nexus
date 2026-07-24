/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Public creator projection for Explore / creator-profile read APIs (platform plan 16 / W3 slice). Field tiers follow v1-spec visibility; omit sensitive fields at the edge.
 */
export interface ExploreCreatorCard {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Creator ID (prefix: 'ctr_')
   */
  creator_id: string;
  /**
   * Public display name
   */
  display_name: string;
  /**
   * Public bio text when visibility allows
   */
  bio?: string;
  /**
   * HTTPS URL to avatar when exposed
   */
  avatar_url?: string;
  /**
   * Denormalized follower count when exposed
   */
  follower_count?: number;
  /**
   * Whether this is a platform-hosted creator (drives 'Official' badge)
   */
  is_platform_owned?: boolean;
  /**
   * Creator registration timestamp ('Member since' display)
   */
  created_at?: string;
  /**
   * Count of public active worlds owned by this creator
   */
  public_world_count?: number;
  /**
   * Effective visibility of this card
   */
  visibility?: "private" | "unlisted" | "public";
}
