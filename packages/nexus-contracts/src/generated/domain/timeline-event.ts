/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * TimelineEvent - a canonical event on the world timeline with causality and sequence. Aligned with data-model-v1.md §5.6.
 */
export interface TimelineEvent {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Unique TimelineEvent identifier
   */
  timeline_event_id: string;
  /**
   * World this event belongs to
   */
  world_id: string;
  /**
   * Fork branch ID (root branch or specific fork)
   */
  branch_id: string;
  /**
   * Type of timeline event
   */
  event_type: "story_advance" | "state_update" | "fork_marker" | "official_progression" | "publish_marker";
  /**
   * Event status
   */
  status: "canon" | "provisional" | "rejected";
  /**
   * Sequence number within the branch
   */
  sequence_no: number;
  /**
   * Event title
   */
  title?: string;
  /**
   * Event summary
   */
  summary?: string;
  /**
   * Preceding events that caused this one
   */
  caused_by_event_ids?: string[];
  /**
   * Knowledge entries affected by this event
   */
  affected_key_block_ids?: string[];
  /**
   * SyncCommand that triggered this event
   */
  source_command_id?: string;
  /**
   * Event creation timestamp
   */
  created_at: string;
}
