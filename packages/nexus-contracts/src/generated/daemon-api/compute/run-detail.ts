/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Full detail for GET /v1/daemon/compute/runs/{run_id}. Extends RunSummary with invocation params, full proposals (on succeeded), error (on failed), and accept metadata (on applied).
 */
export interface RunDetail {
  /**
   * Unique run identifier.
   */
  run_id: string;
  /**
   * Run lifecycle status.
   */
  status: "running" | "succeeded" | "failed" | "applied" | "discarded";
  /**
   * Module that was invoked.
   */
  module_id: string;
  /**
   * Version of the module at invocation time.
   */
  module_version: string;
  /**
   * World the run targeted.
   */
  world_id: string;
  /**
   * Timeline branch the run was scoped to (defaults to root when omitted in request).
   */
  branch_id?: string;
  /**
   * Module-specific invocation parameters as provided in the run request.
   */
  invocation_params?: {
    [k: string]: unknown | undefined;
  };
  proposals?: NexusComputeOutputEnvelope;
  error?: NexusErrorResponse;
  /**
   * ISO 8601 UTC timestamp of run creation.
   */
  created_at: string;
  /**
   * ISO 8601 UTC timestamp of last status change.
   */
  updated_at?: string;
  /**
   * ISO 8601 UTC timestamp when accepted. Present only when status is "applied".
   */
  accepted_at?: string;
  /**
   * IDs of the Timeline events created during accept. Present only when status is "applied".
   */
  timeline_event_ids?: string[];
}
/**
 * Full module output proposals. Present only when status is "succeeded".
 */
export interface NexusComputeOutputEnvelope {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Ordered list of +/-/set state operations to apply to computable KnowledgeEntry bodies.
   */
  state_delta: {
    /**
     * Delta operation: add (increment numeric), sub (decrement numeric), set (replace value). Applied to nested state paths on computable KnowledgeEntry bodies (compass Q5, e.g. character.current_hp).
     */
    op: "add" | "sub" | "set";
    /**
     * Dotted state path within the target KnowledgeEntry body (e.g. 'character.current_hp'). Resolution semantics are finalized in P3 (state delta merge).
     */
    path: string;
    /**
     * KnowledgeEntry entry_id the delta applies to (opaque string per spoke knowledge-entry.schema.json). When omitted, the host applies the delta to the entry implied by the capability context.
     */
    target_key_block_id?: string;
    /**
     * Value for set, or numeric delta for add/sub. Untyped (any JSON) to allow module-declared state shapes.
     */
    value?: {
      [k: string]: unknown | undefined;
    };
  }[];
  /**
   * Timeline events to append (V1.60 timeline.event.append). Typically a story_advance or state_update event recording the outcome of the compute step.
   */
  timeline_events: NexusTimelineEvent[];
  /**
   * New KnowledgeEntry records the module creates (e.g. a spawned item, a newly established faction relation). These are upserted by the host.
   */
  new_key_blocks: {
    [k: string]: unknown | undefined;
  }[];
  /**
   * Module-declared freeform report. kind discriminates the payload; remaining fields are module-specific (combat -> casualties, economy -> market_prices). Kept open (additionalProperties: true) per the V1 envelope decision (compass Q8).
   */
  battle_report: {
    /**
     * Discriminator identifying the report shape (e.g. 'combat' for casualties, 'economy' for market_prices). Consumers switch on this to interpret the freeform fields.
     */
    kind?: string;
    [k: string]: unknown | undefined;
  };
}
/**
 * TimelineEvent - a canonical event on the world timeline with causality and sequence. Aligned with data-model-v1.md §5.6.
 */
export interface NexusTimelineEvent {
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
/**
 * Honest error detail when status is "failed".
 */
export interface NexusErrorResponse {
  /**
   * Stable, machine-readable error code. New endpoints use lowercase snake_case `<resource>_<failure>` (e.g. `work_not_found`); the daemon's legacy coarse codes (e.g. `INVALID_INPUT`, `NOT_FOUND`) remain valid.
   */
  code: string;
  /**
   * Human-readable, actionable error message safe for CLI/UI display.
   */
  message: string;
  /**
   * Optional structured context such as IDs, field names, or validation paths. Do not place unstructured stack traces here.
   */
  details?: {
    [k: string]: unknown | undefined;
  };
}
