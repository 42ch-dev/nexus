/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Single atomic change to an entity in a manuscript world. Aligned with data-model-v1.md §5.12.
 */
export interface Delta {
  /**
   * Target aggregate type for this delta
   */
  delta_type: "world" | "key_block" | "timeline_event" | "fork_branch" | "memory_item" | "story_manifest";
  /**
   * Operation to apply
   */
  operation: "create" | "update" | "upsert" | "delete" | "append";
  /**
   * Sub-type (e.g., 'character' when delta_type='key_block')
   */
  target_entity_type?: string;
  /**
   * Target entity ID (null for create)
   */
  target_entity_id?: string;
  /**
   * Delta payload (validated by per-type sub-schema)
   */
  payload: {
    [k: string]: unknown | undefined;
  };
  source_anchor?: NexusSourceAnchor;
  /**
   * Local timestamp of this delta (RFC 3339 UTC)
   */
  local_timestamp: string;
}
/**
 * Optional source anchor for provenance
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
