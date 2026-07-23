/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/memory/pending-review (cursor-based pagination). The `pagination` envelope reuses the shared `PaginationInfo`; `next_cursor` is the `pending_id` of the last item in the page (opaque to clients).
 */
export interface ListPendingReviewsResponse {
  items: NexusPendingReviewInfo[];
  pagination: NexusPaginationInfo;
}
/**
 * A single pending-review row in list/get responses. Mirrors the `memory_pending_review` table projection 1:1. `task_kind` and `created_at` are always present here (defaults are applied server-side on insert), unlike the create-request where they are optional. `world_id` is nullable.
 */
export interface NexusPendingReviewInfo {
  pending_id: string;
  session_id: string;
  creator_id: string;
  world_id?: string;
  task_kind: string;
  raw_digest: string;
  created_at: string;
}
/**
 * Cursor-based pagination metadata.
 */
export interface NexusPaginationInfo {
  limit: number;
  /**
   * Opaque cursor returned by the previous page. Clients MUST NOT parse it. Non-null only when another page exists.
   */
  next_cursor?: string;
  /**
   * True when the client may request another page (equivalent to `next_cursor` being non-null).
   */
  has_more: boolean;
}
