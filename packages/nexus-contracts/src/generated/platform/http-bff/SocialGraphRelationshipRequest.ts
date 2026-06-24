import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SocialGraphRelationshipRequest
 *
 * Request body for social graph mutations: follow / unfollow / favorite / unfavorite (platform plan 17).
 *
 * @schema_version 1
 * @source social-graph-relationship-request.schema.json
 */

/** Inline enum type */
export type SocialGraphRelationshipRequestAction = 'follow' | 'unfollow' | 'favorite' | 'unfavorite';

/** Request body for social graph mutations: follow / unfollow / favorite / unfavorite (platform plan 17). */
export interface SocialGraphRelationshipRequest {
  schema_version: number;
  action: SocialGraphRelationshipRequestAction;
  target_creator_id: string;
  collection_id?: string;
}
