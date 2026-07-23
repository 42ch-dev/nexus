/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for Explore AI grounded Q&A over world / corpus context (platform plan 19). Boundary with context assembly: this is platform-side retrieval + generation; wire shape only.
 */
export interface ExploreAiAnswerRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * User question
   */
  query: string;
  /**
   * Scope answers to this world when set
   */
  world_id?: string;
  /**
   * Soft cap on citation objects returned
   */
  max_citations?: number;
}
