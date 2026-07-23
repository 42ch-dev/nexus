/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * MemoryItem - structured memory for creator experience and world context. Aligned with data-model-v1.md §5.8.
 */
export interface Memory {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Unique MemoryItem identifier (prefix: 'mem_')
   */
  memory_item_id: string;
  /**
   * Creator who owns this memory
   */
  creator_id: string;
  /**
   * World this memory belongs to
   */
  world_id: string;
  /**
   * canon | working | experience
   */
  memory_type: "canon" | "working" | "experience";
  /**
   * Content morphology sub-type (per ADR-001)
   */
  memory_kind?:
    | "story_summary"
    | "research_material"
    | "review_note"
    | "character_note"
    | "world_building"
    | "plot_outline"
    | "theme_analysis"
    | "personality_core"
    | "custom";
  /**
   * MemoryItem status
   */
  status: "active" | "superseded" | "archived";
  /**
   * Memory summary text
   */
  summary?: string;
  /**
   * Reference to vector embedding
   */
  embedding_ref?: string;
  /**
   * Source references for provenance
   */
  source_refs?: {
    /**
     * Source reference kind (e.g., 'command')
     */
    kind: string;
    /**
     * Source entity ID
     */
    id: string;
    [k: string]: unknown | undefined;
  }[];
  /**
   * Last access timestamp (nullable)
   */
  last_accessed_at?: string;
  /**
   * Last reinforcement timestamp (nullable)
   */
  last_reinforced_at?: string;
  /**
   * Memory creation timestamp
   */
  created_at: string;
  /**
   * Last update timestamp
   */
  updated_at?: string;
}
