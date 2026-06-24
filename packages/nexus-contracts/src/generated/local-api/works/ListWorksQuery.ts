import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListWorksQuery
 *
 * Query parameters for GET /v1/local/works.
 *
 * @schema_version 1
 * @source list-works-query.schema.json
 */
/** Query parameters for GET /v1/local/works. */
export interface ListWorksQuery {
  status?: string;
  intake_status?: string;
  limit?: number;
  offset?: number;
}
