/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/orchestration/schedules/{schedule_id}/core-context.
 */
export interface CoreContextResponse {
  /**
   * Core context version number.
   */
  version: number;
  /**
   * Payload type (text or struct).
   */
  payload_kind: string;
  /**
   * Core context content (text or structured JSON).
   */
  content: {
    [k: string]: unknown | undefined;
  };
  /**
   * How this version was derived (seed, user_edit, preset_hook, llm_summarize, preset_seed_expansion).
   */
  derivation_kind: string;
  /**
   * ISO-8601 creation timestamp.
   */
  created_at: string;
}
