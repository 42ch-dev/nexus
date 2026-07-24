/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for DELETE /v1/daemon/memory/pending-review/{id}. Echoes the path `pending_id`; `success` is `true` on deletion (a missing or non-owned row surfaces as an error envelope, not `success: false`).
 */
export interface DeletePendingReviewResponse {
  success: boolean;
  pending_id: string;
}
