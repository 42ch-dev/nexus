/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for DELETE /v1/daemon/characters/{character_id}/memory/pending-review/{pending_id}.
 */
export interface DeleteCharacterPendingReviewResponse {
  success: boolean;
  pending_id: string;
}
