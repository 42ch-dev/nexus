import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus SyncCommand
 *
 * SyncCommand entity representing a business action with audit attribution. Aligned with data-model-v1.md §5.10.
 *
 * @schema_version 1
 * @source sync-command.schema.json
 */

/** Inline enum type */
export type SyncCommandCommandType = 'advance_world' | 'inject_future_event' | 'extract_kb' | 'sync_push' | 'sync_pull' | 'fork_world' | 'publish_story';

/** Inline enum type */
export type SyncCommandOrigin = 'local_user' | 'local_agent' | 'official_creator' | 'system';

/** Inline enum type */
export type SyncCommandStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

/** SyncCommand entity representing a business action with audit attribution. Aligned with data-model-v1.md §5.10. */
export interface SyncCommand {
  schema_version: number;
  command_id: string;
  workspace_id: string;
  world_id: string;
  creator_id: string;
  command_type: SyncCommandCommandType;
  origin: SyncCommandOrigin;
  output_manuscript?: boolean;
  status: SyncCommandStatus;
  requested_by?: string;
  started_at?: string;
  completed_at?: string;
  created_at: string;
}
