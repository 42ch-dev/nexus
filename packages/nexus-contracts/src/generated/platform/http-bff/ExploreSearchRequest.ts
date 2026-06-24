import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ExploreSearchRequest
 *
 * Request body for POST /v1/explore/search — read-only full-text style query.
 *
 * @schema_version 1
 * @source explore-search-request.schema.json
 */
/** Request body for POST /v1/explore/search — read-only full-text style query. */
export interface ExploreSearchRequest {
  schema_version: number;
  query: string;
  cursor?: string;
  limit?: number;
}
