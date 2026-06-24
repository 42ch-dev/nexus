import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus WorkspaceSummary
 *
 * Summary row for a workspace in list responses.
 *
 * @schema_version 1
 * @source workspace-summary.schema.json
 */
/** Summary row for a workspace in list responses. */
export interface WorkspaceSummary {
  creator_id: string;
  workspace_slug: string;
  creative_root: string;
  display_name?: string;
}
