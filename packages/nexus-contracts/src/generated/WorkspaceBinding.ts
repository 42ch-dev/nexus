import type { SchemaVersion } from './CommonTypes';
/**
 * WorkspaceBinding
 *
 * Binding between a local workspace and a remote world. Aligned with data-model-v1.md §5.14.
 *
 * @schema_version 1
 * @source workspace-binding.schema.json
 */

/** Inline enum type */
export type WorkspaceBindingBindingStatus = 'active' | 'unlinked' | 'stale';

/** Binding between a local workspace and a remote world. Aligned with data-model-v1.md §5.14. */
export interface WorkspaceBinding {
  schema_version: number;
  workspace_id: string;
  local_root: string;
  profile_name?: string;
  world_id: string;
  creator_id: string;
  binding_status: WorkspaceBindingBindingStatus;
  created_at: string;
  updated_at?: string;
}
