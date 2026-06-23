import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ExploreBrowseRequest
 *
 * Request body for POST /v1/explore/browse — read-only directory-style listing.
 *
 * @schema_version 1
 * @source explore-browse-request.schema.json
 */

/** Inline enum type */
export type ExploreBrowseRequestScope = 'all' | 'worlds' | 'creators' | 'manuscripts';

/** Request body for POST /v1/explore/browse — read-only directory-style listing. */
export interface ExploreBrowseRequest {
  schema_version: number;
  cursor?: string;
  limit?: number;
  scope?: ExploreBrowseRequestScope;
}
