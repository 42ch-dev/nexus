import type { CommandOrigin, CommandStatus, CommandType, SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SyncCommand
 *
 * SyncCommand entity representing a business action with audit attribution. Aligned with data-model-v1.md §5.10.
 *
 * @schema_version 1
 * @source sync-command.schema.json
 */
/** SyncCommand entity representing a business action with audit attribution. Aligned with data-model-v1.md §5.10. */
export interface SyncCommand {
  schema_version: number;
  command_id: string;
  workspace_id: string;
  world_id: string;
  creator_id: string;
  command_type: CommandType;
  origin: CommandOrigin;
  output_manuscript?: boolean;
  status: CommandStatus;
  requested_by?: string;
  started_at?: string;
  completed_at?: string;
  created_at: string;
}
