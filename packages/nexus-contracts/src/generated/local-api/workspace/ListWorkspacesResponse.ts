import type { PaginationInfo } from '../kb/PaginationInfo';
import type { WorkspaceSummary } from './WorkspaceSummary';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListWorkspacesResponse
 *
 * Response for GET /v1/local/workspaces.
 *
 * @schema_version 1
 * @source list-workspaces-response.schema.json
 */
/** Response for GET /v1/local/workspaces. */
export interface ListWorkspacesResponse {
  items: WorkspaceSummary[];
  pagination: PaginationInfo;
}
