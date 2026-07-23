/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Paginated list response for memory web read APIs (platform plan 18). Items are read projections; full MemoryItem sync may use domain bundle types separately.
 */
export interface MemoryWebListResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  items: {
    /**
     * MemoryItem id
     */
    memory_item_id: string;
    /**
     * Creator ID (prefix: 'ctr_')
     */
    creator_id: string;
    /**
     * World ID (prefix: 'wld_')
     */
    world_id: string;
    /**
     * Same values as common MemoryType (inline for TS inline-object codegen imports)
     */
    memory_type: "canon" | "working" | "experience";
    /**
     * Same values as common MemoryKind
     */
    memory_kind?:
      | "story_summary"
      | "research_material"
      | "review_note"
      | "character_note"
      | "world_building"
      | "plot_outline"
      | "theme_analysis"
      | "custom";
    /**
     * Same values as common MemoryStatus
     */
    status: "active" | "superseded" | "archived";
    /**
     * Summary text when exposed
     */
    summary?: string;
    /**
     * ISO 8601 / RFC 3339 UTC datetime string
     */
    created_at: string;
    /**
     * ISO 8601 / RFC 3339 UTC datetime string
     */
    updated_at?: string;
  }[];
  next_cursor?: string;
  has_more: boolean;
}
