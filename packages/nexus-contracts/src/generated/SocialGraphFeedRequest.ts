import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus SocialGraphFeedRequest
 *
 * Request body for personalized social / activity feed listing (platform plan 17).
 *
 * @schema_version 1
 * @source social-graph-feed-request.schema.json
 */
/** Request body for personalized social / activity feed listing (platform plan 17). */
export interface SocialGraphFeedRequest {
  schema_version: number;
  cursor?: string;
  limit?: number;
}
