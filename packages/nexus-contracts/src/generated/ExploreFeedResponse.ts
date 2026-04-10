import type { ExploreHit } from './ExploreHit';
import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus ExploreFeedResponse
 *
 * Paginated Explore results for browse and search responses (POST /v1/explore/browse | /v1/explore/search).
 *
 * @schema_version 1
 * @source explore-feed-response.schema.json
 */
/** Paginated Explore results for browse and search responses (POST /v1/explore/browse | /v1/explore/search). */
export interface ExploreFeedResponse {
  schema_version: number;
  entries: ExploreHit[];
  next_cursor?: string;
  has_more: boolean;
}
