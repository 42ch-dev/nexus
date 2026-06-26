import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ListCapabilitiesQuery
 *
 * Query parameters for GET /v1/local/orchestration/capabilities (cursor-based pagination + sort, F-F1).
 *
 * @schema_version 2
 * @source list-capabilities-query.schema.json
 */
/** Query parameters for GET /v1/local/orchestration/capabilities (cursor-based pagination + sort, F-F1). */
export interface ListCapabilitiesQuery {
  limit?: number;
  cursor?: string;
  sort?: string;
}
