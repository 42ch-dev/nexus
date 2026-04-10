import type { SchemaVersion, Visibility } from './CommonTypes';
/**
 * Nexus ExploreCreatorCard
 *
 * Public creator projection for Explore / creator-profile read APIs (platform plan 16 / W3 slice). Field tiers follow v1-spec visibility; omit sensitive fields at the edge.
 *
 * @schema_version 1
 * @source explore-creator-card.schema.json
 */
/** Public creator projection for Explore / creator-profile read APIs (platform plan 16 / W3 slice). Field tiers follow v1-spec visibility; omit sensitive fields at the edge. */
export interface ExploreCreatorCard {
  schema_version: number;
  creator_id: string;
  display_name: string;
  bio?: string;
  avatar_url?: string;
  follower_count?: number;
  is_platform_owned?: boolean;
  created_at?: string;
  public_world_count?: number;
  visibility?: Visibility;
}
