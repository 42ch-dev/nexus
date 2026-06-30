import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CountPendingReviewsResponse
 *
 * Response body for GET /v1/local/memory/pending-review/count. `count` is the number of pending-review rows for the creator.
 *
 * @schema_version 1
 * @source count-pending-reviews-response.schema.json
 */
/** Response body for GET /v1/local/memory/pending-review/count. `count` is the number of pending-review rows for the creator. */
export interface CountPendingReviewsResponse {
  count: number;
}
