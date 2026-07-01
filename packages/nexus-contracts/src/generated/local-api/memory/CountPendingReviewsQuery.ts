import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CountPendingReviewsQuery
 *
 * Query parameters for GET /v1/local/memory/pending-review/count.
 *
 * @schema_version 1
 * @source count-pending-reviews-query.schema.json
 */
/** Query parameters for GET /v1/local/memory/pending-review/count. */
export interface CountPendingReviewsQuery {
  creator_id: string;
}
