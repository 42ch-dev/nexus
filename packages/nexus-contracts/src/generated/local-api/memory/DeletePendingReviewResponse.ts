import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus DeletePendingReviewResponse
 *
 * Response body for DELETE /v1/local/memory/pending-review/{id}. Echoes the path `pending_id`; `success` is `true` on deletion (a missing or non-owned row surfaces as an error envelope, not `success: false`).
 *
 * @schema_version 1
 * @source delete-pending-review-response.schema.json
 */
/** Response body for DELETE /v1/local/memory/pending-review/{id}. Echoes the path `pending_id`; `success` is `true` on deletion (a missing or non-owned row surfaces as an error envelope, not `success: false`). */
export interface DeletePendingReviewResponse {
  success: boolean;
  pending_id: string;
}
