/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for Explore AI summarization (platform plan 19).
 */
export interface ExploreAiSummaryResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Generated summary text
   */
  summary: string;
  /**
   * Optional model identifier
   */
  model?: string;
}
