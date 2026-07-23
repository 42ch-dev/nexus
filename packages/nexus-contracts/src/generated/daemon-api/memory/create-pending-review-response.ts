/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/daemon/memory/pending-review. Echoes the request `pending_id`; `success` is always `true` (uses INSERT OR IGNORE so duplicate retries also return success).
 */
export interface CreatePendingReviewResponse {
  success: boolean;
  pending_id: string;
}
