/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for Explore AI Q&A with optional citations envelope (platform plan 19).
 */
export interface ExploreAiAnswerResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Model answer text
   */
  answer: string;
  /**
   * Grounding citations when the platform returns them
   */
  citations?: {
    /**
     * Citation title or source label
     */
    title: string;
    /**
     * Quoted or summarized excerpt
     */
    snippet?: string;
    /**
     * Opaque source key or stable URI fragment for drill-down
     */
    source_ref?: string;
    /**
     * Related entity id when citation maps to a Nexus object
     */
    entity_id?: string;
  }[];
  /**
   * Optional model identifier for debugging / compliance
   */
  model?: string;
}
