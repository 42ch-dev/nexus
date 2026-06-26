import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListWorksQuery
 *
 * Query parameters for GET /v1/local/works (cursor-based pagination + sort, F-P1 / F-F1).
 *
 * @schema_version 2
 * @source list-works-query.schema.json
 */
/** Query parameters for GET /v1/local/works (cursor-based pagination + sort, F-P1 / F-F1). */
export interface ListWorksQuery {
  status?: string;
  intake_status?: string;
  limit?: number;
  cursor?: string;
  sort?: string;
}
