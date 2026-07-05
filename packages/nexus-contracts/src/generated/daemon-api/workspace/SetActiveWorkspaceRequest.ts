import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SetActiveWorkspaceRequest
 *
 * Request body for POST /v1/daemon/workspace/active.
 *
 * @schema_version 1
 * @source set-active-workspace-request.schema.json
 */
/** Request body for POST /v1/daemon/workspace/active. */
export interface SetActiveWorkspaceRequest {
  creator_id?: string;
  workspace_slug: string;
}
