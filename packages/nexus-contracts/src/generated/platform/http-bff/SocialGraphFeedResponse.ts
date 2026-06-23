import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SocialGraphFeedResponse
 *
 * Paginated personalized feed for social graph (platform plan 17). Entries are activity rows; shape may evolve per v1-spec.
 *
 * @schema_version 1
 * @source social-graph-feed-response.schema.json
 */

/** Inline enum type */
export type SocialGraphFeedResponseEntriesVerb = 'followed' | 'favorited' | 'published' | 'commented' | 'other';

/** Inline enum type */
export type SocialGraphFeedResponseEntriesTargetKind = 'creator' | 'world' | 'manuscript' | 'other';

/** Paginated personalized feed for social graph (platform plan 17). Entries are activity rows; shape may evolve per v1-spec. */
export interface SocialGraphFeedResponse {
  schema_version: number;
  entries: { edge_id: string; actor_creator_id?: string; verb: SocialGraphFeedResponseEntriesVerb; target_entity_id?: string; target_kind?: SocialGraphFeedResponseEntriesTargetKind; title?: string; occurred_at: string }[];
  next_cursor?: string;
  has_more: boolean;
}
