import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListFindingsQuery
 *
 * Query parameters for GET /v1/local/works/{work_id}/findings.
 *
 * @schema_version 1
 * @source list-findings-query.schema.json
 */
/** Query parameters for GET /v1/local/works/{work_id}/findings. */
export interface ListFindingsQuery {
  chapter?: number;
  status?: string;
  severity?: string;
  limit?: number;
  offset?: number;
}
