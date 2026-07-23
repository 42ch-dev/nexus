/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * KeyBlock - a structured knowledge unit in a world timeline. Aligned with data-model-v1.md §5.5.
 */
export interface KeyBlock {
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
