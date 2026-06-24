import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListWorksQuery
 *
 * Query parameters for GET /v1/local/works (cursor-based pagination, F-P1).
 *
 * @schema_version 1
 * @source list-works-query.schema.json
 */
/** Query parameters for GET /v1/local/works (cursor-based pagination, F-P1). */
export interface ListWorksQuery {
  status?: string;
  intake_status?: string;
  limit?: number;
  cursor?: string;
}
