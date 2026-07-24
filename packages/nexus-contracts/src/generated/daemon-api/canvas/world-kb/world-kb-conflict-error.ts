/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Structured detail placed inside the canonical ErrorResponse.details field when a World KB patch is rejected because expected_version is stale (HTTP 409). Per-row OCC on kb_key_blocks.revision / kb_extract_jobs.version (V1.73).
 */
export interface WorldKbConflictError {
  /**
   * Current canonical per-row version of the entity or candidate at conflict time.
   */
  current_version: number;
  /**
   * Identifier of the key_block or candidate involved in the conflict.
   */
  entity_id: string;
  /**
   * Dot-path or locator describing the row/version that changed underneath the client.
   */
  conflicting_path: string;
  /**
   * Actionable hint for the client (e.g. 'refetch the graph and reapply').
   */
  recovery_hint: string;
}
