/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for Explore AI summarization over a world or manuscript (platform plan 19).
 */
export interface ExploreAiSummaryRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Whether entity_id is a world or manuscript
   */
  scope: "world" | "manuscript";
  /**
   * Target id (e.g. wld_*, stm_*) — validated per scope on the platform
   */
  entity_id: string;
  /**
   * Soft cap on summary length in characters
   */
  max_length?: number;
}
