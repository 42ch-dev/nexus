import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListWorkspacesQuery
 *
 * Query parameters for GET /v1/local/workspaces.
 *
 * @schema_version 1
 * @source list-workspaces-query.schema.json
 */
/** Query parameters for GET /v1/local/workspaces. */
export interface ListWorkspacesQuery {
  creator_id?: string;
  limit?: number;
  cursor?: string;
}
