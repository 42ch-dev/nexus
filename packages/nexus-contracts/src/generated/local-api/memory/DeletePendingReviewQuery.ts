import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus DeletePendingReviewQuery
 *
 * Query parameters for DELETE /v1/local/memory/pending-review/{id}. The `{id}` path parameter is the pending review's `pending_id` (not modeled here); `creator_id` gates ownership.
 *
 * @schema_version 1
 * @source delete-pending-review-query.schema.json
 */
/** Query parameters for DELETE /v1/local/memory/pending-review/{id}. The `{id}` path parameter is the pending review's `pending_id` (not modeled here); `creator_id` gates ownership. */
export interface DeletePendingReviewQuery {
  creator_id: string;
}
