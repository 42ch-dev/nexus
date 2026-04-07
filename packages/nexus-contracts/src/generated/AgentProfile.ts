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
export type AgentProfileProfileKind = 'primary' | 'specialist' | 'fallback';

/** Inline enum type */
export type AgentProfileSelectionMode = 'explicit' | 'registry' | 'auto';

/** Inline enum type */
export type AgentProfileTransport = 'local' | 'http' | 'stdio';

/** Inline enum type */
export type AgentProfileStatus = 'active' | 'inactive' | 'error';

/** Configuration for an ACP agent in a workspace. Aligned with data-model-v1.md §5.15. */
export interface AgentProfile {
  schema_version: number;
  agent_profile_id: string;
  workspace_id: string;
  profile_kind: AgentProfileProfileKind;
  selection_mode: AgentProfileSelectionMode;
  registry_agent_id?: string;
  transport?: AgentProfileTransport;
  default_output_manuscript?: string;
  protocol_version?: string;
  status: AgentProfileStatus;
  created_at?: string;
  updated_at?: string;
}
