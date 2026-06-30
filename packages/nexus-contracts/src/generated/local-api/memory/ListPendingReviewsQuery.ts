import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListPendingReviewsQuery
 *
 * Query parameters for GET /v1/local/memory/pending-review. `limit` defaults to 50 (clamped 1..=250) when omitted; `cursor` is the opaque `next_cursor` from a previous page (cursor = pending_id).
 *
 * @schema_version 1
 * @source list-pending-reviews-query.schema.json
 */
/** Query parameters for GET /v1/local/memory/pending-review. `limit` defaults to 50 (clamped 1..=250) when omitted; `cursor` is the opaque `next_cursor` from a previous page (cursor = pending_id). */
export interface ListPendingReviewsQuery {
  creator_id: string;
  limit?: number;
  cursor?: string;
}
