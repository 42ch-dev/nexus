import type { SchemaVersion, Visibility } from '../../common/CommonTypes';
/**
 * Nexus ExploreHit
 *
 * Single browse/search result row for Explore read APIs (platform contract; plan 16 slice).
 *
 * @schema_version 1
 * @source explore-hit.schema.json
 */

/** Inline enum type */
export type ExploreHitHitType = 'world' | 'creator' | 'manuscript' | 'other';

/** Single browse/search result row for Explore read APIs (platform contract; plan 16 slice). */
export interface ExploreHit {
  hit_type: ExploreHitHitType;
  entity_id: string;
  title: string;
  subtitle?: string;
  visibility?: Visibility;
}
