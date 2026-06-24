import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ActiveWorkspaceResponse
 *
 * Response for GET /v1/local/workspace.
 *
 * @schema_version 1
 * @source active-workspace-response.schema.json
 */
/** Response for GET /v1/local/workspace. */
export interface ActiveWorkspaceResponse {
  creator_id: string;
  workspace_slug: string;
  creative_root?: string;
  operational_dir: string;
}
