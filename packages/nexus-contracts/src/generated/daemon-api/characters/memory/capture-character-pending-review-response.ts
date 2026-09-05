/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/daemon/characters/{character_id}/memory/pending-review.
 */
export interface CaptureCharacterPendingReviewResponse {
  success: boolean;
  pending_id: string;
}
