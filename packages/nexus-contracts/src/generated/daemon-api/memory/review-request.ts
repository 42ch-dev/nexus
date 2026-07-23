/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/memory/review. Triggers the review/summarization pipeline for the active creator's entire pending queue. `creator_id` must match the active creator (config.toml), otherwise 403.
 */
export interface ReviewRequest {
  creator_id: string;
}
