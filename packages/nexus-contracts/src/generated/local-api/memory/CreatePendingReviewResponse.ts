import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreatePendingReviewResponse
 *
 * Response body for POST /v1/local/memory/pending-review. Echoes the request `pending_id`; `success` is always `true` (uses INSERT OR IGNORE so duplicate retries also return success).
 *
 * @schema_version 1
 * @source create-pending-review-response.schema.json
 */
/** Response body for POST /v1/local/memory/pending-review. Echoes the request `pending_id`; `success` is always `true` (uses INSERT OR IGNORE so duplicate retries also return success). */
export interface CreatePendingReviewResponse {
  success: boolean;
  pending_id: string;
}
