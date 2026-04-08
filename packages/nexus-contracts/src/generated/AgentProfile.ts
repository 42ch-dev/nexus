import type { AgentProfileStatus, ProfileKind, SchemaVersion, SelectionMode, Transport } from './CommonTypes';
/**
 * AgentProfile
 *
 * Configuration for an ACP agent in a workspace. Aligned with data-model-v1.md §5.15.
 *
 * @schema_version 1
 * @source agent-profile.schema.json
 */
/** Configuration for an ACP agent in a workspace. Aligned with data-model-v1.md §5.15. */
export interface AgentProfile {
  schema_version: number;
  agent_profile_id: string;
  workspace_id: string;
  profile_kind: ProfileKind;
  selection_mode: SelectionMode;
  registry_agent_id?: string;
  launch_command?: string | null;
  transport?: Transport;
  default_output_manuscript?: boolean;
  protocol_version?: number;
  status: AgentProfileStatus;
  created_at?: string;
  updated_at?: string;
}
