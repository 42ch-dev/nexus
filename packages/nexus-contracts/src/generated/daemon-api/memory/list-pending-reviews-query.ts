/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/memory/pending-review. `limit` defaults to 50 (clamped 1..=250) when omitted; `cursor` is the opaque `next_cursor` from a previous page (cursor = pending_id).
 */
export interface ListPendingReviewsQuery {
  creator_id: string;
  limit?: number;
  cursor?: string;
}
