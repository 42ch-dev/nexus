/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for DELETE /v1/daemon/memory/pending-review/{id}. The `{id}` path parameter is the pending review's `pending_id` (not modeled here); `creator_id` gates ownership.
 */
export interface DeletePendingReviewQuery {
  creator_id: string;
}
