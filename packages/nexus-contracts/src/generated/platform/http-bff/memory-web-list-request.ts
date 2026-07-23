/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for memory web read — list / filter MemoryItem rows for a world (platform plan 18). Aligns with domain memory.schema.json field semantics.
 */
export interface MemoryWebListRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Opaque pagination cursor
   */
  cursor?: string;
  limit?: number;
  /**
   * Filter by MemoryItem.memory_type
   */
  memory_types?: ("canon" | "working" | "experience")[];
  /**
   * Filter by MemoryItem.memory_kind when set
   */
  memory_kinds?: (
    | "story_summary"
    | "research_material"
    | "review_note"
    | "character_note"
    | "world_building"
    | "plot_outline"
    | "theme_analysis"
    | "personality_core"
    | "custom"
  )[];
  /**
   * Filter by MemoryItem.status
   */
  statuses?: ("active" | "superseded" | "archived")[];
}
