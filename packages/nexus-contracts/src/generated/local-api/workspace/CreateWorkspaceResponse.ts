import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreateWorkspaceResponse
 *
 * Response for POST /v1/local/workspaces.
 *
 * @schema_version 1
 * @source create-workspace-response.schema.json
 */
/** Response for POST /v1/local/workspaces. */
export interface CreateWorkspaceResponse {
  creator_id: string;
  workspace_slug: string;
  creative_root: string;
  operational_dir: string;
  state_db_path: string;
}
