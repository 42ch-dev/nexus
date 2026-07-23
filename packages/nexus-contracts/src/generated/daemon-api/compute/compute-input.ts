/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Standard input envelope passed into a WASM compute module (V1.61 ABI, compass Q3/Q8). Bundles a read-only KeyBlock snapshot, the narrative position, and module-declared invocation parameters. Modules are stateless pure functions (compass Q6): every call receives a fresh envelope and returns a ComputeOutput.
 */
export interface ComputeInput {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * World and timeline locator for this invocation
   */
  world_ref: {
    /**
     * World the compute invocation runs against
     */
    world_id?: string;
    /**
     * Fork branch ID (root branch or a specific fork)
     */
    branch_id?: string;
    /**
     * Current timeline head the compute advances from
     */
    timeline_head_event_id?: string;
    [k: string]: unknown | undefined;
  };
  /**
   * Snapshot of KeyBlocks relevant to this invocation. Each KeyBlock carries its body including state (for computable blocks) and attributes (immutable compute params). The host selects which blocks to pass based on the module manifest and the capability context.
   */
  key_blocks: NexusKeyBlock[];
  /**
   * Narrative position context (timeline, chapter, scene). Shape is module-declared; fields not listed here may be supplied by the host per the module manifest.
   */
  narrative_state?: {
    /**
     * Opaque timeline position label (module-interpreted)
     */
    timeline_position?: string;
    /**
     * Current chapter identifier, if applicable
     */
    current_chapter?: string;
    /**
     * Current scene identifier, if applicable
     */
    current_scene?: string;
    [k: string]: unknown | undefined;
  };
  /**
   * Module-defined input parameters for this invocation (freeform object). The exact fields are declared by the module's manifest.json; the host passes them through verbatim. This is the V1 envelope escape hatch for module-specific inputs (e.g. chosen targets, difficulty, dice seed).
   */
  invocation?: {
    [k: string]: unknown | undefined;
  };
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
   * Unique KeyBlock identifier
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
