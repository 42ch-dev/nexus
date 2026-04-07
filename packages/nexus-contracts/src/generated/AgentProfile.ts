import type { SchemaVersion } from './CommonTypes';
/**
 * AgentProfile
 *
 * Configuration for an ACP agent in a workspace. Aligned with data-model-v1.md §5.15.
 *
 * @schema_version 1
 * @source agent-profile.schema.json
 */

/** Inline enum type */
export type AgentProfileProfileKind = 'local_agent' | 'platform_hosted';

/** Inline enum type */
export type AgentProfileSelectionMode = 'registry' | 'manual_command' | 'manual_remote';

/** Inline enum type */
export type AgentProfileTransport = 'stdio' | 'http' | 'websocket';

/** Inline enum type */
export type AgentProfileStatus = 'active' | 'unavailable' | 'deprecated';

/** Configuration for an ACP agent in a workspace. Aligned with data-model-v1.md §5.15. */
export interface AgentProfile {
  schema_version: number;
  agent_profile_id: string;
  workspace_id: string;
  profile_kind: AgentProfileProfileKind;
  selection_mode: AgentProfileSelectionMode;
  registry_agent_id?: string;
  launch_command?: string | null;
  transport?: AgentProfileTransport;
  default_output_manuscript?: boolean;
  protocol_version?: number;
  status: AgentProfileStatus;
  created_at?: string;
  updated_at?: string;
}
