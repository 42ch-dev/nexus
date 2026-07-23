/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Standard 4-part output envelope returned by a WASM compute module (V1.61 ABI, compass Q8). Modules emit state deltas to apply, timeline events to append (aligned with V1.60 timeline.event.append), new KeyBlocks to create, and a module-declared freeform report. The host applies these in order: state_delta -> new_key_blocks -> timeline_events, then surfaces battle_report.
 */
export interface ComputeOutput {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Ordered list of +/-/set state operations to apply to computable KeyBlock bodies.
   */
  state_delta: {
    /**
     * Delta operation: add (increment numeric), sub (decrement numeric), set (replace value). Applied to nested state paths on computable KeyBlock bodies (compass Q5, e.g. character.current_hp).
     */
    op: "add" | "sub" | "set";
    /**
     * Dotted state path within the target KeyBlock body (e.g. 'character.current_hp'). Resolution semantics are finalized in P3 (state delta merge).
     */
    path: string;
    /**
     * KeyBlock the delta applies to. When omitted, the host applies the delta to the KeyBlock implied by the capability context.
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
   * New KeyBlocks the module creates (e.g. a spawned item, a newly established faction relation). These are upserted by the host.
   */
  new_key_blocks: NexusKeyBlock[];
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
   * KeyBlocks affected by this event
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
 * KeyBlock - a structured knowledge unit in a world timeline. Aligned with data-model-v1.md §5.5.
 */
export interface NexusKeyBlock {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * KeyBlock ID (prefix: 'kb_')
   */
  key_block_id: string;
  /**
   * World this KB belongs to
   */
  world_id: string;
  /**
   * KeyBlock content type
   */
  block_type:
    | "character"
    | "ability"
    | "scene"
    | "organization"
    | "item"
    | "conflict"
    | "info_point"
    | "event"
    | "species"
    | "faction"
    | "magic_system"
    | "technology"
    | "deity"
    | "level"
    | "economy_tier"
    | "dialogue"
    | "beat"
    | "act"
    | "era";
  /**
   * Canonical name for this KeyBlock
   */
  canonical_name: string;
  /**
   * KeyBlock status
   */
  status: "provisional" | "confirmed" | "deprecated" | "merged" | "deleted";
  /**
   * KeyBlock revision number
   */
  revision?: number;
  /**
   * KeyBlock body content. V1.61 added state (dynamic compute state) and computable (compute participation flag); both optional and additive-only, so existing KeyBlocks without them remain valid.
   */
  body?: {
    /**
     * Structured summary
     */
    summary?: string;
    /**
     * Key-value attributes. For computable KeyBlocks these hold IMMUTABLE compute params; per-module shape is declared in the module's manifest.json `schemas.key_block_attributes[<block_type>]` block (V1.62). Keys are nested by block_type.
     */
    attributes?: {
      [k: string]: unknown | undefined;
    };
    /**
     * Classification tags
     */
    tags?: string[];
    /**
     * DYNAMIC runtime state for computable KeyBlocks (V1.61, compass Q4/Q5). Nested by block_type to avoid field-name collisions across module types (e.g. state.character.current_hp). Per-module state shape is declared in the module's manifest.json `schemas.key_block_state[<block_type>]` block (V1.62). Only meaningful when computable is true.
     */
    state?: {
      [k: string]: unknown | undefined;
    };
    /**
     * Marks this KeyBlock as participating in WASM compute (V1.61, compass Q4). When true, body.state holds mutable runtime state and body.attributes hold immutable compute params. Stored inside body_json (no DB column) for additive, migration-free rollout.
     */
    computable?: boolean;
    [k: string]: unknown | undefined;
  };
  source_anchor?: NexusSourceAnchor;
  /**
   * SyncCommand that created this KB
   */
  created_from_command_id?: string;
  /**
   * KB creation timestamp
   */
  created_at: string;
  /**
   * Last update timestamp
   */
  updated_at?: string;
}
/**
 * Source anchor for provenance
 */
export interface NexusSourceAnchor {
  /**
   * References to platform Story summary entities
   */
  story_summary_refs?: {
    /**
     * StoryManifest ID
     */
    story_manifest_id: string;
    /**
     * Summary unit ID
     */
    summary_unit_id: string;
    /**
     * Unit kind (e.g., 'chapter_summary')
     */
    unit_kind?: string;
    [k: string]: unknown | undefined;
  }[];
  /**
   * Optional excerpt text
   */
  excerpt?: string;
  /**
   * Optional anchor summary
   */
  summary?: string;
}
