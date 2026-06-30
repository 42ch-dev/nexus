import type { PaginationInfo } from '../kb/PaginationInfo';
import type { PendingReviewInfo } from './PendingReviewInfo';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListPendingReviewsResponse
 *
 * Response for GET /v1/local/memory/pending-review (cursor-based pagination). The `pagination` envelope reuses the shared `PaginationInfo`; `next_cursor` is the `pending_id` of the last item in the page (opaque to clients).
 *
 * @schema_version 1
 * @source list-pending-reviews-response.schema.json
 */
/** Response for GET /v1/local/memory/pending-review (cursor-based pagination). The `pagination` envelope reuses the shared `PaginationInfo`; `next_cursor` is the `pending_id` of the last item in the page (opaque to clients). */
export interface ListPendingReviewsResponse {
  items: PendingReviewInfo[];
  pagination: PaginationInfo;
}
