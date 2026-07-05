import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SetActiveWorkspaceResponse
 *
 * Response for POST /v1/daemon/workspace/active.
 *
 * @schema_version 1
 * @source set-active-workspace-response.schema.json
 */
/** Response for POST /v1/daemon/workspace/active. */
export interface SetActiveWorkspaceResponse {
  creator_id: string;
  workspace_slug: string;
}
