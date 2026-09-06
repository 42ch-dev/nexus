/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/daemon/characters/{character_id}/memory/review. Mirrors the Creator review drain contract: counts per classifier action plus the has_more/processed drain signals.
 */
export interface ReviewCharacterMemoryResponse {
  promoted: number;
  fragmented: number;
  dropped: number;
  /**
   * When true the pending queue was not fully drained by this call; the client should re-issue POST review to drain the remainder.
   */
  has_more?: boolean;
  /**
   * Number of pending rows inspected during this call.
   */
  processed?: number;
}
