/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * SyncCommand entity representing a business action with audit attribution. Aligned with data-model-v1.md §5.10.
 */
export interface SyncCommand {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * SyncCommand ID (prefix: 'cmd_')
   */
  command_id: string;
  /**
   * Workspace ID (prefix: 'wrk_')
   */
  workspace_id: string;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Creator ID (prefix: 'ctr_')
   */
  creator_id: string;
  /**
   * Normalized business action type
   */
  command_type:
    "advance_world" | "inject_future_event" | "extract_kb" | "sync_push" | "sync_pull" | "fork_world" | "publish_story";
  /**
   * Command origin
   */
  origin: "local_user" | "local_agent" | "official_creator" | "system";
  /**
   * Whether this execution requires manuscript output
   */
  output_manuscript?: boolean;
  /**
   * Command execution status
   */
  status: "pending" | "running" | "completed" | "failed" | "cancelled";
  /**
   * User who requested the command
   */
  requested_by?: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  started_at?: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  completed_at?: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
}
