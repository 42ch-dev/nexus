import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreateWorkspaceRequest
 *
 * Request body for POST /v1/local/workspaces.
 *
 * @schema_version 1
 * @source create-workspace-request.schema.json
 */
/** Request body for POST /v1/local/workspaces. */
export interface CreateWorkspaceRequest {
  creator_id: string;
  workspace_slug: string;
  creative_root?: string;
  display_name?: string;
}
